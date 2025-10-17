use anyhow::Result;
use chrono::Local;
use serde::Serialize;
use tokio::sync::mpsc;

use aomi_agent::{AgentMessage, LoadingProgress, SharedDocumentStore};

const ASSISTANT_WELCOME: &str = "Hello! I'm your blockchain transaction agent. I can help you interact with EVM-compatible networks using natural language. Here's what I can do:\n\n- **Check anything**\n  - \"What's the best pool to stake my ETH?\"\n  - \"How much money have I made from my LP position?\"\n  - \"Where can I swap my ETH for USDC with the best price?\"\n- **Call anything**\n  - \"Deposit half of my ETH into the best pool\"\n  - \"Sell my NFT collection X on a marketplace that supports it\"\n  - \"Recommend a portfolio of DeFi projects based on my holdings and deploy my capital\"\n- **Switch networks** - I support testnet, mainnet, polygon, base, and more\n\nI have access to:\n🔗 **Networks** - Testnet, Ethereum, Polygon, Base, Arbitrum\n🛠️ **Tools** - Cast, Etherscan, 0x API, Web Search\n💰 **Wallet** - Connect your wallet for seamless transactions\n\nI default to a testnet forked from Ethereum without wallet connection. You can test it out with me first. Once you connect your wallet, I can compose real transactions based on available protocols & contracts info on the public blockchain.\n\n**Important Note:** I'm still under development; use me at your own risk. The source of my knowledge is internet search, so please check transactions before you sign.\n\nWhat blockchain task would you like help with today?";
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
    fn new(phase: SetupPhase) -> Self {
        Self {
            phase,
            detail: None,
        }
    }
}

impl SetupPhase {
    fn allows_user_messages(self) -> bool {
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

pub struct SessionState {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub readiness: ReadinessState,
    pub pending_wallet_tx: Option<String>,
    has_sent_welcome: bool,
    sender_to_llm: mpsc::Sender<String>,
    receiver_from_llm: mpsc::Receiver<AgentMessage>,
    loading_receiver: mpsc::Receiver<LoadingProgress>,
    interrupt_sender: mpsc::Sender<()>,
}

impl SessionState {
    pub async fn new(
        skip_docs: bool,
        shared_document_store: Option<SharedDocumentStore>,
    ) -> Result<Self> {
        if !skip_docs && shared_document_store.is_none() {
            anyhow::bail!(
                "Shared document store is required when documentation loading is enabled"
            );
        }
        let (sender_to_llm, receiver_from_ui) = mpsc::channel(100);
        let (sender_to_ui, receiver_from_llm) = mpsc::channel(100);
        let (loading_sender, loading_receiver) = mpsc::channel(100);
        let (interrupt_sender, interrupt_receiver) = mpsc::channel(100);
        let shared_document_store_for_agent = shared_document_store.clone();

        tokio::spawn(async move {
            let _ = aomi_agent::setup_agent_and_handle_messages(
                receiver_from_ui,
                sender_to_ui,
                loading_sender,
                interrupt_receiver,
                shared_document_store_for_agent,
                skip_docs,
            )
            .await;
        });

        Ok(Self {
            messages: vec![],
            is_processing: false,
            readiness: ReadinessState::new(SetupPhase::ConnectingMcp),
            pending_wallet_tx: None,
            has_sent_welcome: false,
            sender_to_llm,
            receiver_from_llm,
            loading_receiver,
            interrupt_sender,
        })
    }

    fn set_readiness(&mut self, phase: SetupPhase, detail: Option<String>) {
        self.readiness.phase = phase;
        if let Some(detail) = detail {
            self.readiness.detail = Some(detail);
        } else if matches!(phase, SetupPhase::Ready) {
            self.readiness.detail = None;
        }
    }

    pub async fn process_message_from_ui(&mut self, message: String) -> Result<()> {
        if self.is_processing || !self.readiness.phase.allows_user_messages() {
            return Ok(());
        }

        let message = message.trim();
        if message.is_empty() {
            return Ok(());
        }

        self.add_user_message(message);
        self.is_processing = true;

        if let Err(e) = self.sender_to_llm.send(message.to_string()).await {
            self.add_system_message(&format!(
                "Failed to send message: {e}. Agent may have disconnected."
            ));
            self.is_processing = false;
            return Ok(());
        }

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
        while let Ok(progress) = self.loading_receiver.try_recv() {
            match progress {
                LoadingProgress::Message(msg) => {
                    self.add_system_message(&msg);
                }
                LoadingProgress::Complete => {
                    if matches!(
                        self.readiness.phase,
                        SetupPhase::ConnectingMcp | SetupPhase::ValidatingAnthropic
                    ) {
                        self.set_readiness(
                            self.readiness.phase,
                            Some("Documentation loaded".to_string()),
                        );
                    }
                }
            }
        }

        while let Ok(msg) = self.receiver_from_llm.try_recv() {
            match msg {
                AgentMessage::StreamingText(text) => {
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
                    {
                        if assistant_msg.is_streaming {
                            assistant_msg.content.push_str(&text);
                        }
                    }
                }
                AgentMessage::ToolCall { name, args } => {
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
                AgentMessage::Complete => {
                    if let Some(last_msg) = self.messages.last_mut() {
                        last_msg.is_streaming = false;
                    }
                    self.is_processing = false;
                }
                AgentMessage::Error(err) => {
                    if err.contains("CompletionError") {
                        self.add_system_message(
                            "Anthropic API request failed. Please try your last message again.",
                        );
                    } else {
                        self.add_system_message(&format!("Error: {err}"));
                    }
                    self.set_readiness(SetupPhase::Error, Some(err.clone()));
                    self.is_processing = false;
                }
                AgentMessage::WalletTransactionRequest(tx_json) => {
                    self.pending_wallet_tx = Some(tx_json.clone());
                    self.add_system_message(
                        "Transaction request sent to user's wallet. Waiting for user approval or rejection.",
                    );
                }
                AgentMessage::System(msg) => {
                    self.add_system_message(&msg);
                }
                AgentMessage::BackendConnected => {
                    self.add_system_message("All backend services connected and ready");
                    self.set_readiness(
                        SetupPhase::Ready,
                        Some("All backend services connected".to_string()),
                    );
                    if !self.has_sent_welcome {
                        self.add_assistant_message(ASSISTANT_WELCOME);
                        self.has_sent_welcome = true;
                    }
                }
                AgentMessage::BackendConnecting(s) => {
                    let detail = s;
                    self.add_system_message(&detail);
                    let lowered = detail.to_lowercase();
                    if lowered.contains("anthropic") {
                        self.set_readiness(SetupPhase::ValidatingAnthropic, Some(detail));
                    } else {
                        self.set_readiness(SetupPhase::ConnectingMcp, Some(detail));
                    }
                }
                AgentMessage::MissingApiKey => {
                    self.add_system_message(
                        "Anthropic API key missing. Set ANTHROPIC_API_KEY and restart.",
                    );
                    self.set_readiness(
                        SetupPhase::MissingApiKey,
                        Some("Anthropic API key missing".to_string()),
                    );
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

    fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageSender::Assistant,
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

    pub fn add_system_message(&mut self, content: &str) {
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
            readiness: self.readiness.clone(),
            pending_wallet_tx: self.pending_wallet_tx.clone(),
        }
    }

    #[allow(dead_code)]
    pub fn clear_pending_wallet_tx(&mut self) {
        self.pending_wallet_tx = None;
    }

    pub fn send_to_llm(&self) -> &mpsc::Sender<String> {
        &self.sender_to_llm
    }
}

#[derive(Serialize)]
pub struct WebStateResponse {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub readiness: ReadinessState,
    pub pending_wallet_tx: Option<String>,
}
