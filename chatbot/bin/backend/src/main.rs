use anyhow::Result;
// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string())
});
static BACKEND_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string())
});

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Json, Sse},
    routing::{get, post},
    Router,
};
use chrono::Local;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::{Duration, Instant}};
use tokio::{sync::{mpsc, Mutex, RwLock}, time::interval};
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use tower_http::cors::{CorsLayer, Any};
use uuid::Uuid;

use agent::{AgentMessage, LoadingProgress};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum MessageSender {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "agent")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub sender: MessageSender,
    pub content: String,
    pub timestamp: String,
    pub is_streaming: bool,
}

pub struct WebChatState {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub is_loading: bool,
    pub is_connecting_mcp: bool,
    pub missing_api_key: bool,
    pub pending_wallet_tx: Option<String>, // JSON string of pending transaction
    sender_to_llm: mpsc::Sender<String>, // backend -> agent
    receiver_from_llm: mpsc::Receiver<AgentMessage>, // agent -> backend
    loading_receiver: mpsc::Receiver<LoadingProgress>,
    interrupt_sender: mpsc::Sender<()>,
}

impl WebChatState {
    pub async fn new(skip_docs: bool) -> Result<Self> {
        // llm <- backend <- ui
        let (sender_to_llm, receiver_from_ui) = mpsc::channel(100);
        // ui <- backend <- llm
        let (sender_to_ui, receiver_from_llm) = mpsc::channel(100);
        let (loading_sender, loading_receiver) = mpsc::channel(100);
        let (interrupt_sender, interrupt_receiver) = mpsc::channel(100);

        // Start the agent handler - same as TUI
        tokio::spawn(async move {
            let _ = agent::setup_agent_and_handle_messages(
                receiver_from_ui,
                sender_to_ui,
                loading_sender,
                interrupt_receiver,
                skip_docs,
            )
            .await;
        });

        Ok(Self {
            messages: vec![],
            is_processing: false,
            is_loading: true,
            is_connecting_mcp: true,
            missing_api_key: false,
            pending_wallet_tx: None,
            sender_to_llm,
            receiver_from_llm,
            loading_receiver,
            interrupt_sender,
        })
    }

    pub async fn process_message_from_ui(&mut self, message: String) -> Result<()> {
        if self.is_processing || self.is_loading || self.is_connecting_mcp || self.missing_api_key {
            return Ok(());
        }

        let message = message.trim();
        if message.is_empty() {
            return Ok(());
        }

        // Add user message (matching TUI pattern)
        self.add_user_message(message);

        // Set processing state
        self.is_processing = true;

        // Send to agent with error handling
        if let Err(e) = self.sender_to_llm.send(message.to_string()).await {
            self.add_system_message(&format!(
                "Failed to send message: {e}. Agent may have disconnected."
            ));
            self.is_processing = false;
            return Ok(());
        }

        // Start receiving streaming response
        self.add_assistant_message_streaming();

        Ok(())
    }

    pub async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            if self.interrupt_sender.send(()).await.is_err() {
                self.add_system_message("Failed to interrupt: agent not responding");
            } else {
                self.add_system_message("Interrupted by user");
            }
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn update_state(&mut self) {
        // Check for loading progress (matching TUI)
        while let Ok(progress) = self.loading_receiver.try_recv() {
            match progress {
                LoadingProgress::Complete => {
                    self.is_loading = false;
                }
                _ => {}
            }
        }

        // Check for agent responses (matching TUI logic exactly)
        while let Ok(msg) = self.receiver_from_llm.try_recv() {
            // eprintln!("🔍 self.receiver_from_llm received message: {:?}", msg);
            match msg {
                AgentMessage::StreamingText(text) => {
                    // Check if we need to create a new assistant message
                    let needs_new_message = if let Some(last_msg) = self.messages.last() {
                        matches!(last_msg.sender, MessageSender::System)
                    } else {
                        true
                    };

                    if needs_new_message {
                        self.add_assistant_message_streaming();
                    }

                    // Append to the last assistant message
                    if let Some(assistant_msg) = self
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| matches!(m.sender, MessageSender::Assistant))
                    {
                        if assistant_msg.is_streaming {
                            assistant_msg.content.push_str(&text);
                        }
                    }
                }
                AgentMessage::ToolCall { name, args } => {
                    // Mark current assistant message as complete before tool call
                    if let Some(assistant_msg) = self
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| matches!(m.sender, MessageSender::Assistant))
                    {
                        assistant_msg.is_streaming = false;
                    }

                    // Display actual tool call in the UI
                    let tool_msg = format!("tool: {name} | args: {args}");
                    self.add_system_message(&tool_msg);
                }
                AgentMessage::Complete => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.is_streaming = false;
                    }
                    self.is_processing = false;
                }
                AgentMessage::Error(err) => {
                    self.add_system_message(&format!("Error: {err}"));
                    self.is_processing = false;
                }
                AgentMessage::WalletTransactionRequest(tx_json) => {
                    // Store the pending transaction for the frontend to pick up
                    self.pending_wallet_tx = Some(tx_json.clone());

                    // Add a system message to inform the agent
                    self.add_system_message("Transaction request sent to user's wallet. Waiting for user approval or rejection.");
                }
                AgentMessage::System(msg) => {
                    self.add_system_message(&msg);
                }
                AgentMessage::BackendConnected => {
                    self.add_system_message("All backend services connected and ready");
                    self.is_connecting_mcp = false;
                    self.is_loading = false; // Clear loading state when MCP is connected
                }
                AgentMessage::BackendConnecting(s) => {
                    self.add_system_message(&format!("{s}"));
                    // Keep connecting state
                }
                AgentMessage::MissingApiKey => {
                    self.missing_api_key = true;
                    self.is_connecting_mcp = false;
                    self.is_loading = false;
                }
                AgentMessage::Interrupted => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        if matches!(last_msg.sender, MessageSender::Assistant) {
                            last_msg.is_streaming = false;
                        }
                    }
                    self.is_processing = false;
                }
            }
        }
    }

    fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::User,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    fn add_assistant_message_streaming(&mut self) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
    }

    fn add_system_message(&mut self, content: &str) {
        // Check if this exact system message already exists in recent messages
        // Look at the last 5 messages to avoid distant duplicates but catch immediate ones
        let recent_messages = self.messages.iter().rev().take(5);
        let has_duplicate = recent_messages
            .filter(|msg| matches!(msg.sender, MessageSender::System))
            .any(|msg| msg.content == content);

        if !has_duplicate {
            self.messages.push(ChatMessage {
                sender: MessageSender::System,
                content: content.to_string(),
                timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
                is_streaming: false,
            });
        }
    }

    pub fn get_state(&self) -> WebStateResponse {
        WebStateResponse {
            messages: self.messages.clone(),
            is_processing: self.is_processing,
            is_loading: self.is_loading,
            is_connecting_mcp: self.is_connecting_mcp,
            missing_api_key: self.missing_api_key,
            pending_wallet_tx: self.pending_wallet_tx.clone(),
        }
    }

    pub fn clear_pending_wallet_tx(&mut self) {
        self.pending_wallet_tx = None;
    }
}

// Session Management
struct SessionData {
    state: Arc<Mutex<WebChatState>>,
    last_activity: Instant,
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionData>>>,
    cleanup_interval: Duration,
    session_timeout: Duration,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            session_timeout: Duration::from_secs(1800), // 30 minutes
        }
    }

    pub async fn get_or_create_session(&self, session_id: &str, skip_docs: bool) -> Result<Arc<Mutex<WebChatState>>, anyhow::Error> {
        let mut sessions = self.sessions.write().await;

        if let Some(session_data) = sessions.get_mut(session_id) {
            // Update last activity and return existing session
            session_data.last_activity = Instant::now();
            Ok(session_data.state.clone())
        } else {
            // Create new session
            let web_chat_state = WebChatState::new(skip_docs).await?;
            let session_data = SessionData {
                state: Arc::new(Mutex::new(web_chat_state)),
                last_activity: Instant::now(),
            };
            let state_clone = session_data.state.clone();
            sessions.insert(session_id.to_string(), session_data);
            println!("📝 Created new session: {}", session_id);
            Ok(state_clone)
        }
    }

    pub async fn cleanup_inactive_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();

        sessions.retain(|session_id, session_data| {
            let should_keep = now.duration_since(session_data.last_activity) < self.session_timeout;
            if !should_keep {
                println!("🗑️ Cleaning up inactive session: {}", session_id);
            }
            should_keep
        });
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            println!("🗑️ Manually removed session: {}", session_id);
        }
    }

    pub async fn start_cleanup_task(self: Arc<Self>) {
        let cleanup_manager = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_manager.cleanup_interval);
            loop {
                interval.tick().await;
                cleanup_manager.cleanup_inactive_sessions().await;
            }
        });
    }

    #[allow(dead_code)]
    pub async fn get_active_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

// Helper function to generate session IDs
fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}

// API Types
#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct SystemMessageRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct McpCommandRequest {
    command: String,
    args: serde_json::Value,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct InterruptRequest {
    session_id: Option<String>,
}

#[derive(Serialize)]
struct McpCommandResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct WebStateResponse {
    messages: Vec<ChatMessage>,
    is_processing: bool,
    is_loading: bool,
    is_connecting_mcp: bool,
    missing_api_key: bool,
    pending_wallet_tx: Option<String>,
}

type SharedSessionManager = Arc<SessionManager>;

#[derive(Parser)]
#[command(name = "backend")]
#[command(about = "Web backend for EVM chatbot")]
struct Cli {
    /// Skip loading Uniswap documentation at startup
    #[arg(long)]
    no_docs: bool,
}

// HTTP Handlers
async fn health() -> &'static str {
    "OK"
}

async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    if let Err(_) = state.process_message_from_ui(request.message).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    // Use session_id from query params or generate new one for backward compatibility
    let session_id = params.get("session_id").cloned().unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}

async fn chat_stream(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    // Use session_id from query params or generate new one for backward compatibility
    let session_id = params.get("session_id").cloned().unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => {
            // Return simple error stream - just create a dummy session for error case
            let dummy_state = Arc::new(Mutex::new(
                WebChatState::new(false).await.unwrap_or_else(|_| {
                    // This is a fallback - should not happen in practice
                    panic!("Failed to create even a fallback session")
                })
            ));
            dummy_state
        }
    };

    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let session_state = Arc::clone(&session_state);
            async move {
                let mut state = session_state.lock().await;
                state.update_state().await;
                let response = state.get_state();

                axum::response::sse::Event::default()
                    .json_data(&response)
                    .map_err(|_| ())
            }
        })
        .then(|f| f)
        .map(|result| result.map_err(|_| unreachable!()));

    Sse::new(stream)
}

async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<InterruptRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if let Err(_) = state.interrupt_processing().await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn system_message_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<SystemMessageRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    // Add system message to chat display
    state.add_system_message(&request.message);

    // Format message with [[SYSTEM:]] marker and send to agent for processing
    let system_message_for_agent = format!("[[SYSTEM:{}]]", request.message);

    // Send to agent (non-blocking, ignore errors as agent might be busy)
    let _ = state.sender_to_llm.try_send(system_message_for_agent);

    Ok(Json(state.get_state()))
}

async fn mcp_command_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<McpCommandRequest>,
) -> Result<Json<McpCommandResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(|| generate_session_id());

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    // Handle different MCP commands
    match request.command.as_str() {
        "set_network" => {
            // Extract network name from args
            let network_name = request.args
                .get("network")
                .and_then(|v| v.as_str())
                .unwrap_or("testnet");

            // Create the set_network command message
            let command_message = format!("set_network {}", network_name);

            // Send the command through the agent
            if let Err(e) = state.sender_to_llm.send(command_message).await {
                return Ok(Json(McpCommandResponse {
                    success: false,
                    message: format!("Failed to send command to agent: {}", e),
                    data: None,
                }));
            }

            // Add system message to indicate network switch attempt
            state.add_system_message(&format!("🔄 Attempting to switch network to {}", network_name));

            Ok(Json(McpCommandResponse {
                success: true,
                message: format!("Network switch to {} initiated", network_name),
                data: Some(serde_json::json!({ "network": network_name })),
            }))
        }
        _ => {
            Ok(Json(McpCommandResponse {
                success: false,
                message: format!("Unknown command: {}", request.command),
                data: None,
            }))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    // Initialize session manager
    let session_manager = Arc::new(SessionManager::new());

    // Start cleanup task
    let cleanup_manager = Arc::clone(&session_manager);
    cleanup_manager.start_cleanup_task().await;

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        .with_state(session_manager);

    // Get host and port from environment variables or use defaults
    let host = &*BACKEND_HOST;
    let port = &*BACKEND_PORT;
    let bind_addr = format!("{}:{}", host, port);

    println!("🚀 Backend server starting on http://{}", bind_addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_manager_create_session() {
        let session_manager = SessionManager::new();

        let session_id = "test-session-1";
        let session_state = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to create session");

        // Verify we got a session state
        let state = session_state.lock().await;
        assert_eq!(state.messages.len(), 0);
        assert!(state.is_loading); // Should start loading
    }

    #[tokio::test]
    async fn test_session_manager_multiple_sessions() {
        let session_manager = SessionManager::new();

        // Create two different sessions
        let session1_id = "test-session-1";
        let session2_id = "test-session-2";

        let session1_state = session_manager
            .get_or_create_session(session1_id, true)
            .await
            .expect("Failed to create session 1");

        let session2_state = session_manager
            .get_or_create_session(session2_id, true)
            .await
            .expect("Failed to create session 2");

        // Verify they are different instances
        assert_ne!(
            Arc::as_ptr(&session1_state),
            Arc::as_ptr(&session2_state),
            "Sessions should be different instances"
        );

        // Verify session count
        assert_eq!(session_manager.get_active_session_count().await, 2);
    }

    #[tokio::test]
    async fn test_session_manager_reuse_session() {
        let session_manager = SessionManager::new();

        let session_id = "test-session-reuse";

        // Create session first time
        let session_state_1 = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to create session first time");

        // Get session second time
        let session_state_2 = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to get session second time");

        // Should be the same instance
        assert_eq!(
            Arc::as_ptr(&session_state_1),
            Arc::as_ptr(&session_state_2),
            "Should reuse existing session"
        );

        // Verify session count is still 1
        assert_eq!(session_manager.get_active_session_count().await, 1);
    }

    #[tokio::test]
    async fn test_session_manager_remove_session() {
        let session_manager = SessionManager::new();

        let session_id = "test-session-remove";

        // Create session
        let _session_state = session_manager
            .get_or_create_session(session_id, true)
            .await
            .expect("Failed to create session");

        assert_eq!(session_manager.get_active_session_count().await, 1);

        // Remove session
        session_manager.remove_session(session_id).await;

        // Verify session is removed
        assert_eq!(session_manager.get_active_session_count().await, 0);
    }

    #[tokio::test]
    async fn test_generate_session_id_uniqueness() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        assert_ne!(id1, id2, "Session IDs should be unique");
        assert!(!id1.is_empty(), "Session ID should not be empty");
        assert!(!id2.is_empty(), "Session ID should not be empty");
    }
}