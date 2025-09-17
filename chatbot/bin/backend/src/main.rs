use anyhow::Result;
// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string())
});
static BACKEND_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string())
});

use axum::{
    extract::State,
    http::StatusCode,
    response::{Json, Sse},
    routing::{get, post},
    Router,
};
use chrono::Local;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::{sync::{mpsc, Mutex}, time::interval};
use tokio_stream::{wrappers::IntervalStream, StreamExt};
use tower_http::cors::{CorsLayer, Any};

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
    agent_sender: mpsc::Sender<String>,
    response_receiver: mpsc::Receiver<AgentMessage>,
    loading_receiver: mpsc::Receiver<LoadingProgress>,
    interrupt_sender: mpsc::Sender<()>,
}

impl WebChatState {
    pub async fn new(skip_docs: bool) -> Result<Self> {
        let (agent_sender, agent_receiver) = mpsc::channel(100);
        let (response_sender, response_receiver) = mpsc::channel(100);
        let (loading_sender, loading_receiver) = mpsc::channel(100);
        let (interrupt_sender, interrupt_receiver) = mpsc::channel(100);

        // Start the agent handler - same as TUI
        tokio::spawn(async move {
            let _ = agent::setup_agent_and_handle_messages(
                agent_receiver,
                response_sender,
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
            agent_sender,
            response_receiver,
            loading_receiver,
            interrupt_sender,
        })
    }

    pub async fn send_message(&mut self, message: String) -> Result<()> {
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
        if let Err(e) = self.agent_sender.send(message.to_string()).await {
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
        while let Ok(msg) = self.response_receiver.try_recv() {
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
                AgentMessage::System(msg) => {
                    self.add_system_message(&msg);
                }
                AgentMessage::McpConnected => {
                    self.add_system_message("MCP tools connected and ready");
                    self.is_connecting_mcp = false;
                }
                AgentMessage::McpConnecting(_) => {
                    self.add_system_message("Connecting to MCP tools...");
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
        }
    }
}

// API Types
#[derive(Deserialize)]
struct ChatRequest {
    message: String,
}

#[derive(Deserialize)]
struct SystemMessageRequest {
    message: String,
}

#[derive(Deserialize)]
struct McpCommandRequest {
    command: String,
    args: serde_json::Value,
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
}

type SharedChatState = Arc<Mutex<WebChatState>>;

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
    State(chat_state): State<SharedChatState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;
    
    if let Err(_) = state.send_message(request.message).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn state_endpoint(
    State(chat_state): State<SharedChatState>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}

async fn chat_stream(
    State(chat_state): State<SharedChatState>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let chat_state = Arc::clone(&chat_state);
            async move {
                let mut state = chat_state.lock().await;
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
    State(chat_state): State<SharedChatState>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;
    if let Err(_) = state.interrupt_processing().await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn system_message_endpoint(
    State(chat_state): State<SharedChatState>,
    Json(request): Json<SystemMessageRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let mut state = chat_state.lock().await;

    // Add system message to chat display
    state.add_system_message(&request.message);

    // Format message with [[SYSTEM:]] marker and send to agent for processing
    let system_message_for_agent = format!("[[SYSTEM:{}]]", request.message);

    // Send to agent (non-blocking, ignore errors as agent might be busy)
    let _ = state.agent_sender.try_send(system_message_for_agent);

    Ok(Json(state.get_state()))
}

async fn mcp_command_endpoint(
    State(chat_state): State<SharedChatState>,
    Json(request): Json<McpCommandRequest>,
) -> Result<Json<McpCommandResponse>, StatusCode> {
    let mut state = chat_state.lock().await;
    
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
            if let Err(e) = state.agent_sender.send(command_message).await {
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
    let cli = Cli::parse();
    
    // Initialize chat state
    let chat_state = Arc::new(Mutex::new(
        WebChatState::new(cli.no_docs).await?
    ));

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
        .with_state(chat_state);

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