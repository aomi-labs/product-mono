use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::sync::Arc;

use aomi_agent::{ChatApp, ChatCommand};
use aomi_backend::SessionState;

pub use aomi_backend::{ChatMessage, MessageSender};

pub struct SessionContainer {
    pub session: SessionState,
    pub input: String,
    pub scroll_offset: usize,
    pub cursor_position: usize,
    pub spinner_index: usize,
    pub total_list_items: usize,
    pub auto_scroll: bool,
}

impl SessionContainer {
    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        let chat_app = Arc::new(
            ChatApp::new_with_options(skip_docs, skip_mcp)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        );
        let session = SessionState::new(chat_app, Vec::new()).await?;

        Ok(Self {
            session,
            input: String::new(),
            scroll_offset: 0,
            cursor_position: 0,
            spinner_index: 0,
            total_list_items: 0,
            auto_scroll: true,
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

    fn add_user_message(&mut self, content: &str) {
        self.session.add_user_message(content);
    }

    fn add_assistant_message_streaming(&mut self) {
        self.session.add_assistant_message_streaming();
    }

    fn add_system_message(&mut self, content: &str) {
        self.session.add_system_message(content);
    }
}
