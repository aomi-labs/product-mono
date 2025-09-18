use std::{pin::Pin, sync::Arc};

use futures::{Stream, StreamExt};
pub use rig::message::Text;
use rig::{
    OneOrMany,
    agent::Agent,
    completion::{self, CompletionError, CompletionModel, PromptError},
    message::{AssistantContent, Message, ToolResultContent, UserContent},
    streaming::{StreamedAssistantContent, StreamingCompletion},
    tool::ToolSetError,
};
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
pub type StreamingResult = Pin<Box<dyn Stream<Item = Result<Text, StreamingError>> + Send>>;

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

// Example taken from rig. `prompt` is the only API that has multi_turn support built in afaik.
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
        let mut did_call_tool = false;

        'outer: loop {
            let mut stream = agent
                .stream_completion(current_prompt.clone(), chat_history.clone())
                .await?
                .stream()
                .await?;

            chat_history.push(current_prompt.clone());

            let mut tool_calls = vec![];
            let mut tool_results = vec![];

            while let Some(content) = stream.next().await {
                match content {
                    Ok(StreamedAssistantContent::Text(text)) => {
                        // Stream text directly as it comes in for smooth appearance
                        yield Ok(Text { text: text.text });
                        did_call_tool = false;
                    },
                    Ok(StreamedAssistantContent::ToolCall(tool_call)) => {
                        // Send a special marker for tool calls that the UI can detect
                        let tool_indicator = format!("[[TOOL_CALL:{}:{}]]\n",
                            tool_call.function.name,
                            tool_call.function.arguments
                        );
                        yield Ok(Text { text: tool_indicator });

                        // Execute the tool with error handling                            
                        let tool_result = match agent.tools.call(&tool_call.function.name, tool_call.function.arguments.to_string()).await {
                            Ok(result) => {
                                if tool_call.function.name == "send_transaction_to_wallet" {
                                    // The wallet tool already returns [[WALLET_TX_REQUEST:{...}]]
                                    // Just yield it directly without additional wrapping
                                    yield Ok(Text { text: format!("[[WALLET_TX_REQUEST:{result}]]\n") });
                                } else {
                                    // Send tool result marker for other tools
                                    let result_preview = if result.len() > 200 {
                                        format!("{}...", &result[..200])
                                    } else {
                                        result.clone()
                                    };
                                    yield Ok(Text { text: format!("[[TOOL_RESULT:{result_preview}]]\n") });
                                }
                                result
                            },
                            Err(e) => {
                                // Send error marker
                                let error_msg = format!("Tool execution failed: {e}");
                                yield Ok(Text { text: format!("[[TOOL_ERROR:{error_msg}]]\n") });
                                error_msg
                            }
                        };

                        let tool_call_msg = AssistantContent::ToolCall(tool_call.clone());

                        tool_calls.push(tool_call_msg);
                        tool_results.push((tool_call.id, tool_call.call_id, tool_result));

                        did_call_tool = true;
                    },
                    Ok(StreamedAssistantContent::Reasoning(rig::message::Reasoning { reasoning })) => {
                        // Stream reasoning text as well
                        yield Ok(Text { text: reasoning });
                        did_call_tool = false;
                    },
                    Ok(_) => {
                        // do nothing here as we don't need to accumulate token usage
                    }
                    Err(e) => {
                        yield Err(e.into());
                        break 'outer;
                    }
                }
            }

            // Add (parallel) tool calls to chat history
            if !tool_calls.is_empty() {
                chat_history.push(Message::Assistant {
                    id: None,
                    content: OneOrMany::many(tool_calls).expect("Impossible EmptyListError"),
                });
            }

            // Add tool results to chat history
            for (id, call_id, tool_result) in tool_results {
                if let Some(call_id) = call_id {
                    chat_history.push(Message::User {
                        content: OneOrMany::one(UserContent::tool_result_with_call_id(
                            id,
                            call_id,
                            OneOrMany::one(ToolResultContent::text(tool_result)),
                        )),
                    });
                } else {
                    chat_history.push(Message::User {
                        content: OneOrMany::one(UserContent::tool_result(
                            id,
                            OneOrMany::one(ToolResultContent::text(tool_result)),
                        )),
                    });

                }

            }

            // Set the current prompt to the last message in the chat history
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
