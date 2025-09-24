use anyhow::Result;
use chrono::Local;
use serde::Serialize;

use agent::AgentMessage;
use rig::completion::Message;

use crate::agent_pool::ProcessingResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupPhase {
    ConnectingMcp,
    ValidatingAnthropic,
    Ready,
    MissingApiKey,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadinessState {
    pub phase: SetupPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ReadinessState {
    pub fn ready() -> Self {
        Self {
            phase: SetupPhase::Ready,
            detail: None,
        }
    }

    pub fn error(detail: String) -> Self {
        Self {
            phase: SetupPhase::Error,
            detail: Some(detail),
        }
    }
}

impl SetupPhase {
    pub fn allows_user_messages(self) -> bool {
        matches!(self, SetupPhase::Ready)
    }
}

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
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub is_loading: bool,
    pub is_connecting_mcp: bool,
    pub missing_api_key: bool,
    pub pending_wallet_tx: Option<String>,
    pub readiness: ReadinessState,
}

pub struct SessionState {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub is_loading: bool,
    pub is_connecting_mcp: bool,
    pub missing_api_key: bool,
    pub pending_wallet_tx: Option<String>,
    pub chat_history: Vec<Message>,
    pub readiness: ReadinessState,
}

impl SessionState {
    pub async fn new(_skip_docs: bool) -> Result<Self> {
        Ok(Self {
            messages: Vec::new(),
            is_processing: false,
            is_loading: false,
            is_connecting_mcp: false,
            missing_api_key: false,
            pending_wallet_tx: None,
            chat_history: Vec::new(),
            readiness: ReadinessState::ready(),
        })
    }

    pub async fn process_msg_from_ui(&mut self, message: String) -> Result<Option<String>> {
        if self.is_processing
            || self.is_loading
            || self.is_connecting_mcp
            || self.missing_api_key
            || !self.readiness.phase.allows_user_messages()
        {
            return Ok(None);
        }

        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        self.append_user(trimmed);
        self.is_processing = true;

        Ok(Some(trimmed.to_string()))
    }

    pub fn apply_agent_result(&mut self, result: ProcessingResult) {
        self.chat_history = result.updated_history;

        for agent_msg in result.messages {
            match agent_msg {
                AgentMessage::StreamingText(text) => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        if matches!(last_msg.sender, MessageSender::Assistant) && last_msg.is_streaming {
                            last_msg.content.push_str(&text);
                        } else {
                            self.append_assistant_with_text(&text);
                        }
                    } else {
                        self.append_assistant_with_text(&text);
                    }
                }
                AgentMessage::ToolCall { name, args } => {
                    if let Some(assistant_msg) =
                        self.messages.iter_mut().rev().find(|msg| matches!(msg.sender, MessageSender::Assistant))
                    {
                        assistant_msg.is_streaming = false;
                    }
                    let tool_msg = format!("tool: {name} | args: {args}");
                    self.append_system(&tool_msg);
                }
                AgentMessage::System(msg) => {
                    self.append_system(&msg);
                }
                AgentMessage::Error(err) => {
                    self.append_system(&format!("Error: {err}"));
                    self.is_processing = false;
                }
                AgentMessage::WalletTransactionRequest(tx_json) => {
                    self.pending_wallet_tx = Some(tx_json);
                    self.append_system(
                        "Transaction request sent to user's wallet. Waiting for user approval or rejection.",
                    );
                }
                AgentMessage::Complete => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.is_streaming = false;
                    }
                    self.is_processing = false;
                }
                _ => {}
            }
        }
    }

    pub async fn interrupt_processing(&mut self) -> Result<()> {
        if self.is_processing {
            self.append_system("Interrupted by user");
            self.is_processing = false;
        }
        Ok(())
    }

    pub async fn update_state(&mut self) {
        // Stateless agent pool has no background state to poll.
    }

    pub fn get_state(&self) -> SessionResponse {
        SessionResponse {
            messages: self.messages.clone(),
            is_processing: self.is_processing,
            is_loading: self.is_loading,
            is_connecting_mcp: self.is_connecting_mcp,
            missing_api_key: self.missing_api_key,
            pending_wallet_tx: self.pending_wallet_tx.clone(),
            readiness: self.readiness.clone(),
        }
    }

    pub fn clear_pending_wallet_tx(&mut self) {
        self.pending_wallet_tx = None;
    }

    pub fn set_readiness(&mut self, readiness: ReadinessState) {
        self.readiness = readiness;
    }

    fn append_user(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::User,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    fn append_assistant_with_text(&mut self, content: &str) {
        let mut message = ChatMessage {
            sender: MessageSender::Assistant,
            content: String::new(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        };
        message.content.push_str(content);
        self.messages.push(message);
    }

    pub(crate) fn append_system(&mut self, content: &str) {
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
}
