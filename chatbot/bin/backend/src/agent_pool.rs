use anyhow::Result;
use futures::StreamExt;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use agent::AgentMessage;
use rig::OneOrMany;
use rig::{
    agent::Agent,
    completion::{self, Message},
    message::{AssistantContent, ToolResultContent, UserContent},
    providers::anthropic::completion::CompletionModel,
    streaming::StreamedAssistantContent,
    tool::ToolSetError,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct AgentPool {
    agents: Vec<Arc<Agent<CompletionModel>>>,
    semaphore: Arc<Semaphore>,
    busy_count: AtomicUsize,
    next_agent: AtomicUsize,
}

pub struct ProcessingResult {
    pub response: String,
    pub updated_history: Vec<Message>,
    pub messages: Vec<AgentMessage>,
}

impl AgentPool {
    pub fn new(agents: Vec<Arc<Agent<CompletionModel>>>) -> Self {
        let concurrency = agents.len().max(1);
        Self {
            agents,
            semaphore: Arc::new(Semaphore::new(concurrency)),
            busy_count: AtomicUsize::new(0),
            next_agent: AtomicUsize::new(0),
        }
    }

    pub fn get_pool_size(&self) -> usize {
        self.agents.len()
    }

    pub fn get_busy_count(&self) -> usize {
        self.busy_count.load(Ordering::Relaxed)
    }

    pub async fn process_message(&self, message: String, chat_history: Vec<Message>) -> Result<ProcessingResult> {
        if self.agents.is_empty() {
            return Err(anyhow::anyhow!("Agent pool has no agents"));
        }

        let permit = self.acquire_permit().await?;
        let agent = self.next_agent();

        self.busy_count.fetch_add(1, Ordering::SeqCst);
        let result = self.process_with_agent(agent, message, chat_history).await;
        self.busy_count.fetch_sub(1, Ordering::SeqCst);

        drop(permit);

        result
    }

    async fn acquire_permit(&self) -> Result<OwnedSemaphorePermit> {
        Ok(self.semaphore.clone().acquire_owned().await.map_err(|_| anyhow::anyhow!("Agent pool semaphore closed"))?)
    }

    fn next_agent(&self) -> Arc<Agent<CompletionModel>> {
        let index = self.next_agent.fetch_add(1, Ordering::Relaxed) % self.agents.len();
        Arc::clone(&self.agents[index])
    }

    async fn process_with_agent(
        &self,
        agent: Arc<Agent<CompletionModel>>,
        message: String,
        chat_history: Vec<Message>,
    ) -> Result<ProcessingResult> {
        let mut history = chat_history;
        let mut agent_messages = Vec::new();
        let mut full_response = String::new();

        let mut current_prompt = completion::Message::user(message.clone());

        loop {
            let mut stream = agent.stream_completion(current_prompt.clone(), history.clone()).await?.stream().await?;

            // Promote prompt into history so subsequent turns see it
            history.push(current_prompt.clone());

            let mut tool_calls = Vec::new();
            let mut tool_results = Vec::new();
            let mut saw_tool_call = false;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(StreamedAssistantContent::Text(text)) => {
                        let text_value = text.text;
                        if !text_value.is_empty() {
                            full_response.push_str(&text_value);
                            agent_messages.push(AgentMessage::StreamingText(text_value));
                        }
                        saw_tool_call = false;
                    }
                    Ok(StreamedAssistantContent::Reasoning(reasoning)) => {
                        let reasoning = reasoning.reasoning;
                        if !reasoning.is_empty() {
                            full_response.push_str(&reasoning);
                            agent_messages.push(AgentMessage::StreamingText(reasoning));
                        }
                        saw_tool_call = false;
                    }
                    Ok(StreamedAssistantContent::ToolCall(tool_call)) => {
                        let name = tool_call.function.name.clone();
                        let args = tool_call.function.arguments.to_string();
                        agent_messages.push(AgentMessage::ToolCall {
                            name: name.clone(),
                            args: args.clone(),
                        });

                        let tool_result = execute_tool(&agent, &tool_call).await;
                        match tool_result {
                            Ok(ToolExecution::WalletRequest(json)) => {
                                agent_messages.push(AgentMessage::WalletTransactionRequest(json.clone()));
                                tool_results.push((tool_call.id.clone(), tool_call.call_id.clone(), json));
                            }
                            Ok(ToolExecution::ResultText(text)) => {
                                let preview = if text.len() > 200 {
                                    format!("{}...", &text[..200])
                                } else {
                                    text.clone()
                                };
                                agent_messages.push(AgentMessage::System(preview.clone()));
                                tool_results.push((tool_call.id.clone(), tool_call.call_id.clone(), text));
                            }
                            Err(err_msg) => {
                                agent_messages.push(AgentMessage::Error(err_msg.clone()));
                                // Tool failure stops the turn to keep session state consistent
                                history.push(completion::Message::assistant(full_response.clone()));
                                agent_messages.push(AgentMessage::Complete);
                                return Ok(ProcessingResult {
                                    response: full_response,
                                    updated_history: history,
                                    messages: agent_messages,
                                });
                            }
                        }

                        tool_calls.push(AssistantContent::ToolCall(tool_call.clone()));
                        saw_tool_call = true;
                    }
                    Ok(_) => {}
                    Err(err) => {
                        let error_msg = format!("error: {}", err);
                        agent_messages.push(AgentMessage::Error(error_msg.clone()));
                        history.push(completion::Message::assistant(full_response.clone()));
                        agent_messages.push(AgentMessage::Complete);
                        return Ok(ProcessingResult {
                            response: full_response,
                            updated_history: history,
                            messages: agent_messages,
                        });
                    }
                }
            }

            if !tool_calls.is_empty() {
                history.push(completion::Message::Assistant {
                    id: None,
                    content: rig::OneOrMany::many(tool_calls).expect("non-empty tool call list"),
                });
            }

            for (id, call_id, tool_output) in tool_results {
                let content = rig::OneOrMany::one(ToolResultContent::text(tool_output));
                let user_message = if let Some(call_id) = call_id {
                    completion::Message::User {
                        content: rig::OneOrMany::one(UserContent::tool_result_with_call_id(id, call_id, content)),
                    }
                } else {
                    completion::Message::User {
                        content: rig::OneOrMany::one(UserContent::tool_result(id, content)),
                    }
                };
                history.push(user_message);
            }

            if saw_tool_call {
                // Continue the loop with the most recent message (typically tool result) as the next prompt
                current_prompt = history.pop().expect("tool interaction should leave at least one message in history");
                continue;
            }

            break;
        }

        history.push(completion::Message::assistant(full_response.clone()));
        agent_messages.push(AgentMessage::Complete);

        Ok(ProcessingResult {
            response: full_response,
            updated_history: history,
            messages: agent_messages,
        })
    }
}

enum ToolExecution {
    ResultText(String),
    WalletRequest(String),
}

async fn execute_tool(
    agent: &Agent<CompletionModel>,
    tool_call: &rig::message::ToolCall,
) -> Result<ToolExecution, String> {
    agent
        .tools
        .call(&tool_call.function.name, tool_call.function.arguments.to_string())
        .await
        .map_err(|err| format!("Tool execution failed: {}", render_tool_error(&err)))
        .map(|result| {
            if tool_call.function.name == "send_transaction_to_wallet" {
                ToolExecution::WalletRequest(result)
            } else {
                ToolExecution::ResultText(result)
            }
        })
}

fn render_tool_error(err: &ToolSetError) -> String {
    match err {
        ToolSetError::ToolNotFound { name } => format!("tool `{}` not found", name),
        ToolSetError::ToolExecution { name, source } => format!("{}: {}", name, source),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::providers::anthropic;

    fn dummy_agent() -> Arc<Agent<CompletionModel>> {
        let client = anthropic::Client::new("dummy");
        Arc::new(client.agent("claude-sonnet-4-20250514").preamble("Test agent").build())
    }

    #[test]
    fn pool_tracks_capacity() {
        let pool = AgentPool::new(vec![dummy_agent(), dummy_agent()]);
        assert_eq!(pool.get_pool_size(), 2);
        assert_eq!(pool.get_busy_count(), 0);
    }
}
