# AomiTool Architecture Refactor

**Status**: Design Phase
**Author**: Cecilia
**Date**: 2025-01-14
**Goal**: Simplify tool execution by moving scheduler logic into tools, making all tool calls flow through Rig's unified path.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current Architecture](#current-architecture)
3. [Proposed Architecture](#proposed-architecture)
4. [Core Design](#core-design)
5. [Migration Phases](#migration-phases)
6. [Testing Strategy](#testing-strategy)
7. [Rollback Plan](#rollback-plan)
8. [Success Metrics](#success-metrics)

---

## Executive Summary

### Problem Statement

Current architecture has split tool execution paths:
- **Path A**: Scheduler-aware tools → `handler.request()` → scheduler event loop → execution
- **Path B**: MCP/legacy tools → `agent.tools.call()` → direct execution

This creates:
- ❌ Complexity in `completion.rs` (two code paths)
- ❌ Tight coupling between completion layer and scheduler
- ❌ Difficult to reason about async tool lifecycle
- ❌ No session awareness in tools
- ❌ Testing requires scheduler setup

### Solution

Unify all tool calls through Rig, moving scheduler logic into the tools themselves:
- ✅ Single code path in `completion.rs`
- ✅ Tools become self-scheduling (sync or async)
- ✅ Session-aware tool execution
- ✅ Scheduler becomes a service (not executor)
- ✅ Simpler testing (tools are standalone)

### Impact

| Component | Before | After |
|-----------|--------|-------|
| `completion.rs` | 315 lines, scheduler coupling | ~200 lines, Rig-only |
| `scheduler.rs` | Event loop executor | Service/registry |
| Tool implementation | `impl rig::Tool` | `impl AomiTool` (auto-impl Rig) |
| Session awareness | None | Built-in |
| Testing | Complex (scheduler setup) | Simple (standalone tools) |

---

## Current Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      CURRENT FLOW                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  LLM → ToolCall                                                  │
│       │                                                          │
│       ▼                                                          │
│  completion.rs:consume_tool_call()                               │
│       │                                                          │
│       ├──────────────┬─────────────────────────────────────┐    │
│       │              │                                      │    │
│       ▼              ▼                                      ▼    │
│   [Scheduler?]   [YES]                               [NO]       │
│       │              │                                      │    │
│       │         process_tool_call()          process_tool_call_fallback() │
│       │              │                                      │    │
│       │              ▼                                      ▼    │
│       │    handler.request()                    agent.tools.call() │
│       │              │                                      │    │
│       │              ▼                                      │    │
│       │    Scheduler Event Loop                             │    │
│       │         (run loop)                                  │    │
│       │              │                                      │    │
│       │              ▼                                      │    │
│       │    tool.call_with_json()                           │    │
│       │              │                                      │    │
│       │              └──────────────┬─────────────────────┘     │
│       │                             │                            │
│       └─────────────────────────────┼────────────────────────┐  │
│                                     ▼                        ▼  │
│                              ToolStream            ToolStream   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Problems Visualized

```
┌────────────────────────────────────────────────────────────┐
│  COMPLETION.RS COMPLEXITY                                   │
├────────────────────────────────────────────────────────────┤
│                                                             │
│  process_tool_call():                                       │
│    • Check if tool_name in scheduler.list_tool_names()     │
│    • Create CallMetadata                                     │
│    • Lock handler mutex                                    │
│    • Call handler.request()                                │
│    • Resolve to stream                                     │
│    • Return stream                                         │
│                                                             │
│  process_tool_call_fallback():                             │
│    • Different code path                                   │
│    • Direct Rig call                                       │
│    • Manually create ToolStream                            │
│    • Different error handling                              │
│                                                             │
│  ❌ Two paths = double complexity                          │
│  ❌ Scheduler awareness in completion layer                │
│  ❌ Hard to test in isolation                              │
│                                                             │
└────────────────────────────────────────────────────────────┘
```

### Current Scheduler Role

```
┌─────────────────────────────────────────────────────────────┐
│  SCHEDULER (Static Singleton)                                │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  tools: HashMap<String, Arc<dyn AnyTool>>                   │
│  requests_tx: mpsc::Sender                                  │
│  runtime: Arc<SchedulerRuntime>                             │
│                                                              │
│  ┌────────────────────────────────┐                         │
│  │  Event Loop (background task)   │                         │
│  │                                 │                         │
│  │  loop {                         │                         │
│  │    rx.recv() → request          │                         │
│  │    lookup tool                  │                         │
│  │    validate                     │                         │
│  │    execute                      │                         │
│  │    send result                  │                         │
│  │  }                              │                         │
│  └────────────────────────────────┘                         │
│                                                              │
│  ❌ Problem: Single global executor                         │
│  ❌ Problem: No session awareness                           │
│  ❌ Problem: Incomplete calls lost on restart               │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Proposed Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      PROPOSED FLOW                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  LLM → ToolCall                                                  │
│       │                                                          │
│       ▼                                                          │
│  completion.rs:consume_tool_call()                               │
│       │                                                          │
│       ▼                                                          │
│  agent.tools.call(session_id, args)                              │
│       │                                                          │
│       ▼                                                          │
│  rig::Tool::call() [auto-impl for AomiTool]                      │
│       │                                                          │
│       ├─────────────────┬────────────────────────┐              │
│       │                 │                        │              │
│       ▼                 ▼                        ▼              │
│  [support_async?]   [YES]                    [NO]               │
│       │                 │                        │              │
│       │            run_async()              run_sync()          │
│       │                 │                        │              │
│       │                 ▼                        ▼              │
│       │       spawn + mpsc::channel    spawn + oneshot::channel │
│       │                 │                        │              │
│       │                 ▼                        ▼              │
│       │         ToolReciever::Multi      ToolReciever::Single   │
│       │                 │                        │              │
│       │                 └───────┬────────────────┘              │
│       │                         │                               │
│       │                         ▼                               │
│       │         handler.register_receiver(receiver)             │
│       │                         │                               │
│       └─────────────────────────┴───────────────┐              │
│                                                  │              │
│                                                  ▼              │
│                                         Return "queued" JSON    │
│                                                                  │
│  ✅ Single unified path                                         │
│  ✅ Tools self-schedule                                         │
│  ✅ Session-aware                                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### New Scheduler Role

```
┌─────────────────────────────────────────────────────────────────┐
│  SCHEDULER (Service/Registry)                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  tools: HashMap<String, ToolMetadata>                           │
│        (namespace, description, is_async, etc.)                 │
│                                                                  │
│  session_handlers: HashMap<SessionId, Arc<Mutex<ToolHandler>>>  │
│                                                                  │
│  ┌───────────────────────────────────────────┐                  │
│  │  get_session_handler(session_id)          │                  │
│  │    → Arc<Mutex<ToolHandler>>              │                  │
│  │                                           │                  │
│  │  ToolHandler per session:                │                  │
│  │    • unresolved_calls: Vec<ToolReciever> │                  │
│  │    • ongoing_streams: Vec<ToolStream>    │                  │
│  │    • completed_calls: Vec<ToolCompletion>│                  │
│  └───────────────────────────────────────────┘                  │
│                                                                  │
│  cleanup_session(session_id):                                   │
│    • Persist incomplete calls                                   │
│    • Remove from memory                                         │
│    • Can restore later                                          │
│                                                                  │
│  ✅ No event loop                                               │
│  ✅ Session isolation                                           │
│  ✅ Persistence-ready                                           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Session-Aware Tools

```
┌─────────────────────────────────────────────────────────────────┐
│  TOOL EXECUTION WITH SESSION CONTEXT                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Alice's session (session_id = "alice_123"):                    │
│                                                                  │
│    forge_execute({plan: [...], topic: "Deploy USDC"})           │
│         │                                                        │
│         ▼                                                        │
│    rig::Tool::call(session_id="alice_123", args)                │
│         │                                                        │
│         ▼                                                        │
│    scheduler.get_session_handler("alice_123")                   │
│         │                                                        │
│         ▼                                                        │
│    handler.unresolved_calls.push(ToolReciever { ... })          │
│                                                                  │
│  Alice logs off → cleanup_session("alice_123")                  │
│    • Serialize incomplete calls to DB                           │
│    • Remove from memory                                         │
│                                                                  │
│  Alice logs back in → restore_session("alice_123")              │
│    • Load incomplete calls from DB                              │
│    • Recreate ToolHandler                                       │
│    • Resume async execution                                     │
│                                                                  │
│  ✅ Per-session isolation                                       │
│  ✅ Persistence across sessions                                 │
│  ✅ Multi-user safety                                           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Core Design

### AomiTool Trait

```
┌─────────────────────────────────────────────────────────────┐
│  trait AomiTool                                              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Associated Types:                                           │
│    • NAME: &'static str                                      │
│    • NAMESPACE: &'static str                                 │
│    • Args: Deserialize + Serialize                           │
│    • Output: Serialize                                       │
│    • Error: std::error::Error                                │
│                                                              │
│  Methods:                                                    │
│    • support_async() -> bool                                 │
│    • description() -> &'static str                           │
│    • parameters_schema() -> Value                            │
│                                                              │
│    • run_sync(oneshot::Sender, args) -> Future               │
│      ↳ For tools that return one result                      │
│                                                              │
│    • run_async(mpsc::Sender, args) -> Future                 │
│      ↳ For tools that stream multiple results                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Core Types (Pseudocode)

```rust
// ============================================================================
// 1. AomiTool Trait - Core abstraction for sync/async tools
// ============================================================================

pub trait AomiTool: Send + Sync + Clone + 'static {
    /// Tool's unique name
    const NAME: &'static str;

    /// Tool's namespace for organization
    const NAMESPACE: &'static str = "default";

    /// Request type
    type Args: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static;

    /// Response type
    type Output: Serialize + Send + Sync + 'static;

    /// Error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Whether this tool supports async/streaming results
    fn support_async(&self) -> bool {
        false
    }

    /// Get tool description for LLM
    fn description(&self) -> &'static str;

    /// Get JSON schema for arguments
    fn parameters_schema(&self) -> Value;

    /// Execute synchronously - sends one result via oneshot channel
    fn run_sync(
        &self,
        result_sender: oneshot::Sender<Result<Value>>,
        args: Self::Args,
    ) -> impl Future<Output = ()> + Send {
        async move {
            let _ = result_sender.send(Err(eyre::eyre!(
                "Tool {} does not support sync execution",
                Self::NAME
            )));
        }
    }

    /// Execute asynchronously - streams multiple results via mpsc channel
    /// Tool owns the sender and can send multiple values before dropping
    fn run_async(
        &self,
        _results_sender: mpsc::Sender<Result<Value>>,
        _args: Self::Args,
    ) -> impl Future<Output = ()> + Send {
        async move {
            // Default: no async support
            // Async tools must override this
        }
    }

    /// Optional: custom topic for UI display
    fn topic(&self) -> String {
        format_tool_name(Self::NAME)
    }
}

// ============================================================================
// 2. Wrapper Type - Solves Orphan Rule Problem
// ============================================================================

/// Wrapper to enable auto-impl of rig::Tool for AomiTool
/// We own this type, so we can impl foreign traits on it
#[derive(Clone)]
pub struct AomiToolWrapper<T: AomiTool> {
    pub inner: T,
}

impl<T: AomiTool> AomiToolWrapper<T> {
    pub fn new(tool: T) -> Self {
        Self { inner: tool }
    }
}

// Helper macro for registration
#[macro_export]
macro_rules! aomi_tool {
    ($tool:expr) => {
        AomiToolWrapper::new($tool)
    };
}

// ============================================================================
// 3. Auto-impl rig::Tool for AomiToolWrapper<T>
// ============================================================================

impl<T: AomiTool> rig::Tool for AomiToolWrapper<T> {
    const NAME: &'static str = T::NAME;

    // Args wrapper includes session_id
    type Args = AomiToolArgs<T::Args>;
    type Output = Value;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut schema = self.inner.parameters_schema();

        // Inject session_id into schema (optional, auto-provided)
        if let Some(obj) = schema.as_object_mut() {
            if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
                props.insert(
                    "session_id".to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": "Internal session identifier (auto-injected)"
                    })
                );
            }
        }

        ToolDefinition {
            name: Self::NAME.to_string(),
            description: self.inner.description().to_string(),
            parameters: schema,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let session_id = args.session_id();
        let tool_args = args.args;

        // Get scheduler and session handler
        let scheduler = ToolScheduler::get_or_init()
            .await
            .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

        let handler = scheduler.get_session_handler(session_id.clone());

        // Generate unique call_id
        let call_id = CallMetadata::new(
            format!("{}_{}", T::NAME, uuid::Uuid::new_v4()),
            None,
        );

        // Execute based on sync/async support
        let tool_receiver = if self.inner.support_async() {
            // Async: spawn thread, stream results via mpsc
            let (tx, rx) = mpsc::channel::<Result<Value>>(100);
            let tool = self.inner.clone();

            tokio::spawn(async move {
                tool.run_async(tx, tool_args).await;
            });

            ToolReciever::new_multi_step(call_id.clone(), T::NAME.to_string(), rx)
        } else {
            // Sync: spawn thread, single result via oneshot
            let (tx, rx) = oneshot::channel::<Result<Value>>();
            let tool = self.inner.clone();

            tokio::spawn(async move {
                tool.run_sync(tx, tool_args).await;
            });

            ToolReciever::new_single(call_id.clone(), T::NAME.to_string(), rx)
        };

        // Register with session handler
        {
            let mut guard = handler.lock().await;
            guard.register_receiver(tool_receiver);
        }

        // Return immediate "queued" response to LLM
        Ok(serde_json::json!({
            "status": if self.inner.support_async() { "streaming" } else { "queued" },
            "call_id": call_id.id,
            "tool_name": T::NAME,
            "message": format!("Tool {} execution started", T::NAME)
        }))
    }
}

// ============================================================================
// 4. Args Wrapper - Injects session_id
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AomiToolArgs<T> {
    /// Session ID (auto-injected by completion layer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Actual tool arguments
    #[serde(flatten)]
    pub args: T,
}

impl<T> AomiToolArgs<T> {
    pub fn session_id(&self) -> String {
        self.session_id.clone().unwrap_or_else(|| "default".to_string())
    }
}

// ============================================================================
// 5. Updated Scheduler - Service + Session Registry
// ============================================================================

pub struct ToolScheduler {
    /// Tool registry (removed - tools now in Rig registry)
    /// We keep metadata only, name -> meta
    tool_metadata: Arc<RwLock<HashMap<String, ToolMetadata>>>,

    /// Session handlers - one per active session
    session_handlers: Arc<RwLock<HashMap<String, Arc<Mutex<ToolHandler>>>>>,

    /// Runtime (no longer needed for event loop, but kept for spawning)
    runtime: Arc<SchedulerRuntime>,
}

pub struct ToolMetadata {
    pub name: String,
    pub namespace: String,
    pub description: String,
    pub is_async: bool,
}

impl ToolScheduler {
    /// Get or create handler for a session
    pub fn get_session_handler(&self, session_id: String, namespaces: Vec<String>) -> Arc<Mutex<ToolHandler>> {
        let mut handlers = self.session_handlers.write().unwrap();

        // get the subset with name space
        let tools = self.tool_metadata.iter().map(....).collect()

        handlers
            .entry(session_id.clone())
            .or_insert_with(|| {
                eprintln!("Creating new ToolHandler for session: {}", session_id);
                Arc::new(Mutex::new(ToolHandler::new()))
            })
            .clone()
    }

    /// Cleanup session when user logs off
    pub async fn cleanup_session(&self, session_id: &str) {
        // Persist incomplete calls
        if let Some(handler) = self.session_handlers.read().unwrap().get(session_id) {
            let guard = handler.lock().await;
            self.persist_handler_state(session_id, &guard).await;
        }

        // Remove from memory
        self.session_handlers.write().unwrap().remove(session_id);
        eprintln!("Cleaned up session: {}", session_id);
    }

    /// Restore session from persistence
    pub async fn restore_session(&self, session_id: &str, persisted_data: PersistedHandlerState) -> Result<()> {
        let state = self.load_handler_state(session_id).await?;
        let handler = Arc::new(Mutex::new(ToolHandler::from_persisted(state)));

        self.session_handlers
            .write()
            .unwrap()
            .insert(session_id.to_string(), handler);

        Ok(())
    }

    async fn to_persisted_state(&self, session_id: &str) -> Result<PersistedHandlerState> {
        // Serialize incomplete calls to DB
        // Implementation in Phase 5
    }
}

// ============================================================================
// 6. ToolHandler - Per-Session State
// ============================================================================

pub struct ToolHandler {
    namespaces: Vec<String>,
    /// the allowed too set of the requested namespace
    avaliable_tools: HashMap<String, ToolMetadata>>,
    /// Unresolved tool calls (receivers not yet converted to streams)
    unresolved_calls: Vec<ToolReciever>,

    /// Ongoing streams being polled
    ongoing_streams: Vec<ToolStream>,

    /// Completed tool results ready for consumption
    completed_calls: Vec<ToolCompletion>, // take_completed_call()
}

impl ToolHandler {
    pub fn new(tools: HashMap<String, ToolMetadata>>) -> Self {
        Self {
            avaliable_tools: tools,
            unresolved_calls: Vec::new(),
            ongoing_streams: Vec::new(),
            completed_calls: Vec::new(),
        }
    }

    /// Register a new tool receiver (called by auto-impl)
    pub fn register_receiver(&mut self, receiver: ToolReciever) {
        self.unresolved_calls.push(receiver);
    }

    /// Convert unresolved calls to streams (called by session layer)
    pub fn resolve_calls(&mut self) -> Option<Vec<ToolStream>> {
        if self.unresolved_calls.is_empty() {
            return None;
        }

        let mut ui_streams = Vec::new();
        while let Some(mut receiver) = self.unresolved_calls.pop() {
            let (bg_stream, ui_stream) = receiver.into_shared_streams();
            self.ongoing_streams.push(bg_stream);
            ui_streams.push(ui_stream);
        }

        Some(ui_streams)
    }

    /// Poll all streams once (non-blocking)
    pub fn poll_streams_once(&mut self) -> usize {
        use std::task::Poll;

        let mut count = 0;
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        let mut i = 0;
        while i < self.ongoing_streams.len() {
            let stream = &mut self.ongoing_streams[i];

            match Pin::new(&mut *stream).poll_next(&mut cx) {
                Poll::Ready(Some((call_id, result))) => {
                    let is_first_chunk = stream.is_multi_step() && !stream.first_chunk_sent;

                    if is_first_chunk {
                        stream.first_chunk_sent = true;
                    }

                    self.completed_calls.push(ToolCompletion {
                        call_id,
                        tool_name: stream.tool_name.clone(),
                        sync: !stream.is_multi_step() || is_first_chunk,
                        result,
                    });

                    count += 1;

                    if stream.is_multi_step() {
                        i += 1; // Keep stream for more chunks
                    } else {
                        self.ongoing_streams.swap_remove(i); // Remove completed
                    }
                }
                Poll::Ready(None) => {
                    self.ongoing_streams.swap_remove(i); // Stream exhausted
                }
                Poll::Pending => {
                    i += 1; // Not ready yet
                }
            }
        }

        count
    }

    /// Take all completed calls (for pushing to SystemEventQueue)
    pub fn take_completed_calls(&mut self) -> Vec<ToolCompletion> {
        std::mem::take(&mut self.completed_calls)
    }

    /// Check if handler has work pending
    pub fn has_pending_work(&self) -> bool {
        !self.unresolved_calls.is_empty()
            || !self.ongoing_streams.is_empty()
            || !self.completed_calls.is_empty()
    }


    pub fn sanitized_persist(&self) -> Result<PersistedHandlerState> {
      // finish polling the unsolved and ongoing calls
      // make sure everthing becomes completed_calls, and then call below
      
      self.to_persisted()
    }

    /// Serialize state for persistence
    pub fn to_persisted(&self) -> PersistedHandlerState {
        PersistedHandlerState {
            self.namespaces,
            self.avaliable_tools
            self.completed_calls
        }
    }

    /// Restore from persisted state
    pub fn from_persisted(state: PersistedHandlerState) -> Self {
        // Implementation in Phase 5

        // request the same tool set from scheduler
        // insert state.incompleted_calls to the completed_calls of this round
        // the idea is we keep the on-going task in memory before they end,
        // once they end we persist, so when we recovered from persisted things are completed and redy to be consumed

        unimplemented!()
    }
}

// ============================================================================
// 7. Supporting Types
// ============================================================================

// Remove ToolCallID and change to this
pub struct CallMetadata {
  pub name: String,
      pub id: String,
    pub call_id: Option<String>,
    pub is_async: bool,
}


pub struct ToolCompletion {
    pub metadata: CallMetadata
    pub result: Result<Value, String>,
}

pub struct ToolStream {
      pub metadata: CallMetadata
    inner: Option<StreamInner>,
    /// Marks whether the first chunk (sync ACK) has been seen for multi-step streams.
    pub first_chunk_sent: bool,
}


/// Internal type that holds the actual channel receivers.
/// Use `into_shared_streams()` to convert to UI-consumable `ToolStreamream`.
pub struct ToolReciever {
    metadata: CallMetadata,
    finished: bool,
    /// Multi-step tools use mpsc receiver for streaming chunks
    multi_step_rx: Option<mpsc::Receiver<Result<Value>>>,
    /// Single-result tools use oneshot receiver
    single_rx: Option<oneshot::Receiver<Result<Value>>>,
}


pub struct PersistedHandlerState {
    pub namespace: Vec<String>, // Alice used L2Beat tool set & polymarket, default
    pub avaliable_tools: Vec<String>,
    pub completed_calls: Vec<ToolCompletion>,
}
```

### Usage Pattern (With Wrapper)

```
┌─────────────────────────────────────────────────────────────────┐
│  IMPLEMENTATION PATTERN                                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Step 1: Define your tool                                       │
│  ─────────────────────────                                      │
│    struct GetCurrentTime;                                       │
│    struct GetCurrentTimeArgs { topic: String }                  │
│                                                                  │
│  Step 2: Implement AomiTool                                     │
│  ───────────────────────────                                    │
│    impl AomiTool for GetCurrentTime {                           │
│      const NAME = "get_current_time";                           │
│      const NAMESPACE = "time";                                  │
│      type Args = GetCurrentTimeArgs;                            │
│      type Output = String;                                      │
│                                                                  │
│      fn run_sync(sender, args) {                                │
│        let timestamp = chrono::Utc::now().timestamp();          │
│        sender.send(Ok(timestamp.to_string()));                  │
│      }                                                          │
│    }                                                            │
│                                                                  │
│  Step 3: Wrapper gets rig::Tool auto-impl                       │
│  ──────────────────────────────────────                         │
│    impl<T: AomiTool> rig::Tool for AomiToolWrapper<T> {         │
│      // Already implemented (see Core Types above)              │
│      // Handles session_id injection                            │
│      // Spawns execution                                        │
│      // Registers with handler                                  │
│      // Returns queued status                                   │
│    }                                                            │
│                                                                  │
│  Step 4: Register with agent (wrap your tool)                   │
│  ──────────────────────────────────────────                     │
│    // Option A: Explicit wrapper                                │
│    agent.tool(AomiToolWrapper::new(GetCurrentTime))             │
│                                                                  │
│    // Option B: Use helper macro                                │
│    agent.tool(aomi_tool!(GetCurrentTime))                       │
│                                                                  │
│  Why the wrapper?                                               │
│  ────────────────                                               │
│    Rust's orphan rule prevents:                                 │
│      impl<T: AomiTool> rig::Tool for T { }                      │
│                                                                  │
│    We don't own rig::Tool, so we need a local wrapper type.     │
│    AomiToolWrapper is zero-cost (newtype pattern).              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Function Call Path

```
┌──────────────────────────────────────────────────────────────────┐
│  DETAILED CALL PATH                                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. LLM decides to call "forge_execute"                           │
│     ↓                                                             │
│  2. completion.rs receives ToolCall                               │
│     - Extract: name="forge_execute"                               │
│     - Extract: args={plan: [...], topic: "Deploy"}                │
│     - Extract: session_id from state                              │
│     ↓                                                             │
│  3. Call agent.tools.call("forge_execute", json_args)             │
│     - Rig looks up tool in registry                               │
│     - Deserializes args                                           │
│     ↓                                                             │
│  4. rig::Tool::call() [auto-impl]                                 │
│     - Wraps args with session_id                                  │
│     - AomiToolArgs { session_id, args }                           │
│     ↓                                                             │
│  5. Check support_async()                                         │
│     - forge_execute returns true                                  │
│     ↓                                                             │
│  6. Async path: create mpsc channel                               │
│     - (tx, rx) = mpsc::channel(100)                               │
│     ↓                                                             │
│  7. Spawn background task                                         │
│     - tokio::spawn(tool.run_async(tx, args))                      │
│     ↓                                                             │
│  8. Create ToolReciever with CallMetadata                         │
│     - metadata = CallMetadata {                                   │
│         name: "forge_execute",                                    │
│         id: uuid,                                                 │
│         call_id: Some(rig_call_id),                               │
│         is_async: true                                            │
│       }                                                           │
│     - ToolReciever::new_multi_step(metadata, rx)                  │
│     ↓                                                             │
│  9. Get session handler (with namespace filtering)                │
│     - namespaces = ["forge", "ethereum"]  // From session config  │
│     - handler = scheduler.get_session_handler(session_id, namespaces) │
│     - Handler only has tools from these namespaces                │
│     ↓                                                             │
│  10. Register receiver                                            │
│     - handler.register_receiver(receiver)                         │
│     - Adds to unresolved_calls                                    │
│     ↓                                                             │
│  11. Return to LLM                                                │
│     - JSON: {status: "streaming", id: metadata.id, message}       │
│     ↓                                                             │
│  12. Background: tool executes                                    │
│     - For each step in plan:                                      │
│       - tx.send(progress_json)                                    │
│     - Drop tx when done (closes channel)                          │
│     ↓                                                             │
│  13. Session layer polls                                          │
│     - handler.poll_streams_once()                                 │
│     - Converts receivers → streams → completions                  │
│     - Pushes to SystemEventQueue                                  │
│     ↓                                                             │
│  14. Frontend receives                                            │
│     - sync_state() returns system_events                          │
│     - UI displays progress                                        │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

### Session Handler Lifecycle

```
┌──────────────────────────────────────────────────────────────┐
│  SESSION HANDLER LIFECYCLE                                    │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  Session Start:                                               │
│  ──────────────                                               │
│    User logs in → session_id = "alice_123"                    │
│    namespaces = ["forge", "ethereum"]  // From user config    │
│    scheduler.get_session_handler("alice_123", namespaces)     │
│      ↳ Creates new ToolHandler if not exists                  │
│      ↳ Filters tool_metadata by namespaces                    │
│      ↳ Handler only sees tools in these namespaces            │
│      ↳ Or loads from persistence (see below)                  │
│                                                               │
│  During Session:                                              │
│  ──────────────                                               │
│    Tool calls:                                                │
│      tool.call() → handler.register_receiver()                │
│        ↳ unresolved_calls.push(receiver)                      │
│                                                               │
│    Polling loop (session layer):                              │
│      handler.poll_streams_once()                              │
│        ↳ Converts unresolved → ongoing streams                │
│        ↳ Polls ongoing → completed                            │
│        ↳ Returns completions to push to SystemEventQueue      │
│                                                               │
│  Session End (Graceful Logout):                               │
│  ──────────────────────────────                               │
│    User logs off                                              │
│    scheduler.cleanup_session("alice_123")                     │
│      1. handler.sanitized_persist():                          │
│         - Poll all unresolved_calls until empty               │
│         - Poll all ongoing_streams until empty                │
│         - Everything becomes completed_calls                  │
│      2. state = handler.to_persisted():                       │
│         - Serialize namespaces, tools, completed_calls        │
│      3. Save state to DB                                      │
│      4. Remove handler from memory                            │
│                                                               │
│  Session Resume:                                              │
│  ──────────────                                               │
│    User logs back in                                          │
│    scheduler.restore_session("alice_123", persisted_data)     │
│      1. Load PersistedHandlerState from DB                    │
│      2. Create new handler with same namespaces/tools         │
│      3. Insert persisted completed_calls into handler         │
│      4. User can now consume those results                    │
│                                                               │
│  Key Insight:                                                 │
│  ────────────                                                 │
│    We DON'T serialize ongoing async tasks (too complex).      │
│    Instead: wait for everything to complete, then persist     │
│    the results. On restore, results are ready to consume.     │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

---

## Migration Phases

### Phase 0: Preparation (1-2 days)

**Goal**: Set up infrastructure without breaking existing code

**Tasks**:
1. Create new `aomi-tool-v2` crate (experimental)
2. Define `AomiTool` trait
3. Implement auto-impl for `rig::Tool`
4. Add session_id to `CoreState`
5. Write initial tests

**Files**:
- `NEW`: `crates/tools-v2/src/lib.rs` - AomiTool trait
- `NEW`: `crates/tools-v2/src/auto_impl.rs` - rig::Tool blanket impl
- `NEW`: `crates/tools-v2/src/tests/` - Test suite
- `MODIFY`: `crates/chat/src/app.rs` - Add session_id to CoreState

**Success Criteria**:
- [ ] AomiTool trait compiles
- [ ] Auto-impl works for simple mock tool
- [ ] Tests pass showing sync execution
- [ ] Tests pass showing async execution
- [ ] No impact on existing code

**Test Coverage**:
- Unit tests for AomiTool trait
- Unit tests for auto-impl
- Integration test: mock sync tool
- Integration test: mock async tool
- Verify session_id propagation

---

### Phase 1: Scheduler Refactor (2-3 days)

**Goal**: Convert scheduler from executor to service

**Tasks**:
1. Remove event loop from scheduler
2. Add `session_handlers` map
3. Implement `get_session_handler(session_id)`
4. Implement `cleanup_session(session_id)`
5. Add `register_receiver()` to ToolHandler
6. Update tests

**Changes**:

```
Scheduler (Before):
  • tools: HashMap<String, Arc<dyn AnyTool>>
  • requests_tx: mpsc::Sender  ← REMOVE
  • runtime: Arc<Runtime>
  • run(event_loop) { ... }    ← REMOVE

Scheduler (After):
  • tools: HashMap<String, ToolMetadata>
  • session_handlers: HashMap<SessionId, Arc<Mutex<ToolHandler>>>
  • get_session_handler(session_id) -> Arc<Mutex<ToolHandler>>
  • cleanup_session(session_id)
  • restore_session(session_id)

ToolHandler (New Method):
  • register_receiver(receiver: ToolReciever)
    ↳ Adds to unresolved_calls
    ↳ Ready for polling
```

**Success Criteria**:
- [ ] Scheduler has no event loop
- [ ] `get_session_handler()` works
- [ ] Multiple sessions isolated
- [ ] Cleanup doesn't affect other sessions
- [ ] All existing scheduler tests pass

**Test Coverage**:
- Session isolation test
- Concurrent session test
- Cleanup test
- Handler registration test
- Backward compatibility test

---

### Phase 2: Migrate One Tool (2-3 days)

**Goal**: Prove migration pattern with real tool

**Tasks**:
1. Choose simple tool (GetCurrentTime)
2. Implement `AomiTool` for it
3. Keep old `rig::Tool` impl (parallel)
4. Add feature flag `use-aomi-tool-v2`
5. Test both paths work
6. Measure performance

**Approach**:
- Conditional compilation based on feature flag
- Both implementations coexist temporarily
- Gradual rollout with A/B testing

**Success Criteria**:
- [ ] Tool works with `use-aomi-tool-v2` enabled
- [ ] Tool works with flag disabled (old path)
- [ ] Both paths produce same results
- [ ] Integration tests pass
- [ ] No performance regression (<5%)

**Test Coverage**:
- V1 vs V2 result comparison
- Latency benchmark
- Concurrent call test
- Error handling test
- Integration test with real LLM

---

### Phase 3: Completion.rs Simplification (2-3 days)

**Goal**: Remove scheduler awareness from completion layer

**Tasks**:
1. Remove `handler` field from `CompletionRunner`
2. Remove `process_tool_call()` (scheduler path)
3. Rename `process_tool_call_fallback()` → `process_tool_call()`
4. Update all tool calls to use Rig path
5. Remove scheduler imports
6. Update tests

**Code Impact**:
```
CompletionRunner:
  REMOVE: handler: Option<SessionToolHander>
  REMOVE: process_tool_call()
  RENAME: process_tool_call_fallback() → process_tool_call()
  SIMPLIFY: consume_tool_call() - single path

Expected Reduction:
  Lines: 610 → ~200
  Complexity: High → Low
  Dependencies: -6 imports
```

**Success Criteria**:
- [ ] `completion.rs` < 200 lines
- [ ] No scheduler imports
- [ ] Single tool call path
- [ ] All completion tests pass
- [ ] Integration tests pass

**Test Coverage**:
- Tool call flow test
- Error handling test
- Multi-tool test
- Stream handling test
- Regression test suite

---

### Phase 4: Migrate Remaining Tools (3-5 days)

**Goal**: Convert all tools to AomiTool pattern

**Tool Categories**:

**Sync Tools** (8-10 tools, ~30min each):
- GetCurrentTime ✓ (done in phase 2)
- EncodeFunctionCall
- GetContractABI
- GetContractSourceCode
- GetErc20Balance
- GetAccountInfo
- GetAccountTransactionHistory
- All cast tools (7 tools)

**Async Tools** (2-3 tools, ~2hr each):
- ForgeExecutor
- BraveSearch
- Future streaming tools

**Per-Tool Process**:
1. Implement `AomiTool` trait
2. Move logic to `run_sync()` or `run_async()`
3. Remove old `rig::Tool` impl
4. Update unit tests
5. Integration test
6. Mark complete

**Success Criteria**:
- [ ] All tools implement AomiTool
- [ ] No tools directly implement rig::Tool
- [ ] All tool tests pass
- [ ] End-to-end tests pass
- [ ] No feature flags needed

**Tracking**:
Use checklist in Appendix A for each tool

---

### Phase 5: Session Persistence (2-3 days)

**Goal**: Enable graceful session persistence on logout with completed results

**Core Strategy**:
Instead of trying to serialize ongoing async tasks (very complex), we:
1. **On logout**: Poll everything to completion, then persist the results
2. **On login**: Restore completed results for user to consume

**Tasks**:
1. Implement `sanitized_persist()` in ToolHandler (poll to completion)
2. Implement `to_persisted()` serialization
3. Implement `from_persisted()` deserialization
4. Add DB schema for completed tool results
5. Hook into session logout/login events
6. Handle timeout for long-running tasks
7. Test restore/resume flow

**DB Schema**:
```sql
CREATE TABLE session_tool_state (
  session_id VARCHAR(255) PRIMARY KEY,
  user_id VARCHAR(255) NOT NULL,
  namespace VARCHAR(255) NOT NULL,
  last_updated TIMESTAMP NOT NULL,
  tool_results JSONB NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
);

-- tool_results structure (PersistedHandlerState):
{
  "namespace": "forge,ethereum",
  "available_tools": ["forge_execute", "encode_function_call", ...],
  "completed_calls": [
    {
      "metadata": {
        "name": "forge_execute",
        "id": "uuid-here",
        "call_id": "llm-call-id",
        "is_async": true
      },
      "result": {
        "Ok": { ... }  // or "Err": "error message"
      }
    }
  ]
}
```

**Implementation Details**:

**1. Graceful Logout Flow**:
```rust
// When user logs off:
async fn cleanup_session(session_id: &str) {
    let handler = get_session_handler(session_id);

    // Poll everything to completion (with timeout)
    let state = handler.lock().await.sanitized_persist().await?;

    // Serialize and save
    save_to_db(session_id, state).await;

    // Remove from memory
    drop(handler);
}
```

**2. Login Restore Flow**:
```rust
// When user logs back in:
async fn restore_session(session_id: &str, namespaces: Vec<String>) {
    // Load from DB
    let persisted = load_from_db(session_id).await?;

    // Create new handler with same tool set
    let tools = get_tools_for_namespaces(&namespaces);
    let handler = ToolHandler::from_persisted(persisted, tools);

    // User can now consume completed_calls
}
```

**3. Timeout Handling**:
- Set max wait time for `sanitized_persist()` (e.g., 30 seconds)
- If timeout exceeded:
  - Mark remaining calls as "timed_out"
  - Persist what we have
  - Log warning for monitoring

**Success Criteria**:
- [ ] `sanitized_persist()` polls to completion
- [ ] Session state serializes correctly
- [ ] Session state deserializes correctly
- [ ] Restored results are consumable
- [ ] Timeout handling works
- [ ] No data loss on graceful logout
- [ ] Tests verify round-trip

**Important Note**:
This approach trades real-time persistence for simplicity. Long-running tasks must complete before logout finishes. For tasks that take hours:
- Keep session alive (background tab)
- Or accept that user must wait for completion
- Future: Could add "cancel on logout" option

**Test Coverage**:
- Serialization test (PersistedHandlerState ↔ JSON)
- Graceful logout test (everything completes)
- Restore test (completed calls available)
- Timeout test (long-running task)
- Multi-session isolation test
- Namespace filtering on restore

---

### Phase 6: Cleanup & Documentation (2-3 days)

**Goal**: Remove old code, update docs

**Tasks**:
1. Remove `tools-v2` crate (merge into main)
2. Remove feature flags
3. Delete old scheduler event loop code
4. Update architecture docs
5. Update tool development guide
6. Add migration guide for custom tools
7. Update PROGRESS.md

**Documentation Updates**:
- `docs/architecture.md` - New tool flow diagrams
- `docs/guides/tool-development.md` - How to implement AomiTool
- `specs/TOOL-MIGRATION.md` - Guide for migrating existing tools
- `specs/PROGRESS.md` - Mark refactor complete
- `README.md` - Update examples

**Success Criteria**:
- [ ] No dead code
- [ ] No feature flags
- [ ] Docs reflect new architecture
- [ ] Migration guide tested by team member
- [ ] All tests green

---

## Testing Strategy

### Unit Testing Approach

**Per Component Tests**:

```
AomiTool Trait:
  ✓ Sync tool execution
  ✓ Async tool execution
  ✓ Error handling
  ✓ Schema generation
  ✓ Session ID handling

Auto-impl rig::Tool:
  ✓ Arguments wrapping
  ✓ Channel creation
  ✓ Receiver registration
  ✓ Return value format
  ✓ Timeout handling

Scheduler Service:
  ✓ Session handler creation
  ✓ Session isolation
  ✓ Cleanup without interference
  ✓ Concurrent access
  ✓ Persistence/restore

ToolHandler:
  ✓ Receiver registration
  ✓ Stream polling
  ✓ Completion collection
  ✓ Multi-session safety
```

### Integration Testing

**End-to-End Scenarios**:

**Scenario 1: Sync Tool Call**
```
User: "Get current time"
Flow:
  1. LLM → ToolCall("get_current_time")
  2. System → Execute via AomiTool
  3. Result → Streamed to user

Assertions:
  - Single code path used
  - Result correct format
  - Latency < 100ms
  - Session ID propagated
```

**Scenario 2: Async Tool Call**
```
User: "Execute forge plan"
Flow:
  1. LLM → ToolCall("forge_execute")
  2. System → Spawn background task
  3. System → Stream progress updates
  4. Result → Multiple events to user

Assertions:
  - First chunk returned quickly (<100ms)
  - Subsequent chunks streamed
  - All steps completed
  - Session handler cleaned up
  - No memory leaks
```

**Scenario 3: Session Persistence**
```
Flow:
  1. User → Start long async task
  2. User → Logs off
  3. System → Persist incomplete calls
  4. User → Logs back in
  5. System → Restore session
  6. Result → Task resumes, completes

Assertions:
  - No progress lost
  - Task completes successfully
  - User sees all updates
  - Correct order maintained
```

**Scenario 4: Multi-Session Isolation**
```
Flow:
  1. Alice → Start forge execution
  2. Bob → Start different forge execution
  3. System → Both run concurrently
  4. Result → No interference

Assertions:
  - Sessions isolated
  - Correct results to correct users
  - No cross-contamination
  - Independent cleanup
```

### Performance Testing

**Benchmarks**:

```
1. Tool Call Latency
   Measure: Time from LLM ToolCall to first result
   Baseline: 45ms (p50), 120ms (p99)
   Target: <50ms (p50), <100ms (p99)

2. Concurrent Sessions
   Measure: Number of sessions handled
   Baseline: ~50
   Target: >100

3. Memory Per Session
   Measure: Memory footprint per handler
   Target: <1MB per session

4. Session Cleanup
   Measure: Time to cleanup session
   Target: <10ms

5. Restore Time
   Measure: Time to restore persisted session
   Target: <100ms

6. Throughput
   Measure: Tool calls per second
   Target: >1000 calls/sec
```

### Regression Testing

**Before/After Validation**:

```
Run Suite With:
  - Old implementation (baseline)
  - New implementation (comparison)

Compare:
  ✓ Pass rate (must be 100%)
  ✓ Execution time (±10% acceptable)
  ✓ Memory usage (±20% acceptable)
  ✓ Error messages (must match semantically)
  ✓ Result correctness (exact match)
```

---

## Rollback Plan

### Per-Phase Rollback Strategy

**Phase 0 - Preparation**:
```
Rollback: Delete tools-v2 crate
Impact: None (experimental only)
Time: 5 minutes
```

**Phase 1 - Scheduler**:
```
Rollback:
  1. Restore scheduler.rs from git
  2. Keep old event loop
  3. Remove session_handlers
Impact: Medium (scheduler changes)
Time: 30 minutes
```

**Phase 2 - One Tool**:
```
Rollback:
  1. Disable feature flag
  2. Use old rig::Tool impl
Impact: Low (single tool)
Time: 10 minutes
```

**Phase 3 - Completion**:
```
Rollback:
  1. Restore completion.rs from git
  2. Re-add handler field
  3. Restore dual paths
Impact: High (core flow)
Time: 1 hour
```

**Phase 4 - All Tools**:
```
Rollback:
  1. Per-tool rollback
  2. Restore old rig::Tool impls
  3. Disable AomiTool
Impact: High (all tools)
Time: 2-3 hours
```

**Phase 5 - Persistence**:
```
Rollback:
  1. Disable persistence calls
  2. In-memory only
Impact: Low (optional feature)
Time: 15 minutes
```

**Phase 6 - Cleanup**:
```
Rollback:
  1. Restore deleted code
  2. Re-add feature flags
  3. Revert docs
Impact: Medium (cleanup)
Time: 1 hour
```

### Emergency Rollback Procedure

**If Critical Issue Found**:

```
1. Immediate Actions (< 5 min):
   - Disable feature flag globally
   - Fall back to old scheduler
   - Notify team on Slack
   - Create incident ticket

2. Investigation (30 min):
   - Collect error logs
   - Reproduce in staging
   - Identify root cause
   - Assess impact

3. Decision Point:
   - If fixable quickly (<2 hours): Fix forward
   - Otherwise: Complete rollback

4. Post-Incident:
   - Document issue
   - Add test case to prevent recurrence
   - Update migration plan
   - Team retrospective
```

---

## Success Metrics

### Code Metrics

**Line Count Reduction**:
```
completion.rs:
  Before: 610 lines
  After: ~200 lines
  Reduction: 67%

scheduler.rs:
  Before: 638 lines
  After: ~400 lines
  Reduction: 37%

Per Tool:
  Before: ~80 lines
  After: ~50 lines
  Reduction: 38%
```

**Complexity Reduction**:
```
completion.rs:
  Cyclomatic Complexity: 45 → 18
  Code Paths: 2 → 1
  Dependencies: 12 → 6

scheduler.rs:
  Event Loop: Yes → No
  Global State: High → Low
  Testability: Complex → Simple
```

### Performance Metrics

**Latency**:
```
Tool Call (p50):
  Baseline: 45ms
  Target: 40ms
  Acceptable: <50ms

Tool Call (p99):
  Baseline: 120ms
  Target: 100ms
  Acceptable: <150ms
```

**Throughput**:
```
Concurrent Sessions:
  Baseline: ~50
  Target: >100
  Stretch: >200

Tools/Second:
  Baseline: N/A (new metric)
  Target: >1000
```

**Resource Usage**:
```
Memory Per Session:
  Target: <1MB
  Max Acceptable: <2MB

Session Overhead:
  Target: <100KB
  Max Acceptable: <500KB
```

### Reliability Metrics

**Data Integrity**:
```
Session Persistence:
  Incomplete Calls Lost: 0%
  Restore Success Rate: >99%
  Data Corruption: 0%

Multi-User Safety:
  Session Isolation: 100%
  Cross-contamination: 0%
  Race Conditions: 0
```

**Uptime**:
```
Service Availability:
  Target: 99.9%
  No regressions from baseline

Error Recovery:
  Graceful Degradation: Yes
  Automatic Retry: Yes
  User-Facing Errors: <1%
```

### Developer Experience

**Development Time**:
```
Tool Implementation:
  Before: 2-3 hours
  After: 30-60 minutes
  Improvement: 3-4x faster

Test Setup:
  Before: 30 minutes (scheduler setup)
  After: 5 minutes (standalone)
  Improvement: 6x faster
```

**Code Clarity**:
```
New Developer Onboarding:
  Before: "Complex flow, multiple paths"
  After: "Simple, unified path"

Code Reviews:
  Before: Long discussions about flow
  After: Focus on business logic
```

---

## Appendix A: Tool Migration Checklist

```markdown
## Tool: [TOOL_NAME]

### Pre-Migration
- [ ] Review existing implementation
- [ ] Identify if sync or async
- [ ] List dependencies (DB, external APIs, etc.)
- [ ] Review test coverage (current %)
- [ ] Estimate effort (S/M/L)

### Implementation
- [ ] Create `[Tool]Args` struct
- [ ] Implement `AomiTool` trait
  - [ ] Define NAME constant
  - [ ] Define NAMESPACE constant
  - [ ] Set Args/Output types
  - [ ] Implement description()
  - [ ] Implement parameters_schema()
  - [ ] Implement run_sync() OR run_async()
  - [ ] Add error handling
- [ ] Remove old `rig::Tool` impl
- [ ] Update imports
- [ ] Handle edge cases

### Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual CLI test
- [ ] Verify error handling
- [ ] Check async behavior (if applicable)
- [ ] Performance test (no regression)
- [ ] Session isolation test

### Documentation
- [ ] Update tool docs
- [ ] Add code comments
- [ ] Update examples
- [ ] Migration notes

### Review & Deploy
- [ ] Self-review
- [ ] Code review complete
- [ ] Tests green in CI
- [ ] Staging deployment
- [ ] Production deployment
- [ ] Monitor for issues

### Post-Migration
- [ ] Verify in production
- [ ] Check metrics
- [ ] User feedback
- [ ] Mark complete
```

---

## Appendix B: Architecture Decision Records

### ADR-001: Wrapper Pattern to Solve Orphan Rule

**Context**: We want to auto-implement `rig::Tool` for all types implementing `AomiTool`, but Rust's orphan rule prevents this.

**Problem**:
```rust
// ❌ This violates orphan rule
impl<T: AomiTool> rig::Tool for T { }
// We don't own rig::Tool, we don't own all T
```

**Decision**: Use a newtype wrapper (`AomiToolWrapper<T>`) that we own

**Rationale**:
- ✅ Satisfies orphan rule (we own the wrapper type)
- ✅ Zero runtime cost (newtype pattern)
- ✅ Still gets auto-impl benefits
- ✅ Simple macro helper for ergonomics: `aomi_tool!(MyTool)`
- ❌ Con: Extra wrapper during registration (minor)

**Implementation**:
```rust
pub struct AomiToolWrapper<T: AomiTool>(pub T);

// ✅ Legal: we own AomiToolWrapper
impl<T: AomiTool> rig::Tool for AomiToolWrapper<T> { }

// Usage
agent.tool(aomi_tool!(GetCurrentTime))  // Wraps automatically
```

**Alternatives Considered**:
1. **Proc macro `#[aomi_tool]`**
   - Pro: No wrapper needed
   - Con: More complex, worse IDE support, magic

2. **Manual impl for each tool**
   - Pro: No wrapper
   - Con: Defeats purpose of reducing boilerplate

3. **Give up on auto-impl**
   - Pro: No orphan rule issues
   - Con: Loses main benefit of abstraction

**Status**: Accepted

---

### ADR-002: Blanket Impl Over Proc Macro

**Context**: How to implement the auto-impl for `rig::Tool`

**Decision**: Use blanket trait impl instead of proc macro

**Rationale**:
- ✅ Less magic, easier to debug
- ✅ Better IDE support (autocomplete, go-to-definition)
- ✅ Can inspect auto-impl in docs
- ✅ Simpler build process (no proc-macro crate)
- ✅ Easier to maintain
- ❌ Con: Requires wrapper for orphan rule (see ADR-001)

**Status**: Accepted

---

### ADR-003: Session ID in Tool Args vs Context

**Context**: Tools need session awareness for handler lookup

**Decision**: Session ID in tool arguments (via AomiToolArgs wrapper)

**Rationale**:
- ✅ Works with existing Rig infrastructure
- ✅ Serializable (can log, replay, debug)
- ✅ Explicit in function signature
- ✅ No global state needed
- ❌ Con: Pollutes args namespace slightly

**Alternatives Considered**:
1. **Thread-local storage**
   - Pro: Clean API
   - Con: Not serializable
   - Con: Hard to test

2. **Global context**
   - Pro: No API changes
   - Con: Breaks testability
   - Con: Race conditions

3. **Separate context parameter**
   - Pro: Clean separation
   - Con: Breaks Rig API compatibility

**Status**: Accepted

---

### ADR-004: Scheduler as Service vs Trait

**Context**: Need to manage session handlers and tool registry

**Decision**: Scheduler as concrete service (singleton)

**Rationale**:
- ✅ Simple to use (no DI needed)
- ✅ Global registry makes sense for tools
- ✅ Easy session management
- ✅ Single source of truth
- ❌ Con: Harder to test in isolation (but manageable)

**Alternatives Considered**:
1. **Scheduler trait with DI**
   - Pro: More testable
   - Con: Over-engineered for this use case
   - Con: Adds complexity everywhere

2. **Per-session schedulers**
   - Pro: Complete isolation
   - Con: State duplication
   - Con: Tool registry duplicated

3. **No scheduler**
   - Pro: Simplest
   - Con: Tools can't find handlers
   - Con: No session isolation

**Status**: Accepted

---

### ADR-005: Persistence Format

**Context**: Need to persist incomplete async tool calls

**Decision**: JSONB in PostgreSQL

**Rationale**:
- ✅ Flexible schema
- ✅ Query-able if needed
- ✅ Easy migration path
- ✅ Existing DB infrastructure
- ❌ Con: Postgres-specific

**Alternatives Considered**:
1. **Binary format (bincode)**
   - Pro: Smaller, faster
   - Con: Not human-readable
   - Con: Version compatibility issues

2. **Separate KV store (Redis)**
   - Pro: Faster reads
   - Con: Extra infrastructure
   - Con: Persistence concerns

**Status**: Accepted

---

## Appendix C: FAQ

**Q: Why not just keep the dual paths?**

A: Complexity compounds over time. Every new feature needs two implementations. Testing is 2x harder. Reasoning about execution flow requires understanding both paths. Onboarding new developers takes longer.

**Q: What about MCP tools?**

A: They continue to work via Rig's tool registry. No changes needed. They're not scheduler-aware, so they bypass the session handler system (which is fine for simple tools).

**Q: Can we mix old and new tools during migration?**

A: Yes! During migration phases, old `rig::Tool` impls work alongside new `AomiTool` impls. The auto-impl doesn't conflict with manual impls. Feature flags control the rollout.

**Q: What if a tool takes hours to run?**

A: Perfect use case for the new architecture! The session handler keeps the async call alive. When user logs off, we persist the incomplete call to DB. When they log back in, we restore and resume execution.

**Q: How do we test this thoroughly?**

A: Multi-layered approach:
- Unit tests per component
- Integration tests per phase
- End-to-end scenarios
- Performance benchmarks
- Regression suite (old vs new)
- Chaos testing (kill sessions, network failures)

**Q: Performance impact?**

A: Should be neutral or slightly faster:
- Remove: Event loop overhead
- Remove: Channel round-trip
- Add: Session handler lookup (trivial)
- Add: Spawn overhead (amortized)
Net: Faster for sync, same for async

**Q: What about backwards compatibility?**

A: Full compatibility during migration:
- Old tools: Keep working
- New tools: Work alongside
- Gradual rollout: Phase by phase
- Rollback: Available at each phase

**Q: Security implications?**

A: Session isolation improves security:
- Alice's tools can't affect Bob's
- Incomplete calls scoped to user
- Persistence includes user_id check
- No cross-session data leaks

**Q: What if we need to add a third execution mode?**

A: Easy extension point:
- Add new variant to `support_async()`
- Or add new method to `AomiTool`
- Auto-impl handles dispatch
- All tools inherit new behavior

**Q: How do we monitor this in production?**

A: Add metrics for:
- Tool call latency (p50, p95, p99)
- Session handler count
- Incomplete calls per session
- Persistence success rate
- Restore success rate
- Memory usage per session
- Error rates by tool

---

**End of Document**
