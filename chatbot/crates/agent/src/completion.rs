use std::{pin::Pin, sync::Arc};

use chrono::Utc;
use futures::{FutureExt, Stream, StreamExt, stream::FuturesUnordered};
use rig::{
    OneOrMany,
    agent::Agent,
    completion::{self, CompletionError, CompletionModel, PromptError},
    message::{AssistantContent, Message, ToolResultContent, UserContent},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::{ToolError, ToolSetError},
};
use serde_json::Value;
use thiserror::Error;


#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("CompletionError: {0}")]
    Completion(#[from] CompletionError),
    #[error("PromptError: {0}")]
    Prompt(#[from] PromptError),
    #[error("ToolSetError: {0}")]
    Tool(#[from] ToolSetError),
}


type ToolResultFuture = futures::future::BoxFuture<'static, Result<(String, String), ToolSetError>>;

/// Helper function to stream a completion request to stdout and return the full response
#[allow(dead_code)]
pub(crate) async fn custom_stream_to_stdout(
    stream: &mut RespondStream,
) -> Result<String, std::io::Error> {
    println!();

    let mut response = String::new();
    while let Some(content) = stream.next().await {
        match content {
            Ok(RespondMessage::Text(text)) => {
                print!("{text}");
                std::io::Write::flush(&mut std::io::stdout())?;
                response.push_str(&text);
            }
            Ok(RespondMessage::System(system)) => {
                println!("\n[system] {system}");
            }
            Ok(RespondMessage::Error(err)) => {
                eprintln!("[error] {err}");
            }
            Err(err) => {
                eprintln!("Error: {err}");
            }
        }
    }
    // Only add newline if response doesn't already end with one
    if !response.ends_with('\n') {
        println!();
    }

    // One more to separate from user input
    println!();

    Ok(response)
}


pub type RespondStream = Pin<Box<dyn Stream<Item = Result<RespondMessage, StreamingError>> + Send>>;
#[derive(Debug)]
pub enum RespondMessage {
    Text(String),
    System(String),
    Error(String),
}


pub async fn stream_completion<M>(
    agent: Arc<Agent<M>>,
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
        let mut pending_results: FuturesUnordered<ToolResultFuture> = FuturesUnordered::new();

        'outer: loop {
            debug_assert!(pending_results.is_empty());

            let mut stream = agent
                .stream_completion(current_prompt.clone(), chat_history.clone())
                .await?
                .stream()
                .await?
                .fuse();

            chat_history.push(current_prompt.clone());

            let mut tool_results = vec![];
            let mut did_call_tool = false;
            let mut stream_finished = false;

            loop {
                if stream_finished && pending_results.is_empty() {
                    break;
                }

                tokio::select! {
                    result = pending_results.select_next_some(), if !pending_results.is_empty() => {
                        match result {
                            Ok((call_id, output)) => {
                                tool_results.push((call_id, output));
                            }
                            Err(err) => {
                                yield Err(err.into());
                                break 'outer;
                            }
                        }
                    },
                    maybe_content = stream.next(), if !stream_finished => {
                        match maybe_content {
                            Some(Ok(StreamedAssistantContent::Text(text))) => {
                                yield Ok(RespondMessage::Text(text.text));
                            }
                            Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => {
                                // Stream reasoning text as well
                                yield Ok(RespondMessage::Text(reasoning.reasoning));
                            }
                            Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                                if tool_call.function.name.to_lowercase() == "send_transaction_to_wallet" {
                                    match tool_call.function.arguments.clone() {
                                        Value::Object(mut obj) => {
                                            obj.entry("timestamp".to_string())
                                                .or_insert_with(|| Value::String(Utc::now().to_rfc3339()));
                                            let payload = Value::Object(obj);
                                            let message = serde_json::json!({
                                                "wallet_transaction_request": payload
                                            });
                                            yield Ok(RespondMessage::System(message.to_string()));
                                        }
                                        _ => {
                                            yield Ok(RespondMessage::Error(
                                                "send_transaction_to_wallet arguments must be an object".to_string(),
                                            ));
                                        }
                                    }
                                }
                                let (message, future) = match register_tool_call(agent.clone(), tool_call.clone()).await {
                                    Ok(value) => value,
                                    Err(err) => {
                                        yield Err(err.into());
                                        break 'outer;
                                    }
                                };

                                chat_history.push(Message::Assistant {
                                    id: None,
                                    content: OneOrMany::one(message),
                                });

                                pending_results.push(future);

                                yield Ok(RespondMessage::Text(format!(
                                    "\nAwaiting tool `{}` …",
                                    tool_call.function.name
                                )));

                                did_call_tool = true;
                            }
                            Some(Ok(StreamedAssistantContent::Final(_))) => {
                                // Final message handling - typically contains usage statistics
                                // We can ignore this for now or log it
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

            for (id, tool_result) in tool_results {
                chat_history.push(Message::User {
                    content: OneOrMany::one(UserContent::tool_result(
                        id,
                        OneOrMany::one(ToolResultContent::text(tool_result)),
                    )),
                });
            }

            current_prompt = match chat_history.pop() {
                Some(prompt) => prompt,
                None => unreachable!("Chat history should never be empty at this point"),
            };

            if !did_call_tool {
                break;
            }
        }

    })) as _
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use rig::{agent::Agent, client::CompletionClient, completion, providers::anthropic};
    use std::sync::Arc;
    use crate::{time, wallet, abi_encoder};

    async fn create_test_agent() -> Result<Arc<Agent<anthropic::completion::CompletionModel>>, Box<dyn std::error::Error>> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY not set")?;
        
        // Register tools in the global scheduler first
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init().await
            .map_err(|e| format!("Failed to init scheduler: {}", e))?;
            
        scheduler.register_tool(time::GetCurrentTime)
            .map_err(|e| format!("Failed to register time tool: {}", e))?;
        scheduler.register_tool(wallet::SendTransactionToWallet)
            .map_err(|e| format!("Failed to register wallet tool: {}", e))?;
        scheduler.register_tool(abi_encoder::EncodeFunctionCall)
            .map_err(|e| format!("Failed to register abi tool: {}", e))?;
            
        let agent = anthropic::Client::new(&api_key)
            .agent("claude-sonnet-4-20250514")
            .preamble("You are a helpful assistant. Use tools when appropriate.")
            .tool(time::GetCurrentTime)
            .tool(wallet::SendTransactionToWallet)
            .tool(abi_encoder::EncodeFunctionCall)
            .build();
            
        Ok(Arc::new(agent))
    }

    async fn run_stream_test(agent: Arc<Agent<anthropic::completion::CompletionModel>>, prompt: &str, history: Vec<completion::Message>) -> (Vec<String>, usize) {
        let mut stream = stream_completion(agent, prompt, history).await;
        let mut response_chunks = Vec::new();
        let mut tool_calls = 0;

        while let Some(result) = stream.next().await {
            match result {
                Ok(RespondMessage::Text(text)) => {
                    if text.contains("Awaiting tool") {
                        tool_calls += 1;
                    }
                    response_chunks.push(text);
                }
                Ok(RespondMessage::System(_)) => {}
                Ok(RespondMessage::Error(e)) => panic!("Unexpected error: {}", e),
                Err(e) => panic!("Stream error: {}", e),
            }
        }
        
        (response_chunks, tool_calls)
    }

    #[tokio::test]
    async fn test_scheduler_setup() {
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };

        // Verify scheduler has tools registered
        let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init().await.unwrap();
        let tool_names = scheduler.list_tool_names();
        
        println!("Registered tools: {:?}", tool_names);
        assert!(tool_names.contains(&"get_current_time".to_string()), "Time tool should be registered");
        assert!(tool_names.contains(&"encode_function_call".to_string()), "ABI tool should be registered");
        assert!(tool_names.contains(&"send_transaction_to_wallet".to_string()), "Wallet tool should be registered");
    }

    #[tokio::test]
    async fn test_basic_tool_call() {
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return, // Skip if no API key
        };

        let (chunks, tool_calls) = run_stream_test(
            agent, 
            "Get the current time using the get_current_time tool", 
            Vec::new()
        ).await;
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
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };

        let history = vec![
            completion::Message::user("Hello"),
            completion::Message::assistant("Hi! How can I help you?"),
        ];

        let (chunks, _) = run_stream_test(
            agent,
            "What time is it?",
            history
        ).await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive response with history");
        println!("Multi-round test: {} response chunks", chunks.len());
    }

    #[tokio::test]
    async fn test_multiple_tool_calls() {
        let agent = match create_test_agent().await {
            Ok(agent) => agent,
            Err(_) => return,
        };

        let (chunks, tool_calls) = run_stream_test(
            agent,
            "Get the time right now and also encode this ABI function: transfer(address,uint256)",
            Vec::new()
        ).await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive response");
        let response = chunks.join("");
        
        
        println!("Multiple tools test:");
        println!("  Response length: {}", response.len());
        println!("  Tool calls detected: {}", tool_calls);
        
        // Check that both tools were mentioned in response
        let response_lower = response.to_lowercase();
        let mentions_time = response_lower.contains("time") || response_lower.contains("current");
        let mentions_abi = response_lower.contains("abi") || response_lower.contains("encode") || response_lower.contains("function");
        
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

        let (chunks, _) = run_stream_test(
            agent,
            "Call a nonexistent tool called 'fake_tool'",
            Vec::new()
        ).await;
        println!("chunks {:?}", chunks);

        assert!(!chunks.is_empty(), "Should receive error response");
        let response = chunks.join("");
        assert!(response.len() > 10, "Should receive meaningful error response");
        
        println!("Error handling: received {} chars", response.len());
    }
}

async fn register_tool_call<M>(
    agent: Arc<Agent<M>>,
    tool_call: rig::message::ToolCall,
) -> Result<(AssistantContent, ToolResultFuture), ToolSetError>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{

    let rig::message::ToolFunction {name, arguments}  = tool_call.function.clone();
    let scheduler = crate::tool_scheduler::ToolScheduler::get_or_init()
        .await
        .map_err(|e| ToolSetError::from(ToolError::ToolCallError(e.to_string().into())))?;
    let mut handler = scheduler.get_handler();
    // Decide whether to use the native scheduler or the agent's tool registry (e.g. MCP tools)
    let future: ToolResultFuture = if scheduler.list_tool_names().contains(&name){
        // Make the async request to the scheduler
        let receiver = handler
            .request_with_json(name.clone(), arguments)
            .await;

        let tool_id = tool_call.id.clone();
        async move {
            match receiver.await {
                Ok(Ok(json_response)) => {
                    // Convert the JSON response back to a string for the chat transcript
                    let output = serde_json::to_string_pretty(&json_response)
                        .unwrap_or_else(|_| "Tool execution successful".to_string());
                    Ok((tool_id, output))
                }
                Ok(Err(err)) => Err(ToolSetError::from(ToolError::ToolCallError(
                    format!("Tool execution failed: {}", err).into(),
                ))),
                Err(_) => Err(ToolSetError::from(ToolError::ToolCallError(
                    "Tool scheduler channel closed unexpectedly".into(),
                ))),
            }
        }
        .boxed()
    } else {
        let tool_id = tool_call.id.clone();
        async move {
            agent
                .tools
                .call(&name, arguments.to_string())
                .await
                .map(|output| (tool_id, output))
        }
        .boxed()
    };

    Ok((AssistantContent::ToolCall(tool_call), future))
}
