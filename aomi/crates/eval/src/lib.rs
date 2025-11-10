use aomi_backend::{ChatMessage, session::{BackendwithTool, DefaultSessionState}};




pub struct Eval {
    session: DefaultSessionState,
}

pub struct RoundResult {
    input: String,
    actions: Vec<AgentAction>,
} 

enum AgentAction {
    Reasoning(String),
    Response(String),
    ToolCall(ToolCall),
}

pub struct ToolCall {
    topic: String,
    content: String,
}

fn main() {
    println!("Hello, world!");
}
