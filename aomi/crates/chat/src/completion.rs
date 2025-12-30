use aomi_tools::{ToolResultStream, ToolScheduler};
// Type alias for ChatCommand with ToolResultStream
pub type ChatCommand = crate::ChatCommand<ToolResultStream>;

use crate::{SystemEvent, SystemEventQueue};
use chrono::Utc;
use futures::{Stream, StreamExt};
use rig::{
    OneOrMany,
    agent::Agent,
    completion::{self, CompletionModel},
    message::{AssistantContent, Message, ToolResultContent},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::ToolSetError as RigToolError,
};
use tokio::sync::Mutex;
use aomi_tools::scheduler::ToolApiHandler;
use serde_json::{Value, json};
use std::{pin::Pin, sync::Arc};
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

pub type RespondStream = Pin<Box<dyn Stream<Item = Result<ChatCommand, StreamingError>> + Send>>;

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

async fn process_tool_call<M>(
    _agent: Arc<Agent<M>>,
    tool_call: rig::message::ToolCall,
    chat_history: &mut Vec<completion::Message>,
    handler: &Arc<Mutex<ToolApiHandler>>,
) -> Result<ToolResultStream, StreamingError>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
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

        // Retrieve the unresolved call and convert to streams
        guard
            .resolve_last_call()
            .ok_or_else(|| StreamingError::Eyre(eyre::eyre!("Tool call not found")))
    } else {
        // Rig fallback:
        // Fallback to Rig's tool registry (e.g. MCP tools)
        // let result = agent.tools.call(&name, arguments.to_string()).await;
        // let tool_result: Result<Value, String> = match result {
        //     Ok(value_str) => {
        //         // Try to parse as JSON, fallback to string value
        //         Ok(serde_json::from_str(&value_str).unwrap_or_else(|_| Value::String(value_str)))
        //     }
        //     Err(e) => Err(e.to_string()),
        // };
        // // Add tool result to chat history immediately
        // finalize_tool_result(chat_history, tool_call.id.clone(), tool_result.clone());
        // // Return a stream with the result for UI ACK
        // Ok(ToolResultStream::from_result(
        //     tool_call.id,
        //     tool_result,
        //     name,
        //     false, // Rig tools are not multi-step
        // ))
        unreachable!()
    }
}

/// Poll ongoing_streams for ready items and append results to chat_history.
/// Streams that yield are removed, ongoing streams remain for next iteration.
fn finalize_sync_tool(
    chat_history: &mut Vec<completion::Message>,
    call_id: String,
    tool_result: Result<Value, String>,
) {
    let result_text = match tool_result {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
        Err(err) => format!("tool_error: {}", err),
    };
    chat_history.push(Message::User {
        content: OneOrMany::one(rig::message::UserContent::tool_result(
            call_id,
            OneOrMany::one(ToolResultContent::text(result_text)),
        )),
    });
}

fn finalize_async_completion(
    chat_history: &mut Vec<completion::Message>,
    call_id: String,
    tool_name: String,
    result: Result<Value, String>,
) {
    let system_hint = Message::user("[[SYSTEM]]: Asynchronous tool result is ready. Continue with the results.");
    let result_text = match result {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
        Err(err) => format!("tool_error: {}", err),
    };

    let tool_result = Message::user(format!("[[SYSTEM]] Tool result for {} with call id {}: {}", tool_name, call_id, result_text));
    chat_history.push(system_hint);
    chat_history.push(tool_result);
}

pub async fn stream_completion<M>(
    agent: Arc<Agent<M>>,
    handler: Arc<Mutex<ToolApiHandler>>,
    prompt: impl Into<Message> + Send,
    mut chat_history: Vec<completion::Message>,
    system_events: SystemEventQueue,
) -> RespondStream
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: std::marker::Send,
{
    let prompt: Message = prompt.into();

    let chat_command_stream = async_stream::stream! {
        // Full prompt might be appended with System message and continuation hint
        let full_prompt = prompt;

        'outer: loop {
            let mut llm_stream = agent
                .stream_completion(full_prompt.clone(), chat_history.clone())
                .await?
                .stream()
                .await?
                .fuse();

            chat_history.push(full_prompt.clone());

            let mut llm_finished = false;
            let mut expected_sync_calls: Vec<String> = Vec::new();
            loop {

                if llm_finished {
                    break;
                }

                match llm_stream.next().await {
                    Some(Ok(StreamedAssistantContent::Text(text))) => {
                        yield Ok(ChatCommand::StreamingText(text.text));
                    }
                    Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => {
                        yield Ok(ChatCommand::StreamingText(reasoning.reasoning));
                    }
                    Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                        if let Some(msg) = handle_wallet_transaction(&tool_call, &system_events) {
                            yield Ok(msg);
                        }

                        // Try to get topic from arguments, otherwise use static topic
                        let topic = if let Some(topic_value) = tool_call.function.arguments.get("topic") {
                            topic_value.as_str()
                                .unwrap_or(&tool_call.function.name)
                                .to_string()
                        } else {
                            // Get static topic from handler (no longer async)
                            let guard = handler.lock().await;
                            guard.get_topic(&tool_call.function.name)
                        };

                        // Unified API: process_tool_call handles both single and multi-step
                        let ui_stream = match process_tool_call(
                            agent.clone(),
                            tool_call.clone(),
                            &mut chat_history,
                                &handler
                            ).await {
                                Ok(stream) => stream,
                                Err(err) => {
                                yield Err(err); // This err only happens when scheduling fails
                                break 'outer;   // Not actual call, should break since it's a system issue
                            }
                        };

                        let call_id = ui_stream.call_id.clone();
                        expected_sync_calls.push(call_id);

                        yield Ok(ChatCommand::ToolCall {
                            topic,
                            stream: ui_stream,
                        });
                    }
                    Some(Ok(StreamedAssistantContent::Final(_))) => {
                        // Final message with usage statistics - ignored
                    }
                    Some(Err(e)) => {
                        yield Err(e.into());
                        break 'outer;
                    }
                    None => {
                        llm_finished = true;
                    }
                }
            }

            // After LLM finishes, consume tool completions for the LLM path (no UI yields here).
            // Wait briefly for expected sync completions; async updates are best-effort for this round.
            finalization(&system_events, &mut expected_sync_calls, &mut chat_history).await;
            break 'outer;
        }
    };

    chat_command_stream.boxed()
}

async fn finalization(
    system_events: &SystemEventQueue,
    expected_sync_calls: &mut Vec<String>,
    chat_history: &mut Vec<completion::Message>,
) {
    let mut iteration_cnt = 0usize;
    const MAX_IDLE_LOOPS: usize = 10;

    loop {
        let events = system_events.advance_llm_events();
        let mut handled_any = false;

        for event in events {
            match event {
                SystemEvent::SyncUpdate(value) => {
                    if let (Some(call_id), Some(_tool_name), Some(result)) = (
                        value.get("call_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        value.get("tool_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        value.get("result").cloned(),
                    ) {
                        handled_any = true;
                        expected_sync_calls.retain(|c| c != &call_id);
                        finalize_sync_tool(chat_history, call_id, Ok(result));
                    }
                }
                SystemEvent::AsyncUpdate(value) => {
                    if let (Some(call_id), Some(tool_name), Some(result)) = (
                        value.get("call_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        value.get("tool_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        value.get("result").cloned(),
                    ) {
                        handled_any = true;
                        finalize_async_completion(chat_history, call_id, tool_name, Ok(result));
                    }
                }
                _ => {}
            }
        }
        if expected_sync_calls.is_empty() {
            break;
        }

        if handled_any {
            iteration_cnt = 0;
            continue;
        }

        if iteration_cnt >= MAX_IDLE_LOOPS {
            break;
        }

        iteration_cnt += 1;
    }
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
        // Either we get a substantial text response, or we get tool calls
        assert!(
            response.len() > 20 || tool_calls > 0,
            "Should receive substantial response or tool calls"
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
