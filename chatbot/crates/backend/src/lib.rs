pub mod history;
pub mod manager;
pub mod session;

pub use history::{filter_system_messages, to_rig_messages, UserHistory};
pub use manager::{generate_session_id, SessionManager};
pub use session::{
    ChatBackend, ChatMessage, MessageSender, SessionResponse, SessionState,
};

#[cfg(test)]
mod tests;
