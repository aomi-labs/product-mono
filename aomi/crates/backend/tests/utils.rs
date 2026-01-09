#![allow(dead_code)]

//! Shared test utilities for backend integration tests.
//!
//! This module provides reusable mock backends and helpers for testing
//! session state, streaming, and tool execution flows.
//!
//! # Mock Backends
//!
//! | Backend | Use Case |
//! |---------|----------|
//! | [`MockBackend`] | Scripted interactions with expected input/output |
//! | [`StreamingToolBackend`] | Single-shot tool with streaming result |
//! | [`MultiStepToolBackend`] | Multi-step tool that emits `AsyncUpdate` |
//!
//! # Helpers
//!
//! | Helper | Purpose |
//! |--------|---------|
//! | [`flush_state`] | Pump session state until processing completes |
//! | [`test_message`] | Create a `ChatMessage` for assertions |
//! | [`history_snapshot`] | Create a `UserHistory` snapshot |

use anyhow::Result;
use aomi_backend::session::{AomiBackend, ChatMessage, DefaultSessionState, MessageSender};
use aomi_chat::{CoreCommand, Message, SystemEvent, SystemEventQueue, ToolStream};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::Arc, time::Instant};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::yield_now,
};

// ============================================================================
// Data Types
// ============================================================================

/// Snapshot of user history for test assertions.
#[derive(Clone)]
pub struct UserHistory {
    pub messages: Vec<ChatMessage>,
    pub last_activity: Instant,
}

impl UserHistory {
    pub fn new(messages: Vec<ChatMessage>, last_activity: Instant) -> Self {
        Self {
            messages,
            last_activity,
        }
    }
}

/// A scripted interaction for [`MockBackend`].
#[derive(Clone)]
pub struct MockInteraction {
    pub expected_input: String,
    pub streaming_chunks: Vec<String>,
    pub tool_calls: Vec<(String, String)>,
    pub final_reply: String,
}

impl MockInteraction {
    /// Create an interaction that only streams text (no tool calls).
    pub fn streaming_only(input: &str, reply: &str) -> Self {
        Self {
            expected_input: input.to_string(),
            streaming_chunks: vec![reply.to_string()],
            tool_calls: Vec::new(),
            final_reply: reply.to_string(),
        }
    }

    /// Create an interaction with a single tool call.
    pub fn with_tool_call(input: &str, reply: &str, tool_name: &str, tool_args: &str) -> Self {
        Self {
            expected_input: input.to_string(),
            streaming_chunks: vec![reply.to_string()],
            tool_calls: vec![(tool_name.to_string(), tool_args.to_string())],
            final_reply: reply.to_string(),
        }
    }
}

// ============================================================================
// Mock Backends
// ============================================================================

/// A mock backend that plays back scripted interactions.
///
/// Use this when you need to test specific input/output sequences
/// or verify history length tracking.
///
/// # Example
/// ```ignore
/// let backend = MockBackend::new(vec![
///     MockInteraction::streaming_only("hello", "Hi there!"),
/// ]);
/// ```
#[derive(Clone)]
pub struct MockBackend {
    interactions: Arc<Mutex<VecDeque<MockInteraction>>>,
    history_lengths: Arc<Mutex<Vec<usize>>>,
}

impl MockBackend {
    pub fn new(interactions: Vec<MockInteraction>) -> Self {
        Self {
            interactions: Arc::new(Mutex::new(interactions.into())),
            history_lengths: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn history_lengths(&self) -> Vec<usize> {
        self.history_lengths.lock().await.clone()
    }
}

#[async_trait]
impl AomiBackend for MockBackend {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        input: String,
        history: Arc<RwLock<Vec<Message>>>,
        _system_events: SystemEventQueue,
        _handler: Arc<Mutex<aomi_tools::scheduler::ToolHandler>>,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        while interrupt_receiver.try_recv().is_ok() {}

        let interaction = {
            let mut queued = self.interactions.lock().await;
            queued
                .pop_front()
                .expect("no scripted interaction remaining")
        };

        assert_eq!(
            interaction.expected_input, input,
            "unexpected user input routed to agent"
        );

        let snapshot_len = history.read().await.len();
        self.history_lengths.lock().await.push(snapshot_len);

        for chunk in interaction.streaming_chunks.iter() {
            command_sender
                .send(CoreCommand::StreamingText(chunk.clone()))
                .await
                .expect("streaming chunk send");
        }

        for (name, args) in interaction.tool_calls.iter() {
            let topic = format!("{}: {}", name, args);
            let stream = ToolStream::empty();
            command_sender
                .send(CoreCommand::ToolCall { topic, stream })
                .await
                .expect("tool call send");
        }

        command_sender
            .send(CoreCommand::Complete)
            .await
            .expect("complete send");

        {
            let mut history_guard = history.write().await;
            history_guard.push(Message::user(input));
            if !interaction.final_reply.is_empty() {
                history_guard.push(Message::assistant(interaction.final_reply));
            }
        }

        Ok(())
    }
}

/// A mock backend that emits a single streaming tool call.
///
/// Emits: `StreamingText` -> `ToolCall` (with result) -> `Complete`
///
/// Use this to test single-shot tool result accumulation in session state.
#[derive(Clone)]
pub struct StreamingToolBackend;

#[async_trait]
impl AomiBackend for StreamingToolBackend {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        _input: String,
        _history: Arc<RwLock<Vec<Message>>>,
        _system_events: SystemEventQueue,
        _handler: Arc<Mutex<aomi_tools::scheduler::ToolHandler>>,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        command_sender
            .send(CoreCommand::StreamingText("Thinking...".to_string()))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send text: {}", e))?;

        command_sender
            .send(CoreCommand::ToolCall {
                topic: "streaming_tool".to_string(),
                stream: ToolStream::from_result(
                    "test_id".to_string(),
                    Ok(json!("first chunk second chunk")),
                    "streaming_tool".to_string(),
                ),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send tool call: {}", e))?;

        command_sender
            .send(CoreCommand::Complete)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send complete: {}", e))?;

        Ok(())
    }
}

/// A mock backend that emits a multi-step tool with `AsyncUpdate`.
///
/// Emits: `StreamingText` -> `ToolCall` (ACK) -> `AsyncUpdate` -> `Complete`
///
/// Use this to test:
/// - async updates surfacing via `SystemEventQueue`
///
/// # Configuration
/// - `tool_name`: Name of the tool (default: "multi_step_tool")
/// - `call_id`: Tool call ID (default: "multi_step_call_1")
/// - `result`: Final result value (default: `{"status": "completed", "data": [...]}`)
#[derive(Clone)]
pub struct MultiStepToolBackend {
    pub tool_name: String,
    pub call_id: String,
    pub result: Value,
    pub emit_error: bool,
}

impl Default for MultiStepToolBackend {
    fn default() -> Self {
        Self {
            tool_name: "multi_step_tool".to_string(),
            call_id: "multi_step_call_1".to_string(),
            result: json!({
                "status": "completed",
                "data": ["step1", "step2", "step3"]
            }),
            emit_error: false,
        }
    }
}

impl MultiStepToolBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tool_name(mut self, name: &str) -> Self {
        self.tool_name = name.to_string();
        self
    }

    pub fn with_call_id(mut self, id: &str) -> Self {
        self.call_id = id.to_string();
        self
    }

    pub fn with_result(mut self, result: Value) -> Self {
        self.result = result;
        self
    }

    pub fn with_error(mut self) -> Self {
        self.emit_error = true;
        self
    }
}

#[async_trait]
impl AomiBackend for MultiStepToolBackend {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        _input: String,
        _history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        _handler: Arc<Mutex<aomi_tools::scheduler::ToolHandler>>,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        // 1. Initial streaming text
        command_sender
            .send(CoreCommand::StreamingText(
                "Starting multi-step operation...".to_string(),
            ))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send text: {}", e))?;

        // 2. Tool call ACK (first chunk for UI)
        command_sender
            .send(CoreCommand::ToolCall {
                topic: self.tool_name.clone(),
                stream: ToolStream::from_result(
                    self.call_id.clone(),
                    Ok(json!({"step": 1, "status": "started"})),
                    self.tool_name.clone(),
                ),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send tool call: {}", e))?;

        // 3. Async tool result (final result after background processing)
        system_events.push(SystemEvent::AsyncUpdate(json!({
            "type": "tool_async_result",
            "tool_name": self.tool_name.clone(),
            "call_id": self.call_id.clone(),
            "result": if self.emit_error {
                json!({"error": "multi-step failed"})
            } else {
                self.result.clone()
            },
        })));

        // 4. Complete
        command_sender
            .send(CoreCommand::Complete)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send complete: {}", e))?;

        Ok(())
    }
}

/// A mock backend that immediately sends `Interrupted`.
#[derive(Clone)]
pub struct InterruptingBackend;

#[async_trait]
impl AomiBackend for InterruptingBackend {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        _input: String,
        _history: Arc<RwLock<Vec<Message>>>,
        _system_events: SystemEventQueue,
        _handler: Arc<Mutex<aomi_tools::scheduler::ToolHandler>>,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        command_sender
            .send(CoreCommand::StreamingText("starting".to_string()))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send text: {}", e))?;

        command_sender
            .send(CoreCommand::Interrupted)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send interrupted: {}", e))?;

        Ok(())
    }
}

/// A mock backend that pushes events to the SystemEventQueue.
///
/// Use this to test SystemEvent propagation through the session.
#[derive(Clone)]
pub struct SystemEventBackend {
    pub events_to_push: Vec<aomi_chat::SystemEvent>,
}

impl SystemEventBackend {
    pub fn new(events: Vec<aomi_chat::SystemEvent>) -> Self {
        Self {
            events_to_push: events,
        }
    }

    pub fn with_tool_display(tool_name: &str, call_id: &str, result: Value) -> Self {
        Self {
            events_to_push: vec![aomi_chat::SystemEvent::InlineDisplay(serde_json::json!({
                "type": "tool_display",
                "tool_name": tool_name,
                "call_id": call_id,
                "result": result,
            }))],
        }
    }
}

#[async_trait]
impl AomiBackend for SystemEventBackend {
    type Command = CoreCommand<ToolStream>;
    async fn process_message(
        &self,
        _input: String,
        _history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        _handler: Arc<Mutex<aomi_tools::scheduler::ToolHandler>>,
        command_sender: &mpsc::Sender<CoreCommand<ToolStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        // Push all configured events to the queue
        for event in &self.events_to_push {
            system_events.push(event.clone());
        }

        command_sender
            .send(CoreCommand::StreamingText("Events pushed".to_string()))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send text: {}", e))?;

        command_sender
            .send(CoreCommand::Complete)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send complete: {}", e))?;

        Ok(())
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a `ChatMessage` for test assertions.
pub fn test_message(sender: MessageSender, content: &str) -> ChatMessage {
    ChatMessage {
        sender,
        content: content.to_string(),
        tool_stream: None,
        timestamp: "00:00:00 UTC".to_string(),
        is_streaming: false,
    }
}

/// Create a `UserHistory` snapshot for test assertions.
pub fn history_snapshot(messages: Vec<ChatMessage>, last_activity: Instant) -> UserHistory {
    UserHistory::new(messages, last_activity)
}

/// Pump `sync_state()` until the session stops processing (max 8 iterations).
///
/// Use this after sending a message to ensure all commands are processed.
pub async fn flush_state(state: &mut DefaultSessionState) {
    for _ in 0..8 {
        yield_now().await;
        state.sync_state().await;
        if !state.is_processing {
            break;
        }
    }
}
