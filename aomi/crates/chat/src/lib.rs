use std::fmt;
use serde_json::Value;

pub mod accounts;
pub mod app;
pub mod completion;
pub mod connections;
pub mod prompts;

// Re-exports
pub use accounts::generate_account_context;
pub use aomi_tools::ToolResultStream;
pub use app::{ChatApp, ChatAppBuilder, LoadingProgress, run_chat};
pub use completion::{RespondStream, StreamingError, stream_completion};
pub use rig::message::{AssistantContent, Message, UserContent};

/// System-level events that travel outside the LLM chat stream.
#[derive(Debug, Clone)]
pub enum SystemEvent {
    SystemNotice(String),
    SystemError(String),
    BackendConnecting(String),
    BackendConnected,
    MissingApiKey,
    WalletTxRequest { payload: Value },
    WalletTxResponse {
        status: String,
        tx_hash: Option<String>,
        detail: Option<String>,
    },
    UserRequest { kind: String, payload: Value },
    UserResponse { kind: String, payload: Value },
}

// Generic ChatCommand that can work with any stream type
#[derive(Debug)]
pub enum ChatCommand<S = Box<dyn std::any::Any + Send>> {
    StreamingText(String),
    ToolCall { topic: String, stream: S },
    Complete,
    Error(String),
    System(String),
    BackendConnected,
    BackendConnecting(String),
    MissingApiKey,
    Interrupted,
    WalletTransactionRequest(String),
}

impl<S> fmt::Display for ChatCommand<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChatCommand::StreamingText(text) => write!(f, "{}", text),
            ChatCommand::ToolCall { topic, .. } => write!(f, "Tool: {}", topic),
            ChatCommand::Error(error) => write!(f, "{}", error),
            ChatCommand::System(message) => write!(f, "{}", message),
            _ => Ok(()),
        }
    }
}
