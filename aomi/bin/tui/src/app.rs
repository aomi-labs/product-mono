use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::sync::Arc;

use aomi_chat::ChatApp;
use aomi_l2beat::L2BeatApp;
use aomi_backend::{SessionState, session::DefaultSessionState, session::ChatBackend};
use aomi_chat::ToolResultStream;

pub use aomi_backend::{ChatMessage, MessageSender};

static LOAD_L2B: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct SessionContainer {
    pub session: DefaultSessionState,
    pub input: String,
    pub scroll_offset: usize,
    pub cursor_position: usize,
    pub spinner_index: usize,
    pub total_list_items: usize,
    pub auto_scroll: bool,
    chat_backend: Arc<dyn ChatBackend<ToolResultStream>>,
    l2b_backend: Option<Arc<dyn ChatBackend<ToolResultStream>>>,
    current_is_l2b: bool,
}

impl SessionContainer {
    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        let l2b_app = Arc::new(
            L2BeatApp::new_with_options(skip_docs, skip_mcp)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        );

        let chat_app = Arc::new(
            ChatApp::new_with_options(skip_docs, skip_mcp)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        );
        
        let chat_backend = chat_app as Arc<dyn ChatBackend<ToolResultStream>>;
        let l2b_backend = Some(l2b_app as Arc<dyn ChatBackend<ToolResultStream>>);
        let session = SessionState::new(chat_backend.clone(), Vec::new()).await?;

        Ok(Self {
            session,
            input: String::new(),
            scroll_offset: 0,
            cursor_position: 0,
            spinner_index: 0,
            total_list_items: 0,
            auto_scroll: true,
            chat_backend,
            l2b_backend,
            current_is_l2b: false,
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
            KeyCode::Esc if self.session.is_processing => {
                self.interrupt_processing().await?;
            }
            KeyCode::Enter if !self.session.is_processing => {
                if !self.input.trim().is_empty() {
                    self.send_message().await?;
                }
            }
            KeyCode::Char(c) if !self.session.is_processing => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace if !self.session.is_processing => {
                if self.cursor_position > 0 {
                    self.input.remove(self.cursor_position - 1);
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Delete if !self.session.is_processing => {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Left if !self.session.is_processing => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right if !self.session.is_processing => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home if !self.session.is_processing => {
                self.cursor_position = 0;
            }
            KeyCode::End if !self.session.is_processing => {
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
        self.auto_scroll = true;

        let load_l2b = if message.contains("l2beat-magic") {
            LOAD_L2B.store(true, std::sync::atomic::Ordering::Relaxed);
            true
        } else if message.contains("l2b-magic-off") {
            LOAD_L2B.store(false, std::sync::atomic::Ordering::Relaxed);
            false
        } else {
            LOAD_L2B.load(std::sync::atomic::Ordering::Relaxed)
        };

        // Switch to l2b backend only when requested and not already using it
        if load_l2b && !self.current_is_l2b && self.l2b_backend.is_some() {
            tracing::info!("switching to l2b backend");
            // Filter messages to only include user/assistant conversation, exclude system/tool messages
            let filtered_messages: Vec<_> = self.session.messages
                .iter()
                .filter(|msg| matches!(msg.sender, MessageSender::User | MessageSender::Assistant))
                .filter(|msg| msg.tool_stream.is_none()) // Exclude tool stream messages
                .cloned()
                .collect();
            let l2b_backend = Arc::clone(self.l2b_backend.as_ref().unwrap());
            match SessionState::new(l2b_backend, filtered_messages).await {
                Ok(new_session) => {
                    self.session = new_session;
                    self.current_is_l2b = true;
                }
                Err(e) => {
                    tracing::error!("Failed to switch to l2b backend: {}", e);
                    // Continue with current session, don't fail the message sending
                }
            }
        }
        // Note: We don't switch back from l2b to chat like in manager.rs
        // The session continues with l2b until explicitly restarted

        self.session.process_user_message(message).await
    }

    async fn interrupt_processing(&mut self) -> Result<()> {
        self.session.interrupt_processing().await?;
        self.auto_scroll = true;
        Ok(())
    }

    pub async fn on_tick(&mut self) {
        if self.session.is_processing {
            self.spinner_index = (self.spinner_index + 1) % 10;
        }
        self.session.update_state().await;
    }

    #[allow(dead_code)]
    fn add_user_message(&mut self, content: &str) {
        self.session.add_user_message(content);
    }

    #[allow(dead_code)]
    fn add_assistant_message_streaming(&mut self) {
        self.session.add_assistant_message_streaming();
    }

    #[allow(dead_code)]
    fn add_system_message(&mut self, content: &str) {
        self.session.add_system_message(content);
    }
}
