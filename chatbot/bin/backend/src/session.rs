use anyhow::Result;
// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("BACKEND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()));
static BACKEND_PORT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string()));

use chrono::Local;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

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

#[derive(Serialize)]
pub struct SessionResponse {
    messages: Vec<ChatMessage>,
    is_processing: bool,
    is_loading: bool,
    is_connecting_mcp: bool,
    missing_api_key: bool,
    pending_wallet_tx: Option<String>,
}

pub struct SessionState {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub is_loading: bool,
    pub is_connecting_mcp: bool,
    pub missing_api_key: bool,
    pub pending_wallet_tx: Option<String>, // JSON string of pending transaction
    pub(crate) sender_to_llm: mpsc::Sender<String>, // backend -> agent
    pub(crate) receiver_from_llm: mpsc::Receiver<AgentMessage>, // agent -> backend
    pub(crate) loading_receiver: mpsc::Receiver<LoadingProgress>,
    pub(crate) interrupt_sender: mpsc::Sender<()>,
}

impl SessionState {
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

    pub async fn process_msg_from_ui(&mut self, message: String) -> Result<()> {
        if self.is_processing || self.is_loading || self.is_connecting_mcp || self.missing_api_key {
            return Ok(());
        }

        let message = message.trim();
        if message.is_empty() {
            return Ok(());
        }

        // Add user message (matching TUI pattern)
        self.append_user(message);

        // Set processing state
        self.is_processing = true;

        // Send to agent with error handling
        if let Err(e) = self.sender_to_llm.send(message.to_string()).await {
            self.append_system(&format!("Failed to send message: {e}. Agent may have disconnected."));
            self.is_processing = false;
            return Ok(());
        }

        // Start receiving streaming response
        self.append_assistant();

        Ok(())
    }

    pub async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            if self.interrupt_sender.send(()).await.is_err() {
                self.append_system("Failed to interrupt: agent not responding");
            } else {
                self.append_system("Interrupted by user");
            }
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn update_state(&mut self) {
        // Check for loading progress (matching TUI)
        while let Ok(progress) = self.loading_receiver.try_recv() {
            if let LoadingProgress::Complete = progress {
                self.is_loading = false;
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
                        self.append_assistant();
                    }

                    // Append to the last assistant message
                    if let Some(assistant_msg) =
                        self.messages.iter_mut().rev().find(|m| matches!(m.sender, MessageSender::Assistant))
                    {
                        if assistant_msg.is_streaming {
                            assistant_msg.content.push_str(&text);
                        }
                    }
                }
                AgentMessage::ToolCall { name, args } => {
                    // Mark current assistant message as complete before tool call
                    if let Some(assistant_msg) =
                        self.messages.iter_mut().rev().find(|m| matches!(m.sender, MessageSender::Assistant))
                    {
                        assistant_msg.is_streaming = false;
                    }

                    // Display actual tool call in the UI
                    let tool_msg = format!("tool: {name} | args: {args}");
                    self.append_system(&tool_msg);
                }
                AgentMessage::Complete => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.is_streaming = false;
                    }
                    self.is_processing = false;
                }
                AgentMessage::Error(err) => {
                    self.append_system(&format!("Error: {err}"));
                    self.is_processing = false;
                }
                AgentMessage::WalletTransactionRequest(tx_json) => {
                    // Store the pending transaction for the frontend to pick up
                    self.pending_wallet_tx = Some(tx_json.clone());

                    // Add a system message to inform the agent
                    self.append_system(
                        "Transaction request sent to user's wallet. Waiting for user approval or rejection.",
                    );
                }
                AgentMessage::System(msg) => {
                    self.append_system(&msg);
                }
                AgentMessage::McpConnected => {
                    self.append_system("MCP tools connected and ready");
                    self.is_connecting_mcp = false;
                }
                AgentMessage::McpConnecting(_) => {
                    self.append_system("Connecting to MCP tools...");
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

    pub(crate) fn append_user(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::User,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub(crate) fn append_assistant(&mut self) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
    }

    pub(crate) fn append_system(&mut self, content: &str) {
        // Check if this exact system message already exists in recent messages
        // Look at the last 5 messages to avoid distant duplicates but catch immediate ones
        let recent_messages = self.messages.iter().rev().take(5);
        let has_duplicate =
            recent_messages.filter(|msg| matches!(msg.sender, MessageSender::System)).any(|msg| msg.content == content);

        if !has_duplicate {
            self.messages.push(ChatMessage {
                sender: MessageSender::System,
                content: content.to_string(),
                timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
                is_streaming: false,
            });
        }
    }

    pub(crate) fn get_state(&self) -> SessionResponse {
        SessionResponse {
            messages: self.messages.clone(),
            is_processing: self.is_processing,
            is_loading: self.is_loading,
            is_connecting_mcp: self.is_connecting_mcp,
            missing_api_key: self.missing_api_key,
            pending_wallet_tx: self.pending_wallet_tx.clone(),
        }
    }

    pub(crate) fn clear_pending_wallet_tx(&mut self) {
        self.pending_wallet_tx = None;
    }
}
