pub mod eval_app;
pub mod eval_state;
pub mod harness;
#[cfg(test)]
#[cfg(feature = "eval")]
pub mod test_entry;

use std::fmt;

use aomi_backend::{ChatMessage, session::MessageSender};

pub use eval_state::EvalState;

#[derive(Debug, Clone)]
pub struct RoundResult {
    pub input: String,
    pub actions: Vec<AgentAction>,
}

impl RoundResult {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn tool_call_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|action| matches!(action, AgentAction::ToolCall(_)))
            .count()
    }

    pub fn response_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|action| matches!(action, AgentAction::Response(_)))
            .count()
    }
}

impl fmt::Display for RoundResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, ">> {}", self.input)?;
        for (idx, action) in self.actions.iter().enumerate() {
            writeln!(f, "  [{idx:02}] {action}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_id: usize,
    pub intent: String,
    pub rounds: Vec<RoundResult>,
}

impl TestResult {
    pub fn is_empty(&self) -> bool {
        self.rounds.iter().all(RoundResult::is_empty)
    }

    pub fn total_rounds(&self) -> usize {
        self.rounds.len()
    }

    pub fn total_tool_calls(&self) -> usize {
        self.rounds.iter().map(RoundResult::tool_call_count).sum()
    }

    pub fn total_responses(&self) -> usize {
        self.rounds.iter().map(RoundResult::response_count).sum()
    }
}

impl fmt::Display for TestResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Test #{}: {}", self.test_id, self.intent)?;
        if self.rounds.is_empty() {
            writeln!(f, "  (no rounds recorded)")?;
            return Ok(());
        }

        for (round_idx, round) in self.rounds.iter().enumerate() {
            writeln!(f, "\nRound {}:", round_idx + 1)?;
            writeln!(f, "{round}")?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum AgentAction {
    System(String),
    Response(String),
    ToolCall(ToolCall),
}

impl AgentAction {
    fn from_messages(messages: &[ChatMessage]) -> Vec<Self> {
        messages
            .iter()
            .filter_map(|msg| {
                if let Some(tool) = ToolCall::from_message(msg) {
                    return Some(AgentAction::ToolCall(tool));
                }

                match msg.sender {
                    MessageSender::Assistant => {
                        if msg.content.trim().is_empty() {
                            None
                        } else {
                            Some(AgentAction::Response(msg.content.clone()))
                        }
                    }
                    MessageSender::System => {
                        if msg.content.trim().is_empty() {
                            None
                        } else {
                            Some(AgentAction::System(msg.content.clone()))
                        }
                    }
                    MessageSender::User => None,
                }
            })
            .collect()
    }
}

impl fmt::Display for AgentAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentAction::System(text) => write!(f, "[system] {text}"),
            AgentAction::Response(text) => write!(f, "[response] {text}"),
            AgentAction::ToolCall(call) => {
                if std::env::var("DEBUG").is_ok() {
                    write!(f, "[tool] {call}")
                } else {
                    let first_line = call.content.lines().next().unwrap_or("");
                    write!(f, "[tool] {} => {}", call.topic, first_line)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub topic: String,
    pub content: String,
}

impl ToolCall {
    fn from_message(msg: &ChatMessage) -> Option<Self> {
        msg.tool_stream.as_ref().map(|(topic, content)| ToolCall {
            topic: topic.clone(),
            content: content.clone(),
        })
    }
}

impl fmt::Display for ToolCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.topic, self.content)
    }
}
