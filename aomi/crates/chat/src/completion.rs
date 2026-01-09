use aomi_tools::{ToolScheduler, ToolStream, scheduler::SessionToolHander};
use crate::{SystemEvent, SystemEventQueue, app::CoreState};
use chrono::Utc;
use futures::{Stream, StreamExt, stream::BoxStream};
use rig::{
    OneOrMany,
    agent::Agent,
    completion::CompletionModel,
    message::{AssistantContent, Message},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::ToolSetError as RigToolError,
};
use serde_json::{Value, json};
use std::{collections::HashSet, pin::Pin, sync::Arc};
use thiserror::Error;
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

// Type alias for CoreCommand with ToolStreamream
pub type CoreCommand = crate::CoreCommand<ToolStream>;
pub type CoreCommandStream = Pin<Box<dyn Stream<Item = Result<CoreCommand, StreamingError>> + Send>>;

pub struct CompletionRunner<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    agent: Arc<Agent<M>>,
    prompt: Message,
    state: CoreState,
    handler: Option<SessionToolHander>,
}

struct StreamState<R> {
    llm_stream:
        BoxStream<'static, Result<StreamedAssistantContent<R>, rig::completion::CompletionError>>,
    llm_finished: bool,
}

enum ProcessStep {
    Emit(Vec<CoreCommand>),
    Continue,
    Finished,
}

impl<M> CompletionRunner<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    pub fn new(
        agent: Arc<Agent<M>>,
        prompt: Message,
        state: CoreState,
        handler: Option<SessionToolHander>,
    ) -> Self {
        Self {
            agent,
            prompt,
            state,
            handler,
        }
    }

    async fn init_stream_state(
        &mut self,
    ) -> Result<StreamState<<M as CompletionModel>::StreamingResponse>, StreamingError> {
        let llm_stream = self
            .agent
            .stream_completion(self.prompt.clone(), self.state.history.clone())
            .await?
            .stream()
            .await?
            .fuse()
            .boxed();

        self.state.history.push(self.prompt.clone());

        Ok(StreamState {
            llm_stream,
            llm_finished: false,
        })
    }

    async fn consume_stream_item(
        &mut self,
        state: &mut StreamState<<M as CompletionModel>::StreamingResponse>,
    ) -> Result<ProcessStep, StreamingError> {
        let next_item = state.llm_stream.next().await;
        match next_item {
            Some(Ok(StreamedAssistantContent::Text(text))) => {
                Ok(ProcessStep::Emit(vec![CoreCommand::StreamingText(
                    text.text,
                )]))
            }
            Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => Ok(ProcessStep::Emit(
                vec![CoreCommand::StreamingText(reasoning.reasoning)],
            )),
            Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                self.consume_tool_call(tool_call).await
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
        &mut self,
        tool_call: rig::message::ToolCall,
    ) -> Result<ProcessStep, StreamingError> {
        let mut commands = Vec::new();
        if let Some(cmd) = self.consume_system_events(&tool_call) {
            commands.push(cmd);
        }

        let Some(handler) = self.handler.clone() else {
            let (topic, stream) = self.process_tool_call_fallback(tool_call).await?;
            commands.push(CoreCommand::ToolCall { topic, stream });
            return Ok(ProcessStep::Emit(commands));
        };

        let topic = {
            let guard = handler.lock().await;
            if let Some(topic_value) = tool_call.function.arguments.get("topic") {
                topic_value
                    .as_str()
                    .unwrap_or(&tool_call.function.name)
                    .to_string()
            } else {
                guard.get_topic(&tool_call.function.name)
            }
        };

        let ui_stream = self.process_tool_call(tool_call, &handler).await?;

        commands.push(CoreCommand::ToolCall {
            topic,
            stream: ui_stream,
        });

        Ok(ProcessStep::Emit(commands))
    }

    async fn process_tool_call_fallback(
        &mut self,
        tool_call: rig::message::ToolCall,
    ) -> Result<(String, ToolStream), StreamingError> {
        let tool_name = tool_call.function.name.clone();
        let topic = match tool_call.function.arguments.get("topic") {
            Some(v) => v.as_str().unwrap_or(&tool_name).to_string(),
            None => tool_name.clone(),
        };

        self.state.push_tool_call(&tool_call);

        let args = serde_json::to_string(&tool_call.function.arguments)
            .unwrap_or_else(|_| tool_call.function.arguments.to_string());
        let result = self.agent.tools.call(&tool_name, args).await?;

        let result_value = serde_json::from_str(&result).unwrap_or(Value::String(result));
        let result_text = serde_json::to_string_pretty(&result_value)
            .unwrap_or_else(|_| result_value.to_string());
        self.state
            .push_sync_update(tool_call.id.clone(), result_text);

        let result_stream =
            ToolStream::from_result(tool_call.id.clone(), Ok(result_value), tool_name.clone());

        Ok((topic, result_stream))
    }

    pub async fn stream(self) -> CoreCommandStream {
        let mut runner = self;
        if let Some(events) = runner.state.system_events.clone() {
            runner.ingest_llm_events(&events);
        }

        let chat_command_stream = async_stream::stream! {
            let mut state = match runner.init_stream_state().await {
                Ok(state) => state,
                Err(err) => {
                    yield Err(err);
                    return;
                }
            };

            // Process next item on stream
            loop {
                match runner.consume_stream_item(&mut state).await {
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

    // Event updates going into the model
    fn ingest_llm_events(&mut self, system_events: &SystemEventQueue) {
        let mut seen_updates = HashSet::new();
        for event in system_events.advance_llm_events() {
            match &event {
                SystemEvent::SystemError(message) => {
                    self.state
                        .history
                        .push(Message::user(format!("[[SYSTEM]] {}", message)));
                }
                SystemEvent::SyncUpdate(value) => {
                    if let Some((call_id, _tool_name, result_text)) = tool_update_from_value(value) {
                        let update_key = format!("{}:{}", call_id, result_text);
                        if !seen_updates.insert(update_key) {
                            continue;
                        }
                        self.state.push_sync_update(call_id, result_text);
                    }
                }
                SystemEvent::AsyncUpdate(value) => {
                    if let Some((call_id, tool_name, result_text)) = tool_update_from_value(value) {
                        let update_key = format!("{}:{}", call_id, result_text);
                        if !seen_updates.insert(update_key) {
                            continue;
                        }
                        self.state
                            .push_async_update(tool_name, call_id, result_text);
                    }
                }
                _ => {}
            }
        }
    }

    fn consume_system_events(
        &mut self,
        tool_call: &rig::message::ToolCall,
    ) -> Option<CoreCommand> {
        let system_events = self.state.system_events.as_ref()?;
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
                Some(CoreCommand::Error(message))
            }
        }
    }

    async fn process_tool_call(
        &mut self,
        tool_call: rig::message::ToolCall,
        handler: &SessionToolHander,
    ) -> Result<ToolStream, StreamingError> {
        let rig::message::ToolFunction { name, arguments } = tool_call.function.clone();
        let scheduler = ToolScheduler::get_or_init().await?;

        // Add assistant message to chat history, required by the model API pattern
        self.state.push_tool_call(&tool_call);

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
}

fn tool_update_from_value(value: &Value) -> Option<(String, String, String)> {
    let call_id = value.get("call_id")?.as_str()?.to_string();
    let tool_name = value.get("tool_name")?.as_str()?.to_string();
    let result = value.get("result")?.clone();
    let result_text = if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        format!("tool_error: {}", error)
    } else {
        serde_json::to_string_pretty(&result).unwrap_or_else(|err| {
            format!("tool_error: failed to serialize tool result: {}", err)
        })
    };
    Some((call_id, tool_name, result_text))
}


pub async fn stream_completion<M>(
    agent: Arc<Agent<M>>,
    prompt: impl Into<Message> + Send,
    state: CoreState,
    handler: Option<SessionToolHander>,
) -> CoreCommandStream
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: std::marker::Send,
{
    CompletionRunner::new(agent, prompt.into(), state, handler)
        .stream()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aomi_tools::{abi_encoder, scheduler::ToolHandler, time, wallet};
    use eyre::{Context, Result};
    use futures::StreamExt;
    use rig::{agent::Agent, client::CompletionClient, completion, providers::anthropic};
    use tokio::sync::Mutex;
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
        handler: ToolHandler,
    ) -> (Vec<String>, usize) {
        let handler = Arc::new(Mutex::new(handler));
        let state = CoreState {
            history,
            system_events: Some(SystemEventQueue::new()),
        };
        // Get handler once per stream - it manages its own pending results
        let mut stream = stream_completion(agent, prompt, state, Some(handler)).await;
        let mut response_chunks = Vec::new();
        let mut tool_calls = 0;

        while let Some(result) = stream.next().await {
            match result {
                Ok(CoreCommand::StreamingText(text)) => {
                    response_chunks.push(text);
                }
                Ok(CoreCommand::ToolCall { topic, .. }) => {
                    tool_calls += 1;
                    response_chunks.push(format!("Tool: {}", topic));
                }
                Ok(CoreCommand::Error(e)) => panic!("Unexpected error: {}", e),
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

        let scheduler = ToolScheduler::get_or_init().await.unwrap();
        scheduler.register_tool(time::GetCurrentTime).unwrap();
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
        assert!(
            tool_calls > 0 || response.len() > 50,
            "Should receive tool call or substantial response"
        );

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
        scheduler.register_tool(time::GetCurrentTime).unwrap();
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
