use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

use aomi_agent::{ChatApp, ChatCommand, Message};

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
    pub total_list_items: usize,
    pub auto_scroll: bool,
    agent: Arc<ChatApp>,
    agent_history: Arc<RwLock<Vec<Message>>>,
    response_sender: mpsc::Sender<ChatCommand>,
    response_receiver: mpsc::Receiver<ChatCommand>,
    interrupt_sender: Option<mpsc::Sender<()>>,
}

impl App {
    pub async fn new(skip_docs: bool) -> Result<Self> {
        let agent = Arc::new(
            ChatApp::new(skip_docs)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        );
        let (response_sender, response_receiver) = mpsc::channel(1000);

        Ok(Self {
            messages: vec![],
            input: String::new(),
            scroll_offset: 0,
            is_processing: false,
            cursor_position: 0,
            spinner_index: 0,
            total_list_items: 0,
            auto_scroll: true,
            agent,
            agent_history: Arc::new(RwLock::new(Vec::new())),
            response_sender,
            response_receiver,
            interrupt_sender: None,
        })
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
                self.interrupt_processing().await?;
            }
            KeyCode::Enter if !self.is_processing => {
                if !self.input.trim().is_empty() {
                    self.send_message().await?;
                }
            }
            KeyCode::Char(c) if !self.is_processing => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace if !self.is_processing => {
                if self.cursor_position > 0 {
                    self.input.remove(self.cursor_position - 1);
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Delete if !self.is_processing => {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Left if !self.is_processing => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right if !self.is_processing => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home if !self.is_processing => {
                self.cursor_position = 0;
            }
            KeyCode::End if !self.is_processing => {
                self.cursor_position = self.input.len();
            }
            KeyCode::Up => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                    self.auto_scroll = false;
                }
            }
            KeyCode::Down => {
                let visible_height = 20;
                let max_offset = self.total_list_items.saturating_sub(visible_height);
                if self.scroll_offset < max_offset {
                    self.scroll_offset += 1;
                    self.auto_scroll = self.scroll_offset >= max_offset;
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                self.auto_scroll = false;
            }
            KeyCode::PageDown => {
                let visible_height = 20;
                let max_offset = self.total_list_items.saturating_sub(visible_height);
                self.scroll_offset = (self.scroll_offset + 10).min(max_offset);
                self.auto_scroll = self.scroll_offset >= max_offset;
            }
            _ => {}
        }
        Ok(false)
    }

    pub async fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<bool> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
                self.auto_scroll = false;
            }
            MouseEventKind::ScrollDown => {
                let visible_height = 20;
                let max_offset = self.total_list_items.saturating_sub(visible_height);
                self.scroll_offset = (self.scroll_offset + 3).min(max_offset);
                self.auto_scroll = self.scroll_offset >= max_offset;
            }
            _ => {}
        }
        Ok(false)
    }

    async fn send_message(&mut self) -> Result<()> {
        let message = std::mem::take(&mut self.input);
        if message.trim().is_empty() {
            return Ok(());
        }

        self.cursor_position = 0;
        self.add_user_message(&message);
        self.is_processing = true;
        self.auto_scroll = true;

        let agent = Arc::clone(&self.agent);
        let history = Arc::clone(&self.agent_history);
        let sender_to_ui = self.response_sender.clone();
        let (interrupt_tx, mut interrupt_rx) = mpsc::channel(1);
        self.interrupt_sender = Some(interrupt_tx);

        tokio::spawn(async move {
            let result = {
                let mut history_guard = history.write().await;
                agent
                    .process_message(
                        &mut history_guard,
                        message,
                        &sender_to_ui,
                        &mut interrupt_rx,
                    )
                    .await
            };

            if let Err(err) = result {
                let _ = sender_to_ui
                    .send(ChatCommand::Error(format!(
                        "Failed to process message: {err}"
                    )))
                    .await;
            }
        });

        self.add_assistant_message_streaming();
        Ok(())
    }

    async fn interrupt_processing(&mut self) -> Result<()> {
        if let Some(sender) = self.interrupt_sender.take() {
            let _ = sender.send(()).await;
            self.add_system_message("Interrupted by user (Esc)");
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn on_tick(&mut self) {
        if self.is_processing {
            self.spinner_index = (self.spinner_index + 1) % 10;
        }

        while let Ok(msg) = self.response_receiver.try_recv() {
            match msg {
                ChatCommand::StreamingText(text) => {
                    let needs_new_message = if let Some(last_msg) = self.messages.last() {
                        matches!(last_msg.sender, MessageSender::System)
                    } else {
                        true
                    };

                    if needs_new_message {
                        self.add_assistant_message_streaming();
                    }

                    if let Some(assistant_msg) = self
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| matches!(m.sender, MessageSender::Assistant))
                        && assistant_msg.is_streaming
                    {
                        assistant_msg.content.push_str(&text);
                    }
                }
                ChatCommand::ToolCall { name, args } => {
                    if let Some(assistant_msg) = self
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| matches!(m.sender, MessageSender::Assistant))
                    {
                        assistant_msg.is_streaming = false;
                    }

                    let tool_msg = format!("tool: {name} | args: {args}");
                    self.add_system_message(&tool_msg);
                }
                ChatCommand::Complete => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.is_streaming = false;
                    }
                    self.is_processing = false;
                    self.interrupt_sender = None;
                }
                ChatCommand::Error(err) => {
                    self.add_system_message(&format!("Error: {err}"));
                    self.is_processing = false;
                    self.interrupt_sender = None;
                }
                ChatCommand::System(msg) => {
                    self.add_system_message(&msg);
                }
                ChatCommand::BackendConnected => {
                    self.add_system_message("Agent ready");
                }
                ChatCommand::BackendConnecting(msg) => {
                    self.add_system_message(&msg);
                }
                ChatCommand::MissingApiKey => {
                    self.add_system_message(
                        "Anthropic API key missing. Set ANTHROPIC_API_KEY and restart.",
                    );
                    self.is_processing = false;
                    self.interrupt_sender = None;
                }
                ChatCommand::WalletTransactionRequest(tx) => {
                    self.add_system_message(&format!("Transaction request pending approval: {tx}"));
                }
                ChatCommand::Interrupted => {
                    if let Some(last_msg) = self.messages.last_mut()
                        && matches!(last_msg.sender, MessageSender::Assistant)
                    {
                        last_msg.is_streaming = false;
                    }
                    self.is_processing = false;
                    self.interrupt_sender = None;
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
        self.messages.push(ChatMessage {
            sender: MessageSender::System,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }
}
