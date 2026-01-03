use aomi_tools::{ToolResultStream, ToolScheduler};
// Type alias for ChatCommand with ToolResultStream
pub type ChatCommand = crate::ChatCommand<ToolResultStream>;

use crate::{SystemEvent, SystemEventQueue};
use chrono::Utc;
use futures::{Stream, StreamExt, stream::BoxStream};
use rig::{
    OneOrMany,
    agent::Agent,
    completion::{self, CompletionModel},
    message::{AssistantContent, Message},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::ToolSetError as RigToolError,
};
use serde_json::{Value, json};
use std::{pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("CompletionError: {0}")]
    Completion(#[from] rig::completion::CompletionError),
    #[error("PromptError: {0}")]
    Prompt(#[from] rig::completion::PromptError),
    #[error("ToolSetError: {0}")]
    Tool(#[from] RigToolError),
    #[error("Eyre: {0}")]
    Eyre(#[from] eyre::Error),
}

pub type RespondStream = Pin<Box<dyn Stream<Item = Result<ChatCommand, StreamingError>> + Send>>;

pub struct CompletionParams<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    pub agent: Arc<Agent<M>>,
    pub handler: Arc<Mutex<aomi_tools::scheduler::ToolApiHandler>>,
    pub prompt: Message,
    pub chat_history: Vec<completion::Message>,
    pub system_events: SystemEventQueue,
}

pub struct CompletionRunner<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    params: CompletionParams<M>,
}

struct StreamState<R> {
    llm_stream:
        BoxStream<'static, Result<StreamedAssistantContent<R>, rig::completion::CompletionError>>,
    llm_finished: bool,
}

enum ProcessStep {
    Emit(Vec<ChatCommand>),
    Continue,
    Finished,
}

impl<M> CompletionRunner<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    pub fn new(params: CompletionParams<M>) -> Self {
        Self { params }
    }

    async fn init_stream_state(
        &self,
        chat_history: &mut Vec<completion::Message>,
        prompt: Message,
    ) -> Result<StreamState<<M as CompletionModel>::StreamingResponse>, StreamingError> {
        let llm_stream = self
            .params
            .agent
            .stream_completion(prompt.clone(), chat_history.clone())
            .await?
            .stream()
            .await?
            .fuse()
            .boxed();

        chat_history.push(prompt);

        Ok(StreamState {
            llm_stream,
            llm_finished: false,
        })
    }

    async fn consume_stream_item(
        &self,
        state: &mut StreamState<<M as CompletionModel>::StreamingResponse>,
        chat_history: &mut Vec<completion::Message>,
    ) -> Result<ProcessStep, StreamingError> {
        let next_item = state.llm_stream.next().await;
        match next_item {
            Some(Ok(StreamedAssistantContent::Text(text))) => {
                Ok(ProcessStep::Emit(vec![ChatCommand::StreamingText(
                    text.text,
                )]))
            }
            Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => Ok(ProcessStep::Emit(
                vec![ChatCommand::StreamingText(reasoning.reasoning)],
            )),
            Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                self.consume_tool_call(tool_call, state, chat_history)
                    .await
            }
            Some(Ok(StreamedAssistantContent::Final(_))) => Ok(ProcessStep::Continue),
            Some(Err(e)) => Err(e.into()),
            None => {
                state.llm_finished = true;
                Ok(ProcessStep::Finished)
            }
        }
    }

    async fn consume_tool_call(
        &self,
        tool_call: rig::message::ToolCall,
        _state: &mut StreamState<<M as CompletionModel>::StreamingResponse>,
        chat_history: &mut Vec<completion::Message>,
    ) -> Result<ProcessStep, StreamingError> {
        let mut commands = Vec::new();
        if let Some(cmd) = handle_wallet_transaction(&tool_call, &self.params.system_events) {
            commands.push(cmd);
        }

        let topic = {
            let guard = self.params.handler.lock().await;
            if let Some(topic_value) = tool_call.function.arguments.get("topic") {
                topic_value
                    .as_str()
                    .unwrap_or(&tool_call.function.name)
                    .to_string()
            } else {
                guard.get_topic(&tool_call.function.name)
            }
        };

        let ui_stream = process_tool_call(tool_call, chat_history, &self.params.handler).await?;

        commands.push(ChatCommand::ToolCall {
            topic,
            stream: ui_stream,
        });
        Ok(ProcessStep::Emit(commands))
    }

    pub async fn stream(self) -> RespondStream {
        let mut runner = self;
        let mut chat_history = std::mem::take(&mut runner.params.chat_history);
        let prompt = runner.params.prompt.clone();

        let chat_command_stream = async_stream::stream! {
            let mut state = match runner.init_stream_state(&mut chat_history, prompt.clone()).await {
                Ok(state) => state,
                Err(err) => {
                    yield Err(err);
                    return;
                }
            };

            // Process next item on stream
            loop {
                match runner.consume_stream_item(&mut state, &mut chat_history).await {
                    Ok(ProcessStep::Emit(commands)) => {
                        for command in commands {
                            yield Ok(command);
                        }
                    }
                    Ok(ProcessStep::Continue) => {}
                    Ok(ProcessStep::Finished) => break,
                    Err(err) => {
                        yield Err(err);
                        return;
                    }
                }

                if state.llm_finished {
                    break;
                }
            }

            // Finalize after LLM finishes; async updates are best-effort for this round.
        };

        chat_command_stream.boxed()
    }
}

fn handle_wallet_transaction(
    tool_call: &rig::message::ToolCall,
    system_events: &SystemEventQueue,
) -> Option<ChatCommand> {
    if tool_call.function.name.to_lowercase() != "send_transaction_to_wallet" {
        return None;
    }

    match tool_call.function.arguments.clone() {
        Value::Object(mut obj) => {
            obj.entry("timestamp".to_string())
                .or_insert_with(|| Value::String(Utc::now().to_rfc3339()));
            let payload = Value::Object(obj);
            system_events.push(SystemEvent::InlineDisplay(json!({
                "type": "wallet_tx_request",
                "payload": payload,
            })));
            None
        }
        _ => {
            let message = "send_transaction_to_wallet arguments must be an object".to_string();
            system_events.push(SystemEvent::SystemError(message.clone()));
            Some(ChatCommand::Error(message))
        }
    }
}

async fn process_tool_call(
    tool_call: rig::message::ToolCall,
    chat_history: &mut Vec<completion::Message>,
    handler: &Arc<tokio::sync::Mutex<aomi_tools::scheduler::ToolApiHandler>>,
) -> Result<ToolResultStream, StreamingError> {
    let rig::message::ToolFunction { name, arguments } = tool_call.function.clone();
    let scheduler = ToolScheduler::get_or_init().await?;

    // Add assistant message to chat history, required by the model API pattern
    chat_history.push(Message::Assistant {
        id: None,
        content: OneOrMany::one(AssistantContent::ToolCall(tool_call.clone())),
    });

    // Decide whether to use the native scheduler or the agent's tool registry (e.g. MCP tools)
    if scheduler.list_tool_names().contains(&name) {
        // Enqueue request - creates ToolReciever in pending_results
        let mut guard = handler.lock().await;
        guard.request(name, arguments, tool_call.id.clone()).await;

        // Retrieve the unresolved call and convert to streams. Add to ongoing streams
        guard
            .resolve_last_call()
            .ok_or_else(|| StreamingError::Eyre(eyre::eyre!("Tool call not found")))
    } else {
        Err(StreamingError::Eyre(eyre::eyre!(
            "Tool not registered in scheduler: {}",
            name
        )))
    }
}

pub async fn stream_completion<M>(
    agent: Arc<Agent<M>>,
    handler: Arc<Mutex<aomi_tools::scheduler::ToolApiHandler>>,
    prompt: impl Into<Message> + Send,
    chat_history: Vec<completion::Message>,
    system_events: SystemEventQueue,
) -> RespondStream
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: std::marker::Send,
{
    let params = CompletionParams {
        agent,
        handler,
        prompt: prompt.into(),
        chat_history,
        system_events,
    };

    CompletionRunner::new(params).stream().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aomi_tools::{abi_encoder, scheduler::ToolApiHandler, time, wallet};
    use eyre::{Context, Result};
    use futures::StreamExt;
    use rig::{agent::Agent, client::CompletionClient, completion, providers::anthropic};
    use std::sync::Arc;

    fn skip_without_anthropic_api_key() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_err()
    }

    async fn create_test_agent() -> Result<Arc<Agent<anthropic::completion::CompletionModel>>> {
        if skip_without_anthropic_api_key() {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return Err(eyre::eyre!("ANTHROPIC_API_KEY not set"));
        }
        let api_key = std::env::var("ANTHROPIC_API_KEY").wrap_err("ANTHROPIC_API_KEY not set")?;

        // Register tools in the global scheduler first
        let scheduler = ToolScheduler::new_for_test()
            .await
            .wrap_err("Failed to init scheduler")?;

        scheduler
            .register_tool(time::GetCurrentTime)
            .wrap_err("Failed to register time tool")?;
        scheduler
            .register_tool(wallet::SendTransactionToWallet)
            .wrap_err("Failed to register wallet tool")?;
        scheduler
            .register_tool(abi_encoder::EncodeFunctionCall)
            .wrap_err("Failed to register abi tool")?;

        let agent = anthropic::Client::new(&api_key)
            .agent("claude-sonnet-4-20250514")
            .preamble("You are a helpful assistant. Use tools when appropriate.")
            .tool(time::GetCurrentTime)
            .tool(wallet::SendTransactionToWallet)
            .tool(abi_encoder::EncodeFunctionCall)
            .build();

        Ok(Arc::new(agent))
    }

    async fn run_stream_test(
        agent: Arc<Agent<anthropic::completion::CompletionModel>>,
        prompt: &str,
        history: Vec<completion::Message>,
        handler: ToolApiHandler,
    ) -> (Vec<String>, usize) {
        let handler = Arc::new(Mutex::new(handler));
        // Get handler once per stream - it manages its own pending results
        let mut stream =
            stream_completion(agent, handler, prompt, history, SystemEventQueue::new()).await;
        let mut response_chunks = Vec::new();
        let mut tool_calls = 0;

        while let Some(result) = stream.next().await {
            match result {
                Ok(ChatCommand::StreamingText(text)) => {
                    response_chunks.push(text);
                }
                Ok(ChatCommand::ToolCall { topic, .. }) => {
                    tool_calls += 1;
                    response_chunks.push(format!("Tool: {}", topic));
                }
                Ok(ChatCommand::Error(e)) => panic!("Unexpected error: {}", e),
                Ok(_) => {} // Ignore other commands like Complete, BackendConnected, etc.
                Err(e) => panic!("Stream error: {}", e),
            }
        }

        (response_chunks, tool_calls)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_scheduler_setup() {
        let _agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => {
                println!("Skipping tool call tests without API key");
                return;
            }
        };

        // Verify scheduler has tools registered (ensure the global scheduler is seeded)
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        scheduler.register_tool(time::GetCurrentTime).unwrap();
        scheduler
            .register_tool(wallet::SendTransactionToWallet)
            .unwrap();
        scheduler
            .register_tool(abi_encoder::EncodeFunctionCall)
            .unwrap();
        let tool_names = scheduler.list_tool_names();

        println!("Registered tools: {:?}", tool_names);
        assert!(
            tool_names.contains(&"get_current_time".to_string()),
            "Time tool should be registered"
        );
        assert!(
            tool_names.contains(&"encode_function_call".to_string()),
            "ABI tool should be registered"
        );
        assert!(
            tool_names.contains(&"send_transaction_to_wallet".to_string()),
            "Wallet tool should be registered"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_basic_tool_call() {
        println!("🌧️");
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => {
                println!("Skipping tool call tests without API key");
                return;
            }
        };

        let scheduler = ToolScheduler::new_for_test().await.unwrap();
        let handler = scheduler.get_handler();

        let (chunks, tool_calls) = run_stream_test(
            agent,
            "Get the current time using the get_current_time tool",
            Vec::new(),
            handler,
        )
        .await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive response");
        let response = chunks.join("");
        assert!(response.len() > 50, "Should receive substantial response");

        if tool_calls > 0 {
            println!("✓ Tool calls detected: {}", tool_calls);
        } else {
            println!("⚠ No tool calls detected in response");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multi_round_conversation() {
        println!("🌧️");
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => {
                println!("Skipping tool call tests without API key");
                return;
            }
        };

        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        let handler = scheduler.get_handler();

        let history = vec![
            completion::Message::user("Hello"),
            completion::Message::assistant("Hi! How can I help you?"),
        ];

        let (chunks, _) = run_stream_test(agent, "What time is it?", history, handler).await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive response with history");
        println!("Multi-round test: {} response chunks", chunks.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multiple_tool_calls() {
        println!("🌧️");
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => {
                println!("Skipping tool call tests without API key");
                return;
            }
        };
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        let handler = scheduler.get_handler();

        let (chunks, tool_calls) = run_stream_test(
            agent,
            "Get the time right now and also encode this ABI function: transfer(address,uint256)",
            Vec::new(),
            handler,
        )
        .await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive response");
        let response = chunks.join("");

        println!("Multiple tools test:");
        println!("  Response length: {}", response.len());
        println!("  Tool calls detected: {}", tool_calls);

        // Check that both tools were mentioned in response
        let response_lower = response.to_lowercase();
        let mentions_time = response_lower.contains("time") || response_lower.contains("current");
        let mentions_abi = response_lower.contains("abi")
            || response_lower.contains("encode")
            || response_lower.contains("function");

        if mentions_time && mentions_abi {
            println!("✓ Both time and ABI encoding mentioned in response");
        } else {
            println!("⚠ Response: time={}, abi={}", mentions_time, mentions_abi);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_error_handling() {
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => {
                println!("Skipping tool call tests without API key");
                return;
            }
        };
        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        let handler = scheduler.get_handler();

        let (chunks, _) = run_stream_test(
            agent,
            "Call a nonexistent tool called 'fake_tool'",
            Vec::new(),
            handler,
        )
        .await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive error response");
        let response = chunks.join("");
        assert!(
            response.len() > 10,
            "Should receive meaningful error response"
        );

        println!("Error handling: received {} chars", response.len());
    }
}
