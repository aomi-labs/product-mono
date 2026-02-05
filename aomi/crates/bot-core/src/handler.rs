//! Core message handling infrastructure.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tracing::{debug, warn};

use aomi_backend::{MessageSender, NamespaceAuth, SessionManager, SessionResponse};

use crate::error::{BotError, BotResult};
use crate::types::{BotMessage, BotResponse};

/// Default timeout for waiting for LLM response.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Default interval for refreshing typing indicator.
pub const DEFAULT_TYPING_INTERVAL: Duration = Duration::from_secs(4);

/// Configuration for response polling.
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// Maximum time to wait for a response.
    pub timeout: Duration,
    /// Interval at which to refresh typing indicator.
    pub typing_interval: Duration,
    /// Interval at which to poll for completion.
    pub poll_interval: Duration,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            typing_interval: DEFAULT_TYPING_INTERVAL,
            poll_interval: Duration::from_millis(100),
        }
    }
}

/// Trait for sending platform-specific actions (typing indicator, etc.)
#[async_trait]
pub trait PlatformActions: Send + Sync {
    /// Send a typing indicator to the chat.
    async fn send_typing(&self, chat_id: &str) -> BotResult<()>;
}

/// Polls for LLM response completion with typing indicator refresh.
pub struct ResponsePoller<P> {
    session_manager: Arc<SessionManager>,
    platform: P,
    config: PollerConfig,
}

impl<P: PlatformActions> ResponsePoller<P> {
    /// Create a new response poller.
    pub fn new(session_manager: Arc<SessionManager>, platform: P) -> Self {
        Self {
            session_manager,
            platform,
            config: PollerConfig::default(),
        }
    }

    /// Create a new response poller with custom config.
    pub fn with_config(
        session_manager: Arc<SessionManager>,
        platform: P,
        config: PollerConfig,
    ) -> Self {
        Self {
            session_manager,
            platform,
            config,
        }
    }

    /// Process a message and wait for the response.
    ///
    /// This method:
    /// 1. Gets or creates a session for the given key
    /// 2. Sends the user input to the session
    /// 3. Polls until processing is complete (with typing indicator refresh)
    /// 4. Extracts and returns the assistant's response
    pub async fn process_message(
        &self,
        session_key: &str,
        chat_id: &str,
        text: &str,
    ) -> BotResult<BotResponse> {
        // Get or create session with default namespace authorization
        let mut auth = NamespaceAuth::new(None, None, None);
        let session = self
            .session_manager
            .get_or_create_session(session_key, &mut auth, None)
            .await
            .map_err(|e| BotError::Session(e.to_string()))?;

        // Send typing indicator
        let _ = self.platform.send_typing(chat_id).await;

        let mut state = session.lock().await;

        debug!("Sending user input to session {}: {:?}", session_key, text);
        state
            .send_user_input(text.to_string())
            .await
            .map_err(|e| BotError::Session(e.to_string()))?;

        // Poll until processing is complete
        let start = Instant::now();
        let mut last_typing = Instant::now();

        loop {
            state.sync_state().await;
            let response = state.format_session_response(None);

            if !response.is_processing {
                debug!(
                    "Processing complete for session {} after {:?}",
                    session_key,
                    start.elapsed()
                );
                break;
            }

            if start.elapsed() > self.config.timeout {
                warn!(
                    "Timeout waiting for response in session {} after {:?}",
                    session_key,
                    start.elapsed()
                );
                return Err(BotError::Timeout);
            }

            // Refresh typing indicator periodically
            if last_typing.elapsed() > self.config.typing_interval {
                let _ = self.platform.send_typing(chat_id).await;
                last_typing = Instant::now();
            }

            // Release lock briefly to allow processing
            drop(state);
            tokio::time::sleep(self.config.poll_interval).await;
            state = session.lock().await;
        }

        let response = state.format_session_response(None);
        let assistant_text = extract_assistant_text(&response);

        if assistant_text.is_empty() {
            warn!(
                "No assistant response generated for session {}",
                session_key
            );
            return Err(BotError::EmptyResponse);
        }

        Ok(BotResponse::new(assistant_text))
    }
}

/// Extract the last assistant message from a session response.
pub fn extract_assistant_text(response: &SessionResponse) -> String {
    response
        .messages
        .iter()
        .rfind(|m| matches!(m.sender, MessageSender::Assistant))
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// Trait for handling messages from chat platforms.
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message and return a response.
    async fn handle(&self, message: BotMessage) -> BotResult<Option<BotResponse>>;
}
