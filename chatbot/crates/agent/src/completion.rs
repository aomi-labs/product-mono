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
