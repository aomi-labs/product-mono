use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tokio::sync::mpsc;

use agent::{AgentMessage, LoadingProgress};

#[derive(Debug, Clone, PartialEq)]
pub enum MessageSender {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub sender: MessageSender,
    pub content: String,
    pub timestamp: String,
    pub is_streaming: bool,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub scroll_offset: usize,
    pub is_processing: bool,
    pub cursor_position: usize,
    pub spinner_index: usize,
    pub total_list_items: usize,        // Track total items for scrolling
    pub auto_scroll: bool,              // Track if we should auto-scroll
    pub is_loading: bool,               // Track if documents are loading
    pub loading_messages: Vec<String>,  // Store loading progress messages
    pub is_connecting_mcp: bool,        // Track if waiting for MCP server connection
    pub mcp_connection_message: String, // Message about MCP connection status
    pub missing_api_key: bool,          // Track if Anthropic API key is missing
    agent_sender: mpsc::Sender<String>,
    response_receiver: mpsc::Receiver<AgentMessage>,
    loading_receiver: mpsc::Receiver<LoadingProgress>,
    interrupt_sender: mpsc::Sender<()>,
}

impl App {
    pub async fn new(skip_docs: bool) -> Result<Self> {
        let (agent_sender, agent_receiver) = mpsc::channel(100);
        let (response_sender, response_receiver) = mpsc::channel(100);
        let (loading_sender, loading_receiver) = mpsc::channel(100);
        let (interrupt_sender, interrupt_receiver) = mpsc::channel(100);

        // Start loading immediately
        let app = Self {
            messages: vec![],
            input: String::new(),
            scroll_offset: 0,
            is_processing: false,
            cursor_position: 0,
            spinner_index: 0,
            total_list_items: 0,
            auto_scroll: true,
            is_loading: true,
            loading_messages: vec![],
            is_connecting_mcp: true,
            mcp_connection_message: "Connecting to MCP server...".to_string(),
            missing_api_key: false,
            agent_sender,
            response_receiver,
            loading_receiver,
            interrupt_sender,
        };

        // Spawn the agent handler with error handling
        tokio::spawn(async move {
            // Move the entire agent setup into this task to keep the client alive
            let _ = agent::setup_agent_and_handle_messages(
                agent_receiver,
                response_sender,
                loading_sender,
                interrupt_receiver,
                skip_docs,
            )
            .await;
            // Don't print errors to stderr - the UI popups handle user-facing errors
        });

        // No welcome message

        Ok(app)
    }

    pub async fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            KeyCode::Esc if self.is_processing => {
                // Interrupt the current LLM processing
                self.interrupt_processing().await?;
            }
            KeyCode::Enter
                if !self.is_processing
                    && !self.is_loading
                    && !self.is_connecting_mcp
                    && !self.missing_api_key =>
            {
                if !self.input.trim().is_empty() {
                    self.send_message().await?;
                }
            }
            KeyCode::Char(c)
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                if self.cursor_position > 0 {
                    self.input.remove(self.cursor_position - 1);
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Delete
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Left
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                self.cursor_position = 0;
            }
            KeyCode::End
                if !self.is_loading && !self.is_connecting_mcp && !self.missing_api_key =>
            {
                self.cursor_position = self.input.len();
            }
            KeyCode::Up => {
                // Scroll up one line
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                    self.auto_scroll = false; // User manually scrolled
                }
            }
            KeyCode::Down => {
                // Scroll down one line
                let visible_height = 20; // Approximate visible height
                let max_offset = self.total_list_items.saturating_sub(visible_height);
                if self.scroll_offset < max_offset {
                    self.scroll_offset += 1;
                    // Check if we're at the bottom again
                    self.auto_scroll = self.scroll_offset >= max_offset;
                }
            }
            KeyCode::PageUp => {
                // Scroll up by 10
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                self.auto_scroll = false; // User manually scrolled
            }
            KeyCode::PageDown => {
                // Scroll down by 10
                let visible_height = 20;
                let max_offset = self.total_list_items.saturating_sub(visible_height);
                self.scroll_offset = (self.scroll_offset + 10).min(max_offset);
                // Check if we're at the bottom again
                self.auto_scroll = self.scroll_offset >= max_offset;
            }
            _ => {}
        }
        Ok(false)
    }

    pub async fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<bool> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                // Scroll up 3 lines (reasonable scroll speed)
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
                self.auto_scroll = false; // User manually scrolled
            }
            MouseEventKind::ScrollDown => {
                // Scroll down 3 lines
                let visible_height = 20; // Approximate visible height
                let max_offset = self.total_list_items.saturating_sub(visible_height);
                self.scroll_offset = (self.scroll_offset + 3).min(max_offset);
                // Check if we're at the bottom again
                self.auto_scroll = self.scroll_offset >= max_offset;
            }
            _ => {
                // Ignore other mouse events (clicks, moves, etc.)
            }
        }
        Ok(false)
    }

    async fn send_message(&mut self) -> Result<()> {
        let message = self.input.clone();
        self.input.clear();
        self.cursor_position = 0;

        // Add user message
        self.add_user_message(&message);

        // Set processing state and enable auto-scroll
        self.is_processing = true;
        self.auto_scroll = true;

        // Send to agent with error handling
        if let Err(e) = self.agent_sender.send(message).await {
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

    async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            // Send interrupt signal to the agent
            if self.interrupt_sender.send(()).await.is_err() {
                // Channel closed, agent is likely shutting down
                self.add_system_message("Failed to interrupt: agent not responding");
            } else {
                self.add_system_message("Interrupted by user (Esc)");
            }
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn on_tick(&mut self) {
        // Update spinner animation
        if self.is_processing || self.is_loading || self.is_connecting_mcp {
            self.spinner_index = (self.spinner_index + 1) % 10;
        }

        // Check for loading progress
        while let Ok(progress) = self.loading_receiver.try_recv() {
            match progress {
                LoadingProgress::Message(msg) => {
                    self.loading_messages.push(msg);
                }
                LoadingProgress::Complete => {
                    self.is_loading = false;
                    self.loading_messages.clear();
                }
            }
        }

        // Check for agent responses
        while let Ok(msg) = self.response_receiver.try_recv() {
            match msg {
                AgentMessage::WalletTransactionRequest(_) => todo!(),
                AgentMessage::StreamingText(text) => {
                    // Check if we need to create a new assistant message
                    // This happens after tool calls
                    let needs_new_message = if let Some(last_msg) = self.messages.last() {
                        // Create new message if last was a system message (tool call/result)
                        matches!(last_msg.sender, MessageSender::System)
                    } else {
                        true // No messages yet
                    };

                    if needs_new_message {
                        // Create a new assistant message for post-tool response
                        self.add_assistant_message_streaming();
                    }

                    // Now append to the last assistant message
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
                    // A new assistant message will be created when streaming resumes
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
                    self.is_connecting_mcp = false;
                }
                AgentMessage::McpConnecting(msg) => {
                    self.mcp_connection_message = msg;
                }
                AgentMessage::MissingApiKey => {
                    self.missing_api_key = true;
                    self.is_connecting_mcp = false;
                    self.is_loading = false;
                }
                AgentMessage::Interrupted => {
                    // Mark current assistant message as complete since it was interrupted
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
        // Don't auto-scroll here, let the UI handle it
    }

    fn add_assistant_message_streaming(&mut self) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
        // Don't auto-scroll here, let the UI handle it
    }

    fn add_system_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::System,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
        // Don't auto-scroll here, let the UI handle it
    }
}
