use std::{future::Future, pin::Pin, sync::Arc};

use async_stream::stream;
use futures::{Stream, StreamExt};
use rig::{
    agent::Agent,
    completion::{self as rig_completion, CompletionModel},
    message::Message,
    streaming::{StreamedAssistantContent, StreamingCompletion},
};

use crate::{
    completion::{self as completion_mod, StreamingError},
    tool_scheduler::ToolApiHandler,
    ChatCommand,
};

type StdResult<T, E> = std::result::Result<T, E>;

pub trait IntoRigAgent {
    type Model: CompletionModel + 'static;
    fn into_rig_agent(self) -> Arc<Agent<Self::Model>>;
}

impl<M> IntoRigAgent for Arc<Agent<M>>
where
    M: CompletionModel + 'static,
{
    type Model = M;

    fn into_rig_agent(self) -> Arc<Agent<M>> {
        self
    }
}

/// Common abstraction for applications that orchestrate an inference or agent runtime with
/// tool scheduling and streaming responses.
pub trait AomiApp {
    /// Concrete agent handle used to drive completions.
    type Agent;
    /// Scheduler that coordinates tool execution for the application.
    type Scheduler;
    /// Handler type created from the scheduler for a single streaming session.
    type ToolHandler: Send + 'static;
    /// Prompt or request type fed into the model for each completion.
    type Prompt;
    /// Conversation or state history material passed into the completion call.
    type History: Send + 'static;
    /// Stream items emitted back to the caller while the model responds.
    type Event: Send + 'static;
    /// Error surface for streamed items.
    type StreamError: From<StreamingError> + Send + 'static;
    /// Concrete stream returned by `stream_completion`.
    type Stream: Stream<Item = Result<Self::Event, Self::StreamError>> + Send + 'static;
    /// Optional document store handle shared by the application.
    type DocumentStore;
    /// Optional MCP toolbox handle shared by the application.
    type Toolbox;
    /// Result produced when completing without streaming.
    type CompletionOutput: From<String> + Send + 'static;

    /// Access the agent driving model interactions.
    fn agent(&self) -> Self::Agent;

    /// Access the global scheduler for tool orchestration.
    fn scheduler(&self) -> Self::Scheduler;

    /// Acquire a fresh tool handler for a new streaming session.
    fn tool_handler(&self) -> Self::ToolHandler;

    /// Retrieve the optional document store backing the app, if any.
    fn document_store(&self) -> Option<Self::DocumentStore> {
        None
    }

    /// Retrieve the optional MCP toolbox backing the app, if any.
    fn mcp_toolbox(&self) -> Option<Self::Toolbox> {
        None
    }

    /// Extract textual output from a streamed event. Defaults to ignoring all events.
    fn event_text(event: &Self::Event) -> Option<String> {
        let _ = event;
        None
    }

    /// Begin streaming a model completion using the provided handler, prompt, and history.
    fn stream_completion(
        &self,
        handler: Self::ToolHandler,
        prompt: Self::Prompt,
        history: Self::History,
    ) -> Pin<Box<dyn Future<Output = Self::Stream> + Send>>
    where
        Self: Sized,
        Self::Agent: IntoRigAgent,
        Self::ToolHandler: Into<ToolApiHandler>,
        Self::Prompt: Into<Message>,
        Self::History: Into<Vec<Message>>,
        Self::Stream: From<Pin<Box<dyn Stream<Item = Result<Self::Event, Self::StreamError>> + Send>>>,
        Self::Event: From<ChatCommand>,
    {
        let agent = self.agent().into_rig_agent();
        let mut handler: ToolApiHandler = handler.into();
        let prompt: Message = prompt.into();
        let history_vec: Vec<Message> = history.into();
        let mut chat_history: Vec<rig_completion::Message> =
            history_vec.into_iter().map(Into::into).collect();

        Box::pin(async move {
            let raw_stream = stream! {
                let mut current_prompt = prompt;

                'outer: loop {
                    debug_assert!(!handler.has_pending_results());

                    let completion_stream = match agent
                        .stream_completion(current_prompt.clone(), chat_history.clone())
                        .await
                    {
                        Ok(stream) => stream,
                        Err(err) => {
                            let err = StreamingError::from(err);
                            yield Err(err);
                            break;
                        }
                    };

                    let mut stream = match completion_stream.stream().await {
                        Ok(inner) => inner.fuse(),
                        Err(err) => {
                            let err = StreamingError::from(err);
                            yield Err(err);
                            break;
                        }
                    };

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
                                    Some(Ok(())) => {}
                                    Some(Err(err)) => {
                                        let err = StreamingError::from(err);
                                        yield Err(err);
                                        break 'outer;
                                    }
                                    None => {}
                                }
                            },
                            maybe_content = stream.next(), if !stream_finished => {
                                match maybe_content {
                                    Some(Ok(StreamedAssistantContent::Text(text))) => {
                                        let event = ChatCommand::StreamingText(text.text);
                                        yield Ok(Self::Event::from(event));
                                    }
                                    Some(Ok(StreamedAssistantContent::Reasoning(reasoning))) => {
                                        let event = ChatCommand::StreamingText(reasoning.reasoning);
                                        yield Ok(Self::Event::from(event));
                                    }
                                    Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                                        if let Some(cmd) = completion_mod::handle_wallet_transaction(&tool_call) {
                                            yield Ok(Self::Event::from(cmd));
                                        }

                                        if let Err(err) = completion_mod::process_tool_call(
                                            agent.clone(),
                                            tool_call.clone(),
                                            &mut chat_history,
                                            &mut handler
                                        ).await {
                                            yield Err(err);
                                            break 'outer;
                                        }

                                        let cmd = ChatCommand::ToolCall {
                                            name: tool_call.function.name.clone(),
                                            args: format!("Awaiting tool `{}` …", tool_call.function.name),
                                        };
                                        yield Ok(Self::Event::from(cmd));

                                        did_call_tool = true;
                                    }
                                    Some(Ok(StreamedAssistantContent::Final(_))) => {}
                                    Some(Err(err)) => {
                                        yield Err(StreamingError::from(err));
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
                    completion_mod::finalize_tool_results(tool_results, &mut chat_history);

                    current_prompt = chat_history
                        .pop()
                        .expect("Chat history should never be empty at this point");

                    if !did_call_tool {
                        break;
                    }
                }
            };

            let mapped = raw_stream.map(|result| result.map_err(Into::into));
            Self::Stream::from(Box::pin(mapped))
        })
    }

    fn completion(
        &self,
        handler: Self::ToolHandler,
        prompt: Self::Prompt,
        history: Self::History,
    ) -> Pin<Box<dyn Future<Output = StdResult<Self::CompletionOutput, Self::StreamError>> + Send>>
    where
        Self: Sized,
        Self::Agent: IntoRigAgent,
        Self::ToolHandler: Into<ToolApiHandler>,
        Self::Prompt: Into<Message>,
        Self::History: Into<Vec<Message>>,
    {
        let agent = self.agent().into_rig_agent();
        let mut handler: ToolApiHandler = handler.into();
        let prompt: Message = prompt.into();
        let history_vec: Vec<Message> = history.into();
        let mut chat_history: Vec<rig_completion::Message> =
            history_vec.into_iter().map(Into::into).collect();

        Box::pin(async move {
            let mut current_prompt = prompt;
            let mut accumulator = String::new();

            loop {
                debug_assert!(!handler.has_pending_results());

                let completion_stream = agent
                    .stream_completion(current_prompt.clone(), chat_history.clone())
                    .await
                    .map_err(|err| Self::StreamError::from(StreamingError::from(err)))?;

                let mut stream = completion_stream
                    .stream()
                    .await
                    .map_err(|err| Self::StreamError::from(StreamingError::from(err)))?
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
                                Some(Ok(())) => {}
                                Some(Err(err)) => {
                                    return Err(Self::StreamError::from(StreamingError::from(err)));
                                }
                                None => {}
                            }
                        },
                        maybe_content = stream.next(), if !stream_finished => {
                            match maybe_content {
                                Some(Ok(StreamedAssistantContent::Text(text))) => {
                                    accumulator.push_str(&text.text);
                                }
                                Some(Ok(StreamedAssistantContent::Reasoning(_))) => {}
                                Some(Ok(StreamedAssistantContent::ToolCall(tool_call))) => {
                                    if let Err(err) = completion_mod::process_tool_call(
                                        agent.clone(),
                                        tool_call.clone(),
                                        &mut chat_history,
                                        &mut handler
                                    ).await {
                                        return Err(Self::StreamError::from(err));
                                    }

                                    did_call_tool = true;
                                }
                                Some(Ok(StreamedAssistantContent::Final(_))) => {}
                                Some(Err(err)) => {
                                    return Err(Self::StreamError::from(StreamingError::from(err)));
                                }
                                None => {
                                    stream_finished = true;
                                }
                            }
                        }
                    }
                }

                let tool_results = handler.take_finished_results();
                completion_mod::finalize_tool_results(tool_results, &mut chat_history);

                current_prompt = chat_history
                    .pop()
                    .expect("Chat history should never be empty at this point");

                if !did_call_tool {
                    break;
                }
            }

            Ok(accumulator.into())
        }) as Pin<Box<dyn Future<Output = StdResult<Self::CompletionOutput, Self::StreamError>> + Send>>
    }
}
