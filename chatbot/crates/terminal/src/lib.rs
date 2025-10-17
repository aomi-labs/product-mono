use chrono::Local;
use serde::Serialize;

mod abstractions;
pub use abstractions::*;

pub use baml_client::BamlClient;

// _____________________Terminal Specific_________________________

#[derive(Debug, Clone, Default)]
pub struct ChatTerminal {
    preamble: Option<String>,
    transaction_decoder: TransactionDecoder,
    abi_retriever: AbiRetriever,
}

struct DummyOutput;

impl ChatTerminal {
    pub fn new() -> Self {
        Self {
            preamble: Some("You are a helpful blockchain assistant.".to_string()),
            transaction_decoder: TransactionDecoder,
            abi_retriever: AbiRetriever,
        }
    }

    pub fn preamble(&self) -> Option<&str> {
        self.preamble.as_deref()
    }

    pub fn initialize_state(&self) -> ChatState {
        ChatState::default()
    }

    pub fn run_chat(
        &self,
        input: ChatInput,
        state: ChatState,
    ) -> impl Future<Output = DummyOutput> + Send + 'static {
        async move { todo!() }
    }
}

impl AomiApp for ChatTerminal {
    type Input = String;
    type Output = ChatStatus;
    type State = ChatState;

    fn process(
        &self,
        input: Self::Input,
        mut state: ChatState,
        baml_client: BamlClient,
    ) -> ChatState {
        state.history.push(format!("user: {input}"));
        loop {
            // call bamel client here
            todo!()
        }
        state
    }

    fn complete(&self, state: ChatState, baml_client: BamlClient) -> ChatStatus {
        state.status
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerminalToolBox {
    decoder: TransactionDecoder,
    abi_retriever: AbiRetriever,
}

impl TerminalToolBox {
    pub fn decoder(&self) -> &TransactionDecoder {
        &self.decoder
    }
    pub fn abi_retriever(&self) -> &AbiRetriever {
        &self.abi_retriever
    }
}

impl AomiToolBox for TerminalToolBox {}

#[derive(Debug, Clone, Default)]
pub struct TransactionDecoder;

#[derive(Debug, Clone, Default)]
pub struct DecoderOutput;

impl AomiTool for TransactionDecoder {
    type Output = DecoderOutput;

    fn name(&self) -> String {
        "TxDecoder".to_string()
    }

    fn description(&self) -> String {
        "Decode a transaction".to_string()
    }

    fn execute(&self, _input: String) -> ToolResult<Self::Output> {
        ToolResult::new(async move { DecoderOutput })
    }
}

#[derive(Debug, Clone, Default)]
pub struct AbiRetriever;

#[derive(Debug, Clone, Default)]
pub struct AbiRetrieverOutput;

impl AomiTool for AbiRetriever {
    type Output = AbiRetrieverOutput;

    fn name(&self) -> String {
        "AbiRetriever".to_string()
    }

    fn description(&self) -> String {
        "Fetch an ABI for a contract".to_string()
    }

    fn execute(&self, _input: String) -> ToolResult<Self::Output> {
        ToolResult::new(async move { AbiRetrieverOutput })
    }
}

#[derive(Debug, Clone)]
enum IntermediateOutput {
    StreamingText(String),
    DecoderOutput(DecoderOutput),
    AbiRetrieverOutput(AbiRetrieverOutput),
}

#[derive(Debug, Clone)]
pub enum ChatStatus {
    Init,
    Processing(IntermediateOutput),
    Complete,
}

impl Default for ChatStatus {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub history: Vec<String>,
    pub status: ChatStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "agent")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub sender: MessageRole,
    pub content: String,
    pub timestamp: String,
    pub is_streaming: bool,
}

#[derive(Debug, Clone)]
pub struct ChatInput {
    pub session_id: String,
    pub latest_message: String,
}

impl ChatInput {
    pub fn new(session_id: impl Into<String>, latest_message: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            latest_message: latest_message.into(),
        }
    }
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            history: Vec::new(),
            status: ChatStatus::default(),
        }
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageRole::User,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            sender: MessageRole::System,
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: false,
        });
    }

    pub fn start_assstant_streaming(&mut self) {
        self.messages.push(ChatMessage {
            sender: MessageRole::Assistant,
            content: String::new(),
            timestamp: Local::now().format("%H:%M:%S %Z").to_string(),
            is_streaming: true,
        });
    }
}
