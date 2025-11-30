use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::{collections::HashMap, sync::Arc};

use aomi_backend::{
    BackendType, SessionState,
    session::{BackendwithTool, DefaultSessionState},
};

pub use aomi_backend::{ChatMessage, MessageSender};

pub struct SessionContainer {
    pub session: DefaultSessionState,
    pub input: String,
    pub scroll_offset: usize,
    pub cursor_position: usize,
    pub spinner_index: usize,
    pub total_list_items: usize,
    pub auto_scroll: bool,
    backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>,
    current_backend: BackendType,
}

impl SessionContainer {
    pub async fn new(backends: Arc<HashMap<BackendType, Arc<BackendwithTool>>>) -> Result<Self> {
        let default_backend = backends
            .get(&BackendType::Default)
            .ok_or_else(|| anyhow::anyhow!("default backend missing"))?;
        let session = SessionState::new(Arc::clone(default_backend), Vec::new()).await?;

        Ok(Self {
            session,
            input: String::new(),
            scroll_offset: 0,
            cursor_position: 0,
            spinner_index: 0,
            total_list_items: 0,
            auto_scroll: true,
            backends,
            current_backend: BackendType::Default,
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

        let normalized = message.to_lowercase();
        let backend_request = match normalized.as_str() {
            s if s.contains("default-magic") => Some(BackendType::Default),
            s if s.contains("l2beat-magic") => Some(BackendType::L2b),
            s if s.contains("forge-magic") => Some(BackendType::Forge),
            _ => None,
        };

        let desired_backend = backend_request.unwrap_or(self.current_backend);

        if desired_backend != self.current_backend
            && let Some(backend) = self.backends.get(&desired_backend)
        {
            tracing::info!("switching to {:?} backend", desired_backend);
            let current_messages = self.session.messages.clone();
            match SessionState::new(Arc::clone(backend), current_messages).await {
                Ok(new_session) => {
                    self.session = new_session;
                    self.current_backend = desired_backend;
                }
                Err(e) => {
                    tracing::error!("Failed to switch backend {:?}: {}", desired_backend, e);
                }
            }
        }

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
