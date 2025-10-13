mod abstractions;
use abstractions::*;

// _____________________Terminal Specific_________________________

#[derive(Debug, Clone, Default)]
pub struct ChatTerminal {
    preamble: Option<String>,
    transaction_decoder: TransactionDecoder,
    abi_retriever: AbiRetriever,
}

impl ChatTerminal {

    pub fn preamble(&self) -> Option<&str> {
        self.preamble.as_deref()
    }

    pub fn initialize_state(&self) -> TerminalState {
        TerminalState::default()
    }
}

impl AomiApp for ChatTerminal {
    type Input = String;
    type Output = TerminalOutput;
    type State = TerminalState;

    fn process(&self, input: Self::Input, mut state:TerminalState) -> TerminalState {
        state.history.push(format!("user: {input}"));
        state
    }

    fn complete(&self, state: TerminalState) -> TerminalOutput {
        state.output
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
pub enum TerminalOutput {
    Init,
    Processing(IntermediateOutput),
    Complete,
}

impl Default for TerminalOutput {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerminalState {
    pub history: Vec<String>,
    pub last_output: Option<String>,
    pub output: TerminalOutput,
}

#[derive(Debug, Clone)]
pub struct TerminalInput {
    pub session_id: String,
    pub latest_message: String,
}

impl TerminalInput {
    pub fn new(session_id: impl Into<String>, latest_message: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            latest_message: latest_message.into(),
        }
    }
}
