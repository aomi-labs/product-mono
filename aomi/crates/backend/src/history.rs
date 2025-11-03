use std::{sync::Arc, time::Instant};

use aomi_chat::Message;
use tokio::sync::Mutex;

use crate::session::{ChatMessage, MessageSender, DefaultSessionState};

#[derive(Clone)]
pub struct UserHistory {
    messages: Vec<ChatMessage>,
    last_activity: Instant,
}

impl UserHistory {
    pub fn new(messages: Vec<ChatMessage>, last_activity: Instant) -> Self {
        Self {
            messages,
            last_activity,
        }
    }

    pub fn empty_with_activity(last_activity: Instant) -> Self {
        Self::new(Vec::new(), last_activity)
    }

    pub fn from_messages_now(messages: Vec<ChatMessage>) -> Self {
        Self::new(messages, Instant::now())
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn into_messages(self) -> Vec<ChatMessage> {
        self.messages
    }

    pub fn conversation_messages(&self) -> Vec<ChatMessage> {
        filter_system_messages(&self.messages)
    }

    pub async fn sync_message_history(
        &mut self,
        session_activity: Instant,
        session_state: Arc<Mutex<DefaultSessionState>>,
    ) {
        let mut state = session_state.lock().await;
        if self.last_activity > session_activity {
            // TODO: self should contains the whole history of the user return from DB
            // we need to figure out the repetition and then append and save to DB
            unimplemented!()
        } else {
            // TODO: should we grab the whole history from DB for each session?
            *state.get_messages_mut() = self.messages.clone();
            *state.agent_history_handle().write().await = to_rig_messages(&self.messages);
        }
        state.sync_welcome_flag();
        self.last_activity = session_activity;
    }
}

pub fn filter_system_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter(|&msg| !matches!(msg.sender, MessageSender::System))
        .cloned()
        .collect()
}

pub fn to_rig_messages(messages: &[ChatMessage]) -> Vec<Message> {
    filter_system_messages(messages)
        .into_iter()
        .map(Message::from)
        .collect()
}
