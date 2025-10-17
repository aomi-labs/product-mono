use std::sync::{Arc, OnceLock};
use crate::{BamlClient, ContractApi, ToolApiHandler, ToolScheduler, WeatherApi};
// use rig::agent::Agent;

pub static SCHEDULER_SINGLETON: OnceLock<std::io::Result<Arc<ToolScheduler>>> = OnceLock::new();



pub trait AomiApp: Clone + Default {
    type Input;
    type Output;
    type State: Clone;

    fn process(&self, input: Self::Input, state: Self::State, baml_client: BamlClient) -> Self::State;
    fn complete(&self, state: Self::State, baml_client: BamlClient) -> Self::Output;
}


#[derive(Clone)]
pub struct ChatApp<M: CompletionModel> {
    preamble: Option<String>,
    tool_scheduler: Arc<ToolScheduler>,
    tool_handler: Arc<ToolApiHandler>,
    rig_agent: Arc<rig::agent::Agent<M>>,
    baml_client: Arc<BamlClient>,
}

impl ChatApp<rig::providers::anthropic::completion::CompletionModel> {
    pub fn new() -> Self {
        let anthropic_client = rig::providers::anthropic::Client::new("..");
        let agent = anthropic_client
            .agent(rig::providers::anthropic::CLAUDE_3_5_SONNET)
            .tool(WeatherApi::new())
            .tool(ContractApi::new())
            .tool(GetCurrentTime)
            .build();
        
        
        let (handler, mut scheduler) = ToolScheduler::new();
        scheduler.register_tool(ContractApi::new());
        scheduler.register_tool(WeatherApi::new());
        let scheduler = Arc::new(scheduler);

        SCHEDULER_SINGLETON.get_or_init(|| Ok(scheduler.clone()));


        Self {
            preamble: None,
            tool_scheduler: scheduler.clone(),
            tool_handler: Arc::new(handler),
            rig_agent: Arc::new(agent),
            baml_client: Arc::new(BamlClient::new()),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_app() {
        let app = ChatApp::new();
        let _a = app.clone();
    }
}

use rig::{client::CompletionClient, completion::CompletionModel};
// use rig::client::CompletionClient;
use rig_derive::rig_tool;

#[rig_tool]
pub fn get_current_time() -> Result<String, rig::tool::ToolError> {
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
    let seconds = duration.as_secs();

    Ok(seconds.to_string())
}


