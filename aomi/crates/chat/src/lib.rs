use serde::Serialize;
use serde_json::Value;
use std::{
    fmt,
    sync::{Arc, Mutex},
};

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
#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    SystemBroadcast(String),
    SystemNotice(String),
    SystemError(String),
    SystemToolDisplay {
        tool_name: String,
        call_id: String,
        result: Value,
    },
    WalletTxRequest {
        payload: Value,
    },
    WalletTxResponse {
        status: String,
        tx_hash: Option<String>,
        detail: Option<String>,
    },
    UserRequest {
        kind: String,
        payload: Value,
    },
    UserResponse {
        kind: String,
        payload: Value,
    },
}

#[derive(Clone, Debug, Default)]
pub struct SystemEventQueue {
    inner: Arc<Mutex<Vec<SystemEvent>>>,
}

impl SystemEventQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn push(&self, event: SystemEvent) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push(event);
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Clone all events from the provided index onward.
    pub fn slice_from(&self, start: usize) -> Vec<SystemEvent> {
        if let Ok(guard) = self.inner.lock() {
            return guard.get(start..).unwrap_or(&[]).to_vec();
        }
        Vec::new()
    }
}

// Generic ChatCommand that can work with any stream type
#[derive(Debug)]
pub enum ChatCommand<S = Box<dyn std::any::Any + Send>> {
    StreamingText(String),
    ToolCall { topic: String, stream: S },
    AsyncToolResult {
        call_id: String,
        tool_name: String,
        result: Value,
    },
    Complete,
    Error(String),
    Interrupted,
}

impl<S> fmt::Display for ChatCommand<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChatCommand::StreamingText(text) => write!(f, "{}", text),
            ChatCommand::ToolCall { topic, .. } => write!(f, "Tool: {}", topic),
            ChatCommand::Error(error) => write!(f, "{}", error),
            _ => Ok(()),
        }
    }
}
