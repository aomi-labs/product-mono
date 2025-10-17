use std::{
    pin::Pin,
    sync::{Arc, OnceLock},
};

use crate::tool_scheduler::{ToolApiHandler, ToolScheduler};
use futures::{FutureExt, Stream, StreamExt, stream::FuturesUnordered};
pub use rig::message::Text;
use rig::{
    OneOrMany,
    agent::Agent,
    completion::{self, CompletionError, CompletionModel, PromptError},
    message::{AssistantContent, Message, ToolResultContent, UserContent},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::{ToolError, ToolSetError},
};
use thiserror::Error;

// Global singleton for the tool scheduler handler
pub static SCHEDULER_SINGLETON: OnceLock<Arc<ToolApiHandler>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("CompletionError: {0}")]
    Completion(#[from] CompletionError),
    #[error("PromptError: {0}")]
    Prompt(#[from] PromptError),
    #[error("ToolSetError: {0}")]
    Tool(#[from] ToolSetError),
}
pub type StreamingResult = Pin<Box<dyn Stream<Item = Result<Text, StreamingError>> + Send>>;

// TODO: Uncomment when tool scheduler integration is complete
// #[derive(Debug, Error)]
// #[error("Tool call receiver canceled")]
// struct ReceiverCanceledError;
type ToolResultFuture = futures::future::BoxFuture<'static, Result<(String, String), ToolSetError>>;

/// Helper function to stream a completion request to stdout and return the full response
#[allow(dead_code)]
pub(crate) async fn custom_stream_to_stdout(
    stream: &mut StreamingResult,
) -> Result<String, std::io::Error> {
    println!();

    let mut response = String::new();
    while let Some(content) = stream.next().await {
        match content {
            Ok(Text { text }) => {
                print!("{text}");
                std::io::Write::flush(&mut std::io::stdout())?;
                response.push_str(&text);
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

pub async fn multi_turn_prompt<M>(
    agent: Arc<Agent<M>>,
    prompt: impl Into<Message> + Send,
    mut chat_history: Vec<completion::Message>,
) -> StreamingResult
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
                                yield Ok(Text { text: text.text });
                            }
                            Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => {
                                // Stream reasoning text as well
                                yield Ok(Text { text: reasoning.reasoning });
                            }
                            Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                                let (message, future) = match start_tool_call(tool_call.clone()).await {
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

                                yield Ok(Text {
                                    text: format!(
                                        "\nAwaiting tool `{}` …",
                                        tool_call.function.name
                                    ),
                                });

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

/// Initialize the global scheduler singleton
pub fn initialize_scheduler() -> Arc<ToolApiHandler> {
    SCHEDULER_SINGLETON
        .get_or_init(|| {
            let (handler, mut scheduler) = ToolScheduler::new();

            // Register all the tools
            scheduler.register_tool(crate::AbiEncoderTool::new());
            scheduler.register_tool(crate::WalletTransactionTool::new());
            scheduler.register_tool(crate::TimeTool::new());

            // Clone the handler before moving scheduler
            let handler_arc = Arc::new(handler);

            // Start the scheduler - it will run in its own tokio task
            scheduler.run();

            handler_arc
        })
        .clone()
}

async fn start_tool_call(
    tool_call: rig::message::ToolCall,
) -> Result<(AssistantContent, ToolResultFuture), ToolSetError> {
    // Get or initialize the scheduler
    let handler = initialize_scheduler();

    // Extract the tool name and arguments
    let function_name = tool_call.function.name.clone();
    let function_args = tool_call.function.arguments.to_string();

    // Parse the arguments as JSON
    let args_json: serde_json::Value = serde_json::from_str(&function_args).unwrap_or_else(|_| {
        // If parsing fails, try to wrap the string as a simple JSON value
        serde_json::json!({ "input": function_args })
    });

    // Make the async request to the scheduler
    let receiver = handler
        .request_with_json(function_name.clone(), args_json)
        .await;

    // Extract the tool call ID for the response
    let tool_id = tool_call.id.clone();

    // Create a future that will resolve when the tool completes
    let future = async move {
        // Wait for the scheduler to process the tool call
        match receiver.await {
            Ok(Ok(json_response)) => {
                // Convert the JSON response back to a string
                let output = serde_json::to_string_pretty(&json_response)
                    .unwrap_or_else(|_| "Tool execution successful".to_string());
                Ok((tool_id, output))
            }
            Ok(Err(err)) => {
                // Tool execution failed
                Err(ToolSetError::from(ToolError::ToolCallError(
                    format!("Tool execution failed: {}", err).into(),
                )))
            }
            Err(_) => {
                // Channel was closed
                Err(ToolSetError::from(ToolError::ToolCallError(
                    "Tool scheduler channel closed unexpectedly".into(),
                )))
            }
        }
    }
    .boxed();

    // Return the AssistantContent with the tool call and the future
    Ok((AssistantContent::ToolCall(tool_call), future))
}
