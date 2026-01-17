use std::fmt;

pub mod app;
pub mod completion;
pub mod connections;
pub mod events;
pub mod prompts;
pub mod state;

// Re-exports from events module
pub use events::{SystemEvent, SystemEventQueue};

// Re-exports from state module
pub use state::{CoreCtx, CoreState};

// Re-exports from prompts module
pub use aomi_tools::{CallMetadata, ToolCompletion, ToolReturn};
pub use app::{CoreApp, CoreAppBuilder};
pub use completion::{CoreCommandStream, StreamingError, stream_completion};
pub use prompts::generate_account_context;
pub use rig::message::{AssistantContent, Message, UserContent};

// CoreCommand for tool results and streaming text
#[derive(Debug)]
pub enum CoreCommand {
    StreamingText(String),
    ToolCall {
        topic: String,
        stream: aomi_tools::ToolReturn,
    },
    Complete,
    Error(String),
    Interrupted,
}

impl fmt::Display for CoreCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreCommand::StreamingText(text) => write!(f, "{}", text),
            CoreCommand::ToolCall { topic, .. } => write!(f, "Tool: {}", topic),
            CoreCommand::Error(error) => write!(f, "{}", error),
            _ => Ok(()),
        }
    }
}
