use crate::CoreCommand;
use crate::events::SystemEvent;
use crate::state::CoreState;
use aomi_tools::{CallMetadata, ToolCallCtx, ToolReturn};
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

pub type CoreCommandStream =
    Pin<Box<dyn Stream<Item = Result<CoreCommand, StreamingError>> + Send>>;

pub struct CompletionRunner<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    agent: Arc<Agent<M>>,
    state: CoreState,
}

struct StreamState<R> {
    llm_stream:
        BoxStream<'static, Result<StreamedAssistantContent<R>, rig::completion::CompletionError>>,
    llm_finished: bool,
    cached_tool_calls: Vec<rig::message::ToolCall>,
    cached_tool_returns: Vec<ToolReturn>,
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
    pub fn new(agent: Arc<Agent<M>>, state: CoreState) -> Self {
        Self { agent, state }
    }

    async fn init_stream_state(
        &mut self,
        prompt: Message,
    ) -> Result<StreamState<<M as CompletionModel>::StreamingResponse>, StreamingError> {
        let llm_stream = self
            .agent
            .stream_completion(prompt.clone(), self.state.history.clone())
            .await?
            .stream()
            .await?
            .fuse()
            .boxed();

        self.state.history.push(prompt);

        Ok(StreamState {
            llm_stream,
            llm_finished: false,
            cached_tool_calls: Vec::new(),
            cached_tool_returns: Vec::new(),
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
                let mut cmds = self
                    .consume_system_events(&tool_call)
                    .map(|c| vec![c])
                    .unwrap_or_default();
                let topic = match tool_call.function.arguments.get("topic") {
                    Some(Value::String(topic)) => topic.clone(),
                    _ => tool_call.function.name.clone(),
                };
                let tool_return = self.process_tool_call(tool_call.clone()).await?;
                state.cached_tool_calls.push(tool_call);
                state.cached_tool_returns.push(tool_return.clone());

                cmds.push(CoreCommand::ToolCall {
                    topic,
                    stream: tool_return,
                });
                Ok(ProcessStep::Emit(cmds))
            }
            Some(Ok(StreamedAssistantContent::Final(_))) => Ok(ProcessStep::Continue),
            Some(Err(e)) => Err(e.into()),
            None => {
                state.llm_finished = true;
                Ok(ProcessStep::Finished)
            }
        }
    }

    pub async fn stream(self, prompt: Message) -> CoreCommandStream {
        let mut runner = self;
        runner.state.ingest_events();

        let mut current_prompt = prompt;

        let chat_command_stream = async_stream::stream! {
            // Outer loop: restart LLM when tools are called
            'outer: loop {
                let mut streamer = match runner.init_stream_state(current_prompt.clone()).await {
                    Ok(state) => state,
                    Err(err) => {
                        yield Err(err);
                        return;
                    }
                };

                // Inner loop: process LLM stream items
                loop {
                    match runner.consume_stream_item(&mut streamer).await {
                        Ok(ProcessStep::Emit(commands)) => {
                            for command in commands {
                                yield Ok(command);
                            }
                        }
                        Ok(ProcessStep::Continue) => {}
                        Ok(ProcessStep::Finished) => break, // Break inner loop
                        Err(err) => {
                            yield Err(err);
                            return;
                        }
                    }

                    if streamer.llm_finished {
                        break; // Break inner loop
                    }
                }

                // After LLM stream finishes, check if tools were called
                if streamer.cached_tool_returns.is_empty() {
                    // No tools called, we're done
                    break 'outer;
                }

                if !streamer.cached_tool_calls.is_empty() {
                    let tool_calls: Vec<_> = streamer
                        .cached_tool_calls
                        .into_iter()
                        .map(AssistantContent::ToolCall)
                        .collect();
                    runner.state.history.push(Message::Assistant {
                        id: None,
                        content: OneOrMany::many(tool_calls).expect("tool calls cannot be empty"),
                    });
                }

                // Tools were called - push results to history and restart
                runner.state.push_tool_results(streamer.cached_tool_returns);

                // Set prompt to the last tool result message
                current_prompt = match runner.state.history.pop() {
                    Some(prompt) => prompt,
                    None => unreachable!("history cannot be empty after tool results"),
                };
            }
        };

        chat_command_stream.boxed()
    }

    fn consume_system_events(&mut self, tool_call: &rig::message::ToolCall) -> Option<CoreCommand> {
        self.state.system_events.as_ref()?;
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
    ) -> Result<ToolReturn, StreamingError> {
        let rig::message::ToolFunction { name, arguments } = tool_call.function.clone();
        let aomi_namespace = self.state.tool_namespaces.get(&name).cloned();

        if let Some(namespace) = aomi_namespace.clone() {
            let metadata = CallMetadata::new(
                name.clone(),
                namespace,
                tool_call.id.clone(),
                tool_call.call_id.clone(),
                false,
            );
            let ctx: ToolCallCtx = ToolCallCtx {
                session_id: self.state.session_id.clone(),
                metadata: metadata.clone(),
            };
            let envelope = json!({
                "ctx": ctx,
                "args": arguments,
            });
            let envelope_args =
                serde_json::to_string(&envelope).unwrap_or_else(|_| envelope.to_string());

            let result = match self.agent.tools.call(&name, envelope_args).await {
                Ok(result) => result,
                Err(e) => e.to_string(),
            };
            let value = serde_json::from_str(&result).unwrap_or(Value::String(result));

            return Ok(ToolReturn {
                metadata,
                inner: value,
                is_sync_ack: true,
            });
        }

        // All tools now go through Rig's unified agent.tools interface (V2 + MCP)
        let args = serde_json::to_string(&arguments).unwrap_or_else(|_| arguments.to_string());
        let result = self.agent.tools.call(&name, args).await?;
        let value = serde_json::from_str(&result).unwrap_or(Value::String(result));
        let metadata = CallMetadata::new(
            name.clone(),
            "external".to_string(),
            tool_call.id.clone(),
            tool_call.call_id.clone(),
            false,
        );
        Ok(ToolReturn {
            metadata,
            inner: value,
            is_sync_ack: true,
        })
    }
}

pub async fn stream_completion<M>(
    agent: Arc<Agent<M>>,
    prompt: impl Into<Message> + Send,
    state: CoreState,
) -> CoreCommandStream
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: std::marker::Send,
{
    CompletionRunner::new(agent, state)
        .stream(prompt.into())
        .await
}
