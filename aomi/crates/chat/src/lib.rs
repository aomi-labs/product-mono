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

// Re-exports from aomi-tools - the canonical location for tool infrastructure
pub use aomi_tools::scheduler::{PersistedHandlerState, SessionToolHandler, ToolHandler};
pub use aomi_tools::{
    AomiTool, AomiToolArgs, AomiToolWrapper, CallMetadata, RuntimeEnvelope, ToolCallCtx,
    ToolCompletion, ToolMetadata, ToolReciever as ToolReceiver, ToolReturn, ToolScheduler,
    WithTopic, with_topic,
};

// Re-exports from prompts module
pub use prompts::generate_account_context;

// Re-exports from app module
pub use app::{CoreApp, CoreAppBuilder};

// Re-exports from model module
pub use aomi_baml::{AomiModel, Selection};

// Re-exports from completion module
pub use completion::{CoreCommandStream, StreamingError, stream_completion};

// Re-exports from rig
pub use rig::message::{AssistantContent, Message, UserContent};

// CoreCommand for tool results and streaming text
#[derive(Debug)]
pub enum CoreCommand {
    StreamingText(String),
    ToolCall { topic: String, stream: ToolReturn },
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
