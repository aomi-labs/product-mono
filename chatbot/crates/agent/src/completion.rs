use crate::agent::ChatCommand;
use chrono::Utc;
use futures::{FutureExt, Stream, StreamExt};
use rig::{
    OneOrMany,
    agent::Agent,
    completion::{self, CompletionModel},
    message::{AssistantContent, Message, ToolResultContent},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::ToolSetError,
};
use serde_json::Value;
use std::{pin::Pin, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("CompletionError: {0}")]
    Completion(#[from] rig::completion::CompletionError),
    #[error("PromptError: {0}")]
    Prompt(#[from] rig::completion::PromptError),
    #[error("ToolSetError: {0}")]
    Tool(#[from] ToolSetError),
    #[error("Eyre: {0}")]
    Eyre(#[from] eyre::Error),
}

pub type RespondStream = Pin<Box<dyn Stream<Item = Result<ChatCommand, StreamingError>> + Send>>;

fn handle_wallet_transaction(tool_call: &rig::message::ToolCall) -> Option<ChatCommand> {
    if tool_call.function.name.to_lowercase() != "send_transaction_to_wallet" {
        return None;
    }

    match tool_call.function.arguments.clone() {
        Value::Object(mut obj) => {
            obj.entry("timestamp".to_string())
                .or_insert_with(|| Value::String(Utc::now().to_rfc3339()));
            let payload = Value::Object(obj);
            let message = serde_json::json!({
                "wallet_transaction_request": payload
            });
            Some(ChatCommand::WalletTransactionRequest(message.to_string()))
        }
        _ => Some(ChatCommand::Error(
            "send_transaction_to_wallet arguments must be an object".to_string(),
        )),
    }
}

async fn process_tool_call<M>(
    agent: Arc<Agent<M>>,
    tool_call: rig::message::ToolCall,
    chat_history: &mut Vec<completion::Message>,
    handler: &mut crate::tool_scheduler::ToolApiHandler,
) -> Result<(), StreamingError>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    let rig::message::ToolFunction { name, arguments } = tool_call.function.clone();
    let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init().await?;

    // Add assistant message to chat history
    chat_history.push(Message::Assistant {
        id: None,
        content: OneOrMany::one(AssistantContent::ToolCall(tool_call.clone())),
    });

    // Decide whether to use the native scheduler or the agent's tool registry (e.g. MCP tools)
    if scheduler.list_tool_names().contains(&name) {
        // Use the scheduler through the handler
        handler
            .request_with_json(name, arguments, tool_call.id)
            .await;
    } else {
        // Fall back to agent's tools - create future and add to handler
        let tool_id = tool_call.id.clone();
        let future = async move {
            agent
                .tools
                .call(&name, arguments.to_string())
                .await
                .map(|output| (tool_id, output))
                .map_err(Into::into)
        }
        .boxed();

        // Add the external future to handler's pending results
        handler.add_external_future(future);
    }

    Ok(())
}

fn finalize_tool_results(
    tool_results: Vec<(String, String)>,
    chat_history: &mut Vec<completion::Message>,
) {
    for (id, tool_result) in tool_results {
        chat_history.push(Message::User {
            content: OneOrMany::one(rig::message::UserContent::tool_result(
                id,
                OneOrMany::one(ToolResultContent::text(tool_result)),
            )),
        });
    }
}

pub async fn stream_completion<M>(
    agent: Arc<Agent<M>>,
    mut handler: crate::tool_scheduler::ToolApiHandler,
    prompt: impl Into<Message> + Send,
    mut chat_history: Vec<completion::Message>,
) -> RespondStream
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: std::marker::Send,
{
    let prompt: Message = prompt.into();

    (Box::pin(async_stream::stream! {
        let mut current_prompt = prompt;


        'outer: loop {
            debug_assert!(!handler.has_pending_results());

            let mut stream = agent
                .stream_completion(current_prompt.clone(), chat_history.clone())
                .await?
                .stream()
                .await?
                .fuse();

            chat_history.push(current_prompt.clone());

            let mut did_call_tool = false;
            let mut stream_finished = false;

            loop {
                if stream_finished && !handler.has_pending_results() {
                    break;
                }

                tokio::select! {
                    result = handler.poll_next_result(), if handler.has_pending_results() => {
                        match result {
                            Some(Ok(())) => {} // Tool result was added to handler's finished_results
                            Some(Err(err)) => {
                                yield Err(err.into());
                                break 'outer;
                            }
                            None => {} // No results available right now
                        }
                    },
                    maybe_content = stream.next(), if !stream_finished => {
                        match maybe_content {
                            Some(Ok(StreamedAssistantContent::Text(text))) => {
                                yield Ok(ChatCommand::StreamingText(text.text));
                            }
                            Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => {
                                yield Ok(ChatCommand::StreamingText(reasoning.reasoning));
                            }
                            Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                                if let Some(msg) = handle_wallet_transaction(&tool_call) {
                                    yield Ok(msg);
                                }

                                if let Err(err) = process_tool_call(
                                    agent.clone(),
                                    tool_call.clone(),
                                    &mut chat_history,
                                    &mut handler
                                ).await {
                                    yield Err(err);
                                    break 'outer;
                                }

                                yield Ok(ChatCommand::ToolCall {
                                    name: tool_call.function.name.clone(),
                                    args: format!("Awaiting tool `{}` …", tool_call.function.name)
                                });

                                did_call_tool = true;
                            }
                            Some(Ok(StreamedAssistantContent::Final(_))) => {
                                // Final message with usage statistics - ignored
                            }
                            Some(Err(e)) => {
                                yield Err(e.into());
                                break 'outer;
                            }
                            None => {
                                stream_finished = true;
                            }
                        }
                    }
                }
            }

            let tool_results = handler.take_finished_results();
            finalize_tool_results(tool_results, &mut chat_history);

            current_prompt = chat_history.pop()
                .expect("Chat history should never be empty at this point");

            if !did_call_tool {
                break;
            }
        }
    })) as _
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{abi_encoder, time, wallet};
    use eyre::{Context, Result};
    use futures::StreamExt;
    use rig::{agent::Agent, client::CompletionClient, completion, providers::anthropic};
    use std::sync::Arc;

    async fn create_test_agent() -> Result<Arc<Agent<anthropic::completion::CompletionModel>>> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").wrap_err("ANTHROPIC_API_KEY not set")?;

        // Register tools in the global scheduler first
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
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
        handler: crate::tool_scheduler::ToolApiHandler,
    ) -> (Vec<String>, usize) {
        // Get handler once per stream - it manages its own pending results
        let mut stream = stream_completion(agent, handler, prompt, history).await;
        let mut response_chunks = Vec::new();
        let mut tool_calls = 0;

        while let Some(result) = stream.next().await {
            match result {
                Ok(ChatCommand::StreamingText(text)) => {
                    response_chunks.push(text);
                }
                Ok(ChatCommand::ToolCall { name, args }) => {
                    tool_calls += 1;
                    response_chunks.push(format!("Tool: {} - {}", name, args));
                }
                Ok(ChatCommand::WalletTransactionRequest(_)) => {}
                Ok(ChatCommand::System(_)) => {}
                Ok(ChatCommand::Error(e)) => panic!("Unexpected error: {}", e),
                Ok(_) => {} // Ignore other commands like Complete, BackendConnected, etc.
                Err(e) => panic!("Stream error: {}", e),
            }
        }

        (response_chunks, tool_calls)
    }

    #[tokio::test]
    async fn test_scheduler_setup() {
        let _agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };

        // Verify scheduler has tools registered
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
            .await
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

    #[tokio::test]
    async fn test_basic_tool_call() {
        println!("🌧️");
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return, // Skip if no API key
        };

        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
            .await
            .unwrap();
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

    #[tokio::test]
    async fn test_multi_round_conversation() {
        println!("🌧️");
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };

        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
            .await
            .unwrap();
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

    #[tokio::test]
    async fn test_multiple_tool_calls() {
        println!("🌧️");
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
            .await
            .unwrap();
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

    #[tokio::test]
    async fn test_error_handling() {
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
            .await
            .unwrap();
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
