use crate::{SystemEvent, SystemEventQueue, app::CoreState};
use aomi_tools::{CallMetadata, ToolStream};
use chrono::Utc;
use futures::{Stream, StreamExt, stream::BoxStream};
use rig::{
    agent::Agent,
    completion::CompletionModel,
    message::Message,
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
pub type CoreCommandStream =
    Pin<Box<dyn Stream<Item = Result<CoreCommand, StreamingError>> + Send>>;

pub struct CompletionRunner<M>
where
    M: CompletionModel + 'static,
    <M as CompletionModel>::StreamingResponse: Send,
{
    agent: Arc<Agent<M>>,
    prompt: Message,
    state: CoreState,
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
    ) -> Self {
        Self {
            agent,
            prompt,
            state,
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
        let topic = match tool_call.function.arguments.get("topic") {
            Some(Value::String(topic)) => topic.clone(),
            _ => tool_call.function.name.clone(),
        };

        let ui_stream = self.process_tool_call(tool_call).await?;

        commands.push(CoreCommand::ToolCall {
            topic,
            stream: ui_stream,
        });

        Ok(ProcessStep::Emit(commands))
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
                    if let Some((call_id, _tool_name, result_text)) = tool_update_from_value(value)
                    {
                        let update_key = format!("{}:{}", call_id.key(), result_text);
                        if !seen_updates.insert(update_key) {
                            continue;
                        }
                        self.state.push_sync_update(call_id, result_text);
                    }
                }
                SystemEvent::AsyncUpdate(value) => {
                    if let Some((call_id, tool_name, result_text)) = tool_update_from_value(value) {
                        let update_key = format!("{}:{}", call_id.key(), result_text);
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

    fn consume_system_events(&mut self, tool_call: &rig::message::ToolCall) -> Option<CoreCommand> {
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
    ) -> Result<ToolStream, StreamingError> {
        let rig::message::ToolFunction { name, mut arguments } = tool_call.function.clone();

        // Add assistant message to chat history, required by the model API pattern
        self.state.push_tool_call(&tool_call);

        if let Value::Object(ref mut obj) = arguments {
            obj.insert(
                "session_id".to_string(),
                Value::String(self.state.session_id.clone()),
            );
        }

        // All tools now go through Rig's unified agent.tools interface (V2 + MCP)
        let args = serde_json::to_string(&arguments).unwrap_or_else(|_| arguments.to_string());
        let result = self.agent.tools.call(&name, args).await?;
        let value = serde_json::from_str(&result).unwrap_or(Value::String(result));
        let metadata = CallMetadata::new(
            name.clone(),
            tool_call.id.clone(),
            tool_call.call_id.clone(),
            false,
        );
        Ok(ToolStream::from_result(metadata, Ok(value)))
    }
}

fn tool_update_from_value(value: &Value) -> Option<(CallMetadata, String, String)> {
    let id = value
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| value.get("call_id").and_then(|value| value.as_str()))?
        .to_string();
    let call_id = value
        .get("call_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let tool_name = value.get("tool_name")?.as_str()?.to_string();
    let result = value.get("result")?.clone();
    let result_text = if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        format!("tool_error: {}", error)
    } else {
        serde_json::to_string_pretty(&result)
            .unwrap_or_else(|err| format!("tool_error: failed to serialize tool result: {}", err))
    };
    Some((
        CallMetadata::new(tool_name.clone(), id, call_id, false),
        tool_name,
        result_text,
    ))
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
    CompletionRunner::new(agent, prompt.into(), state)
        .stream()
        .await
}

#[cfg(test)]
mod tests {
    use super::tool_update_from_value;
    use serde_json::json;

    #[test]
    fn tool_update_from_value_parses_completion_payload() {
        let payload = json!({
            "id": "req_1",
            "call_id": "call_99",
            "tool_name": "sample_tool",
            "result": { "ok": true }
        });

        let (meta, tool_name, text) =
            tool_update_from_value(&payload).expect("parsed completion payload");
        assert_eq!(tool_name, "sample_tool");
        assert_eq!(meta.name, "sample_tool");
        assert_eq!(meta.id, "req_1");
        assert_eq!(meta.call_id.as_deref(), Some("call_99"));
        assert!(text.contains("\"ok\": true"));
    }

    #[test]
    fn tool_update_from_value_reports_error_payloads() {
        let payload = json!({
            "id": "req_2",
            "tool_name": "error_tool",
            "result": { "error": "boom" }
        });

        let (_meta, _tool_name, text) =
            tool_update_from_value(&payload).expect("parsed error payload");
        assert!(text.contains("tool_error: boom"));
    }

    #[test]
    fn tool_update_from_value_returns_none_on_missing_fields() {
        let payload = json!({
            "id": "req_3",
            "result": { "ok": true }
        });

        assert!(tool_update_from_value(&payload).is_none());
    }
}
