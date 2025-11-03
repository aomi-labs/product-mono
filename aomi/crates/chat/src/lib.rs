use std::fmt;

pub mod accounts;
pub mod app;
pub mod completion;
pub mod connections;
pub mod prompts;

// Re-exports
pub use accounts::generate_account_context;
pub use app::{ChatApp, LoadingProgress, run_chat};
pub use completion::{RespondStream, StreamingError, stream_completion};
pub use rig::message::{AssistantContent, Message, UserContent};
pub use aomi_tools::ToolResultStream;

// Generic ChatCommand that can work with any stream type
#[derive(Debug)]  
pub enum ChatCommand<S = Box<dyn std::any::Any + Send>> {
    StreamingText(String),
    ToolCall {
        topic: String,
        stream: S,
    },
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