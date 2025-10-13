use chrono::Local;

use aomi_terminal::TerminalState;

use crate::threads::{ThreadEvent, ToolInvocation};

use crate::session::{
    ChatMessage, MessageSender, ReadinessState, SetupPhase, WebStateResponse,
};

const ASSISTANT_WELCOME: &str =
    "Hello! I'm your blockchain transaction agent. How can I help you today?";

/// Manages UI-facing state for a single session while delegating inference to stateless threads.
pub struct Frontend {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub readiness: ReadinessState,
    pub pending_wallet_tx: Option<String>,
    pub terminal_state: TerminalState,
    has_sent_welcome: bool,
}

impl Frontend {
    pub fn new(initial_state: TerminalState) -> Self {
        Self {
            messages: Vec::new(),
            is_processing: false,
            readiness: ReadinessState {
                phase: SetupPhase::Ready,
                detail: None,
            },
            pending_wallet_tx: None,
            terminal_state: initial_state,
            has_sent_welcome: false,
        }
    }

    pub fn begin_request(&mut self, user_message: &str) {
        if !self.has_sent_welcome {
            self.add_assistant_message(ASSISTANT_WELCOME);
            self.has_sent_welcome = true;
        }

        self.add_user_message(user_message);
        self.is_processing = true;
    }

    pub fn apply_thread_events(&mut self, state: TerminalState, events: Vec<ThreadEvent>) {
        self.terminal_state = state;

        for event in events {
            match event {
                ThreadEvent::StreamingText(chunk) => {
                    self.add_streaming_chunk(&chunk);
                }
                ThreadEvent::ToolCall { invocation, .. } => {
                    self.add_tool_invocation(invocation);
                }
                ThreadEvent::Complete => {
                    if let Some(last) = self.messages.last_mut() {
                        if matches!(last.sender, MessageSender::Assistant) {
                            last.is_streaming = false;
                        }
                    }
                    self.is_processing = false;
                }
            }
        }
    }

    pub fn snapshot(&self) -> WebStateResponse {
        WebStateResponse {
            messages: self.messages.clone(),
            is_processing: self.is_processing,
            readiness: self.readiness.clone(),
            pending_wallet_tx: self.pending_wallet_tx.clone(),
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

    fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    fn add_streaming_chunk(&mut self, chunk: &str) {
        let needs_new_message = self
            .messages
            .last()
            .map(|msg| !matches!(msg.sender, MessageSender::Assistant) || !msg.is_streaming)
            .unwrap_or(true);

        if needs_new_message {
            self.messages.push(ChatMessage {
                sender: MessageSender::Assistant,
                content: String::new(),
                timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
                is_streaming: true,
            });
        }

        if let Some(last) = self.messages.last_mut() {
            last.content.push_str(chunk);
        }
    }

    fn add_tool_invocation(&mut self, invocation: ToolInvocation) {
        let content = format!("tool: {} | args: {}", invocation.name, invocation.args);
        self.messages.push(ChatMessage {
            sender: MessageSender::System,
            content,
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }
}
