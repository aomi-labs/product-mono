use std::time::Instant;

use aomi_agent::Message;

use crate::session::{ChatMessage, MessageSender};

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

    pub fn should_replace_messages(
        &self,
        previous_activity: Instant,
        previous_messages: &[ChatMessage],
    ) -> bool {
        let incoming = self.conversation_messages();
        let current = filter_system_messages(previous_messages);
        incoming.len() > current.len()
            || incoming != current
            || self.last_activity >= previous_activity
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
