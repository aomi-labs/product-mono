# System Bus Plan

> The System serves as an **async worker** for both Human (UI) and AI (LLM). A single bidirectional event queue enables decoupled, event-driven communication across all actors.

---

## Vision

```
┌─────────┐                                           ┌─────────┐
│   LLM   │                                           │   UI    │
│ (Agent) │                                           │ (Human) │
└────┬────┘                                           └────┬────┘
     │                                                     │
     │  push(Request)                       push(Request)  │
     │  pull(Response)                     pull(Response)  │
     ▼                                                     ▼
┌──────────────────────────────────────────────────────────────┐
│                    SystemEventQueue                          │
│              (bidirectional, unbiased buffer)                │
│                                                              │
│   Both sides push requests, System processes async,          │
│   responses flow back through the same queue.                │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │     System      │
                    │  (async worker) │
                    │                 │
                    │  - Wallet ops   │
                    │  - ABI fetch    │
                    │  - RPC calls    │
                    │  - Forge exec   │
                    └─────────────────┘
```

---

## Tool Calling: The Normal Path

**Normal tool calls stay on the `ChatCommand` channel** — this is the synchronous UI ↔ LLM conversation path.

```
LLM calls tool (e.g., get_block_number)
    ↓
ToolScheduler executes synchronously
    ↓
ChatCommand::ToolCall { result } sent to UI
    ↓
Result appended to agent history
    ↓
LLM continues with tool result
```

This path is **unchanged**. Tools that return a single result immediately use `ChatCommand`.

---

## Long-Running Tools: Async Notifications

Some tools are **long-running** and produce multiple results over time:
- 1 **immediate response** (tool returns "started" or initial data)
- N **async notifications** (streamed via `SystemEvent`)

```
LLM calls tool (e.g., execute_forge_script)
    ↓
Tool returns immediate response: "Script execution started, task_id: abc123"
    ↓
ChatCommand::ToolCall { result: "started" } → agent history
    ↓
LLM can continue or wait
    ↓
[async] System pushes ToolProgress { task_id, stage: "compiling" }
[async] System pushes ToolProgress { task_id, stage: "simulating" }
[async] System pushes ToolProgress { task_id, stage: "broadcasting" }
[async] System pushes ToolResult { task_id, result, error }
    ↓
SessionState drains, injects [[SYSTEM:...]] into agent history
    ↓
UI shows progress updates
    ↓
LLM sees final result on next completion tick
```

**Examples**:
- `execute_forge_script` — compile → simulate → broadcast → confirm
- `batch_transfer` — multiple txs with individual status
- `fetch_all_abis` — fetching ABIs for multiple contracts
- `run_simulation` — long simulation with progress updates

---

### Forge Tools (Planned): Async Updates via SystemEvent::AsyncUpdate

The Forge CLI needs `set_execution_plan` and `next_groups` to execute without blocking chat.
We will treat those two tools as **async-only**, returning an ACK immediately and streaming
their final results via `SystemEvent::AsyncUpdate`.

**Scope**:
- Only `set_execution_plan` and `next_groups`
- Other tools remain synchronous via `ChatCommand`

**Async update payload** (wrapped in `SystemEvent::AsyncUpdate(Value)`):
```json
{
  "type": "tool_async_result",
  "tool_name": "next_groups",
  "call_id": "...",
  "result": { "ok": true, "value": { /* tool result */ } },
  "llm_notify": true
}
```

**SystemEvent enum in code**:
```rust
pub enum SystemEvent {
    InlineDisplay(Value),
    SystemNotice(String),
    SystemError(String),
    AsyncUpdate(Value),
}
```

**Routing rule**:
- `llm_notify: true` signals `handle_system_event` to relay a `[[SYSTEM:...]]` message to the LLM.
- UI still receives the event via `system_events`.

---

## The Four Cases

The SystemEventQueue is **bidirectional and unbiased** — both LLM and UI can push requests and receive responses. System acts as the shared async worker.

### Case 1: LLM → System → LLM (Async Tool Results)
Long-running tool produces async results that need to reach the agent.

```
LLM calls long-running tool
    ↓
Tool returns immediate: "started, task_id: abc123"
    ↓
[async] System pushes ToolProgress { task_id, ... }  (optional, UI-only)
[async] System pushes ToolResult { task_id, result }
    ↓
SessionState drains, injects [[SYSTEM:...]] into agent history
    ↓
LLM sees result on next completion tick
```

**Examples**: Forge script execution, batch operations, long simulations

### Case 2: LLM → System → UI
Agent requests something that requires human action, UI receives and acts.

```
LLM calls tool (e.g., send_transaction_to_wallet)
    ↓
System pushes WalletTxRequest { payload }
    ↓
SessionState drains, sets pending_wallet_tx flag
    ↓
UI receives via SSE (get_state returns system_events)
    ↓
UI renders wallet approval modal
```

**Examples**: Wallet transaction approval, user confirmation dialogs, file download prompts

### Case 3: UI → System → LLM
Human provides input that needs to reach the agent (not via chat).

```
User signs/rejects transaction in wallet UI
    ↓
UI POST /api/system/event (fire-and-forget)
    ↓
System pushes WalletTxResponse { status, tx_hash, detail }
    ↓
SessionState drains, clears pending_wallet_tx
    ↓
SessionState injects [[SYSTEM: tx confirmed 0x...]] into agent history
    ↓
LLM sees wallet result on next completion tick
```

**Examples**: Wallet tx result, user-provided parameters, external data paste

### Case 4: UI → System → UI
Human requests something from System, response goes back to UI only (LLM not involved).

```
User clicks "Refresh gas prices" in UI
    ↓
UI POST /api/system/event (fire-and-forget)
    ↓
System pushes UserRequest { kind: "gas_price", payload }
    ↓
System worker processes async (fetch from RPC)
    ↓
System pushes UserResponse { kind: "gas_price", payload }
    ↓
UI receives via SSE, updates gas display
```

**Examples**: Gas price refresh, balance check, tx status polling, ABI decode preview

---

## Event Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemEvent {
    // ─── System Notices ───
    SystemNotice { message: String },
    SystemError { message: String },

    // ─── Connection Status ───
    BackendConnecting { message: String },
    BackendConnected,
    MissingApiKey,

    // ─── Wallet Flow (LLM ↔ UI via System) ───
    WalletTxRequest { payload: Value },
    WalletTxResponse {
        status: String,           // "confirmed" | "rejected" | "failed"
        tx_hash: Option<String>,
        detail: Option<String>,
    },

    // ─── Long-Running Tool Notifications ───
    // Tool returns immediate result via ChatCommand, then pushes async updates here
    ToolProgress {
        task_id: String,          // correlates to tool's immediate response
        tool_name: String,        // "execute_forge_script", "batch_transfer", etc.
        stage: String,            // "compiling", "simulating", "broadcasting", etc.
        progress: Option<f32>,    // 0.0 - 1.0, optional
        message: Option<String>,  // human-readable status
    },
    ToolResult {
        task_id: String,
        tool_name: String,
        result: Value,            // final result payload
        error: Option<String>,    // if failed
    },

    // ─── User ↔ System (UI-initiated, no LLM involvement) ───
    UserRequest {
        request_id: Option<String>,  // optional for fire-and-forget
        kind: String,
        payload: Value,
    },
    UserResponse {
        request_id: Option<String>,
        kind: String,
        payload: Value,
        error: Option<String>,
    },
}
```

### Event Routing Matrix

| Event Type | Source | Destination | LLM Injection | UI Visible |
|------------|--------|-------------|---------------|------------|
| `WalletTxRequest` | LLM (tool) | UI | No | Yes (pending_wallet_tx) |
| `WalletTxResponse` | UI | LLM | Yes | Yes (clear pending) |
| `ToolProgress` | System | UI | No | Yes (progress indicator) |
| `ToolResult` | System | LLM + UI | Yes | Yes |
| `UserRequest` | UI | System | No | No |
| `UserResponse` | System | UI | No | Yes |
| `SystemNotice` | System | UI | No | Yes |
| `SystemError` | System | Both | Optional | Yes |

### Tool Call Flow Comparison

| Tool Type | Immediate Response | Async Updates | Example |
|-----------|-------------------|---------------|---------|
| **Sync** | `ChatCommand::ToolCall` | None | `get_block_number` |
| **Long-running** | `ChatCommand::ToolCall` (started) | `ToolProgress*` + `ToolResult` | `execute_forge_script` |
| **Human-gated** | None (waits for human) | `WalletTxRequest` → `WalletTxResponse` | `send_transaction_to_wallet` |

---

## Transport Layer

### SSE: System → UI (server push)

Existing `/api/chat/stream` already returns `SessionResponse` every 100ms. The `system_events` field carries all events destined for UI.

```rust
pub struct SessionResponse {
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub pending_wallet_tx: Option<String>,
    pub system_events: Vec<SystemEvent>,  // ← UI drains this
}
```

**UI consumption**:
- SSE delivers `system_events` array
- UI processes each event by type
- Events are drained (not re-sent on next tick)

### POST: UI → System (fire-and-forget)

New endpoint accepts any `SystemEvent` from UI, pushes to queue, returns immediately.

```
POST /api/system/event
Content-Type: application/json

{
  "session_id": "abc123",
  "event": {
    "type": "WalletTxResponse",
    "status": "confirmed",
    "tx_hash": "0x...",
    "detail": null
  }
}

Response: 202 Accepted
{ "queued": true }
```

**Characteristics**:
- Fire-and-forget: UI doesn't wait for processing
- Immediate 202 response
- Event queued for next `update_state()` drain cycle
- Idempotent where possible (request_id for dedup)

---

## SessionState Processing

### Queue Architecture

```rust
pub struct SessionState {
    // Chat stream (LLM ↔ UI conversation)
    receiver_from_llm: mpsc::Receiver<ChatCommand>,
    sender_to_llm: mpsc::Sender<String>,

    // System event queue (bidirectional, shared)
    system_event_queue: SystemEventQueue,
    processed_system_event_idx: usize,

    // Drained events for UI response
    system_events: Vec<SystemEvent>,

    // State flags
    pending_wallet_tx: Option<String>,
    // ... other flags
}
```

### update_state() Flow

```rust
pub async fn update_state(&mut self) {
    // 1. Process ChatCommand stream (LLM conversation)
    while let Ok(msg) = self.receiver_from_llm.try_recv() {
        match msg {
            ChatCommand::StreamingText(text) => { /* ... */ }
            ChatCommand::ToolCall { .. } => { /* ... */ }
            ChatCommand::Complete => { /* ... */ }
            ChatCommand::Error(e) => { /* ... */ }
        }
    }

    // 2. Drain system events (bidirectional queue)
    let new_events = self.system_event_queue.slice_from(self.processed_system_event_idx);
    self.processed_system_event_idx += new_events.len();

    for event in new_events {
        self.process_system_event(event.clone()).await;
        self.system_events.push(event);  // for UI via get_state()
    }
}

async fn process_system_event(&mut self, event: SystemEvent) {
    match event {
        // ─── Wallet: LLM → UI ───
        SystemEvent::WalletTxRequest { payload } => {
            let wrapped = json!({ "wallet_transaction_request": payload });
            self.pending_wallet_tx = Some(wrapped.to_string());
            // UI will see this in system_events + pending_wallet_tx flag
        }

        // ─── Wallet: UI → LLM ───
        SystemEvent::WalletTxResponse { status, tx_hash, detail } => {
            self.pending_wallet_tx = None;
            let msg = format_wallet_response(&status, &tx_hash, &detail);
            self.inject_to_llm(&msg).await;
        }

        // ─── Long-running tool: Progress (UI-only) ───
        SystemEvent::ToolProgress { task_id, tool_name, stage, .. } => {
            // UI-only; shows progress indicator
            // No LLM injection for progress updates
        }

        // ─── Long-running tool: Final result (LLM + UI) ───
        SystemEvent::ToolResult { task_id, tool_name, result, error } => {
            // Inject final result into LLM history
            let msg = match error {
                Some(e) => format!("Tool {} (task {}) failed: {}", tool_name, task_id, e),
                None => format!("Tool {} (task {}) completed: {}", tool_name, task_id, result),
            };
            self.inject_to_llm(&msg).await;
        }

        // ─── User request/response: UI ↔ System only ───
        SystemEvent::UserRequest { .. } | SystemEvent::UserResponse { .. } => {
            // No processing needed; UI will see it in system_events
            // System worker handles UserRequest separately
        }

        // ─── Connection status ───
        SystemEvent::BackendConnected => {
            if !self.has_sent_welcome {
                self.add_assistant_message(ASSISTANT_WELCOME);
                self.has_sent_welcome = true;
            }
        }

        // ─── Notices/Errors ───
        SystemEvent::SystemNotice { .. } | SystemEvent::SystemError { .. } => {
            // UI-only; no LLM injection for generic notices
        }

        _ => {}
    }
}

async fn inject_to_llm(&mut self, message: &str) {
    let wrapped = format!("[[SYSTEM: {}]]", message);
    let _ = self.sender_to_llm.send(wrapped).await;
}
```

---

## API Endpoints

### Existing Endpoints (unchanged)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/chat` | POST | User message → LLM |
| `/api/chat/stream` | GET | SSE stream (messages + system_events) |
| `/api/state` | GET | Poll current state |
| `/api/interrupt` | POST | Interrupt LLM processing |

### New Endpoint

#### POST `/api/system/event`

Push any SystemEvent into the queue. Fire-and-forget.

**Request**:
```json
{
  "session_id": "abc123",
  "event": { ... }  // SystemEvent JSON
}
```

**Response**: `202 Accepted`
```json
{
  "queued": true,
  "event_type": "WalletTxResponse"
}
```

**Supported event types from UI**:
- `WalletTxResponse` — wallet tx result
- `UserRequest` — request system action
- `UserResponse` — (rare) UI providing data back

**Rejected event types** (server-only):
- `WalletTxRequest` — only LLM can initiate
- `ToolProgress` / `ToolResult` — only tools can push
- `BackendConnected` / `BackendConnecting` — internal

---

## System Worker (Future)

For async processing of `UserRequest`, a background worker drains requests and pushes responses.

Note: Long-running tools push `ToolProgress`/`ToolResult` directly — they don't go through SystemWorker.

```rust
pub struct SystemWorker {
    queue: SystemEventQueue,
    handlers: HashMap<String, Box<dyn RequestHandler>>,
}

impl SystemWorker {
    pub async fn run(&self) {
        loop {
            // Drain UserRequests only
            for event in self.queue.drain_requests() {
                if let SystemEvent::UserRequest { request_id, kind, payload } = event {
                    let handler = self.handlers.get(&kind);
                    let result = handler.handle(payload).await;
                    self.queue.push(SystemEvent::UserResponse {
                        request_id,
                        kind,
                        payload: result.payload,
                        error: result.error,
                    });
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
```

**Handler examples** (for UserRequest):
- `gas_price`: Fetch current gas prices
- `tx_status`: Check transaction confirmation
- `decode_calldata`: Decode tx input data
- `balance_check`: Get token balances

---

## Implementation Plan

### Phase 1: Wire Wallet Flow (Current Sprint)

**Goal**: Complete the wallet request/response cycle using SystemEventQueue.

| Step | Task | Status |
|------|------|--------|
| 1.1 | Add `Deserialize` to `SystemEvent` enum | Pending |
| 1.2 | Create `POST /api/system/event` endpoint | Pending |
| 1.3 | Validate event types (reject server-only events) | Pending |
| 1.4 | Test wallet flow end-to-end | Pending |

**Files to modify**:
- `crates/chat/src/lib.rs` — add Deserialize, serde tag
- `bin/backend/src/endpoint.rs` — new endpoint
- `crates/backend/src/session.rs` — (already handles WalletTxResponse)

### Phase 2: Long-Running Tool Support

**Goal**: Enable tools to push async progress and results via SystemEvent.

| Step | Task |
|------|------|
| 2.1 | Add `task_id` generation utility |
| 2.2 | Add `ToolProgress` / `ToolResult` to SystemEvent enum |
| 2.3 | Update `process_system_event` to inject `ToolResult` to LLM |
| 2.4 | Implement first long-running tool (e.g., `execute_forge_script`) |

### Phase 3: User Request/Response Flow

**Goal**: Enable UI to request async operations from System.

| Step | Task |
|------|------|
| 3.1 | Implement `UserRequest` / `UserResponse` handling |
| 3.2 | Background worker skeleton for user requests |
| 3.3 | First handler: `gas_price` fetch |

### Phase 4: Advanced Features

- Request deduplication (by task_id / request_id)
- Request timeout / cancellation
- Priority queue for urgent events
- Persistence for crash recovery (optional)

---

## Open Questions (Resolved)

| Question | Decision |
|----------|----------|
| SSE vs WebSocket vs POST? | **SSE + fire-and-forget POST** — simple, works everywhere |
| Separate endpoint per event type? | **No** — single `/api/system/event` accepts all types |
| UI poll vs push for system events? | **Push via existing SSE** — already have `system_events` in response |
| Which events inject to LLM? | **`WalletTxResponse`, `ToolResult`** — only final results, not progress |
| Normal tools vs long-running? | **Normal uses `ChatCommand`; long-running returns immediate + pushes `ToolProgress`/`ToolResult`** |

---

## Summary

Two paths for tool execution:

```
┌─────────────────────────────────────────────────────────────────────┐
│                        NORMAL TOOL PATH                             │
│                     (ChatCommand channel)                           │
│                                                                     │
│   LLM ──► ToolCall ──► ToolScheduler ──► Result ──► ChatCommand    │
│                         (sync)           (immediate)                │
│                                                                     │
│   Examples: get_block_number, encode_function_call, get_balance     │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                    LONG-RUNNING TOOL PATH                           │
│              (ChatCommand + SystemEventQueue)                       │
│                                                                     │
│   LLM ──► ToolCall ──► Tool ──► "started" ──► ChatCommand          │
│                          │                                          │
│                          └──► [async] ToolProgress* ──► UI only    │
│                          └──► [async] ToolResult ──► LLM + UI      │
│                                                                     │
│   Examples: execute_forge_script, batch_transfer, run_simulation    │
└─────────────────────────────────────────────────────────────────────┘
```

SystemEventQueue connects LLM, UI, and System for async flows:

```
        ┌─────────────────────────────────────────┐
        │           SystemEventQueue              │
        │                                         │
   LLM ─┼─► WalletTxRequest                       │
        │                   WalletTxResponse ─────┼─► LLM
    UI ─┼─► WalletTxResponse                      │
        │                   WalletTxRequest ──────┼─► UI
        │                                         │
 Tools ─┼─► ToolProgress    (UI only) ────────────┼─► UI
        │   ToolResult      (LLM + UI) ───────────┼─► LLM + UI
        │                                         │
    UI ─┼─► UserRequest     UserResponse ─────────┼─► UI
        │                                         │
        └─────────────────────────────────────────┘
                           │
                           ▼
                    System Workers
                    (async handlers)
```

**Transport**:
- **System → UI**: SSE via existing `/api/chat/stream`
- **UI → System**: POST `/api/system/event` (fire-and-forget)
- **System ↔ LLM**: Internal queue + `[[SYSTEM:...]]` injection
