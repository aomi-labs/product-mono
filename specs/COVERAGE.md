# Test Coverage Analysis

## Test Files Overview

| File | Location | Purpose | Requires API Key |
|------|----------|---------|------------------|
| `tools/src/tests/test_scheduler.rs` | `crates/tools/src/tests/test_scheduler.rs` | Scheduler/handler level streaming & mock tool tests | No |
| `tools/src/tests/utils.rs` | `crates/tools/src/tests/utils.rs` | Mock tools & test helpers for scheduler | N/A (helper) |
| `session_tests.rs` | `crates/backend/tests/session_tests.rs` | Session state loop & UI message accumulation | No |
| `utils.rs` | `crates/backend/tests/utils.rs` | Shared mock backends & test helpers | N/A (helper) |
| `completion.rs` (tests) | `crates/chat/src/completion.rs` | End-to-end LLM completion with real tools | **Yes** |

---

## Core Types

### ToolCompletion

```rust
pub struct ToolCompletion {
    pub call_id: String,
    pub tool_name: String,
    pub sync: bool,        // true = single-step OR first chunk; false = follow-up chunks
    pub result: Result<Value, String>,
}
```

### ToolResultStream

```rust
pub struct ToolResultStream {
    inner: Option<StreamInner>,
    pub call_id: String,
    pub tool_name: String,
    pub first_chunk_sent: bool,  // tracks whether first chunk emitted for multi-step
}

impl ToolResultStream {
    pub fn is_multi_step(&self) -> bool;  // inspects StreamInner::Multi vs Single
}
```

### SystemEvent

```rust
pub enum SystemEvent {
    InlineDisplay(Value),    // LLM → UI only
    SystemNotice(String),    // System → UI only
    SystemError(String),     // System → UI & LLM
    SyncUpdate(Value),       // System → UI & LLM (single-step or first chunk)
    AsyncUpdate(Value),      // System → UI & LLM (follow-up chunks)
}
```

### ChatCommand

```rust
pub enum ChatCommand<S> {
    StreamingText(String),
    ToolCall { topic: String, stream: S },
    Complete,
    Error(String),
    Interrupted,
}
```

---

## Mock Types Available

| Mock Type | File | `multi_steps()` | Behavior | Used In |
|-----------|------|-----------------|----------|---------|
| `MockMultiStepTool` | `tests/utils.rs` | `true` | Sends 3 chunks via `call_with_sender`: `{step:1}`, `{step:2}`, `{step:3,finished:true}` with 10ms delays | Multi-step tests |
| `MockSingleTool` | `tests/utils.rs` | `false` | Returns `{result:"single"}` via `call_with_json` (oneshot) | Single-step tests |
| `MockSlowSingleTool` | `tests/utils.rs` | `false` | 50ms delay, returns `{result:"slow"}` via `call_with_json` | Timing tests |
| `MockMultiStepErrorTool` | `tests/utils.rs` | `true` | Sends `{step:1}` then `Err("step 2 failed")` via sender | Error handling tests |
| `MockBackend` | `backend/tests/utils.rs` | N/A | Scripted interactions with expected input/output | Session tests |
| `StreamingToolBackend` | `backend/tests/utils.rs` | N/A | Sends `StreamingText`, one `ToolCall`, then `Complete` | Session tests |

---

## Tests by Code Path Coverage

### Scheduler/Handler Level

| Test Name | Code Path Tested | Assertions |
|-----------|------------------|------------|
| `test_typed_scheduler_unknown_tool_and_streaming` | Unknown tool → error via streaming ACK | Stream yields error containing "Unknown tool" |
| `test_multi_step_tool_first_chunk` | Multi-step tool → first chunk via UI stream | First chunk `{step:1}` arrives within timeout |
| `test_single_tool_uses_oneshot` | Single tool → oneshot result via stream | `{result:"single"}`, then stream exhausted |
| `test_single_tool_waits_for_completion` | Single tool (slow) → no premature yield | Stream pending at 20ms, yields after |
| `test_multi_step_flag_detection` | `handler.is_multi_step()` metadata cache | Correct `true`/`false` for registered tools |
| `test_multi_step_tool_streams_all_chunks_and_errors` | Full multi-step lifecycle | All chunks emitted, error handled |

### Session/Backend Level

| Test Name | Code Path Tested | Assertions |
|-----------|------------------|------------|
| `streaming_tool_content_is_accumulated` | `ChatCommand::ToolCall` → session message accumulation | Tool message contains accumulated content |

### Format Helpers

| Test Name | Code Path Tested | Assertions |
|-----------|------------------|------------|
| `test_format_tool_name_snake_case` | Snake case → human readable | `"encode_function_call"` → `"Encode function call"` |
| `test_format_tool_name_non_snake_case` | CamelCase → human readable | `"MyTool"` → `"My tool"` |

### Completion Tests (API Key Required)

| Test Name | Code Path Tested | Assertions |
|-----------|------------------|------------|
| `test_scheduler_setup` | Tool registration in scheduler | Time, ABI, wallet tools registered |
| `test_basic_tool_call` | Real LLM + `get_current_time` | Response non-empty |
| `test_multi_round_conversation` | Real LLM with history context | Response with history non-empty |
| `test_multiple_tool_calls` | Real LLM + multiple tools | Response mentions time and ABI |
| `test_error_handling` | Nonexistent tool request | Meaningful error response |

---

## ChatCommand Variants Coverage

| Variant | Where Generated | Test Coverage |
|---------|-----------------|---------------|
| `StreamingText(String)` | `completion.rs` | `session_tests.rs` via mocks |
| `ToolCall { topic, stream }` | `completion.rs` | `test_scheduler.rs`, `session_tests.rs` |
| `Complete` | `completion.rs`, mocks | `session_tests.rs` |
| `Error(String)` | `completion.rs` | `completion.rs::test_error_handling` (API key) |
| `Interrupted` | `session.rs` | **NOT TESTED** |

---

## SystemEvent Variants Coverage

| Variant | Where Generated | Test Coverage |
|---------|-----------------|---------------|
| `InlineDisplay(Value)` | `completion.rs` (wallet_tx_request) | **NOT TESTED** |
| `SystemNotice(String)` | `connections.rs`, `session.rs` | **NOT TESTED** |
| `SystemError(String)` | `connections.rs`, `completion.rs` | **NOT TESTED** |
| `SyncUpdate(Value)` | `push_tool_update()` when `sync=true` | **NOT TESTED** |
| `AsyncUpdate(Value)` | `push_tool_update()` when `sync=false` | **NOT TESTED** |

---

## SystemEventQueue Methods Coverage

| Method | Purpose | Test Coverage |
|--------|---------|---------------|
| `push(event)` | Append event, return index | **NOT TESTED** |
| `advance_frontend_events()` | Return all new events, advance counter | **NOT TESTED** |
| `advance_llm_events()` | Return filtered events (Sync/Async/Error), advance counter | **NOT TESTED** |
| `push_tool_update(completion)` | Emit SyncUpdate or AsyncUpdate based on `sync` flag | **NOT TESTED** |
| `has_frontend_events()` | Check for unread frontend events | **NOT TESTED** |
| `has_llm_events()` | Check for unread LLM events | **NOT TESTED** |

---

## ToolApiHandler Methods Coverage

| Method | Purpose | Test Coverage |
|--------|---------|---------------|
| `request()` | Enqueue tool request | Covered (all scheduler tests) |
| `resolve_last_call()` | Convert last unresolved → streams, add bg to ongoing | Covered |
| `resolve_calls()` | Convert all unresolved → streams | Covered |
| `poll_streams_once()` | Non-blocking single-pass poll, drain to completed_calls | **NOT TESTED** |
| `take_completed_calls()` | Drain and return completed_calls | **NOT TESTED** |
| `poll_stream(&call_id)` | Poll specific stream by call_id | Partial |
| `poll_streams()` | Async version (for tests) | Covered |

---

## Gap Analysis — Missing Test Coverage

### High Priority

| Gap | Description | Why Important |
|-----|-------------|---------------|
| **Sync/Async routing** | No test verifies `push_tool_update()` emits `SyncUpdate` vs `AsyncUpdate` based on `sync` flag | Core event routing logic |
| **Counter-based consumption** | No test verifies `advance_*_events()` delivers exactly-once per consumer | Prevents duplicate/missed events |
| **LLM event filtering** | No test verifies `advance_llm_events()` filters out `InlineDisplay`/`SystemNotice` | LLM should only see relevant events |
| **Background poller flow** | No test verifies poller calls `poll_streams_once` → `take_completed_calls` → `push_tool_update` | Core async delivery mechanism |

### Medium Priority

| Gap | Description | Why Important |
|-----|-------------|---------------|
| **`first_chunk_sent` tracking** | No test verifies multi-step streams set `first_chunk_sent` after first chunk | Determines sync vs async for chunks |
| **Finalization loop** | No test verifies `finalization()` consumes events and updates `chat_history` | LLM context update mechanism |
| **Handler mutex sharing** | No test verifies `Arc<Mutex<ToolApiHandler>>` works between LLM and poller | Concurrency correctness |
| **Multi-step stream lifecycle** | Tests check first chunk; no test polls all chunks and verifies stream stays alive | Full lifecycle coverage |

### Low Priority

| Gap | Description | Why Important |
|-----|-------------|---------------|
| **`MockMultiStepErrorTool` error path** | Registered but error mid-stream not actively tested | Error resilience |
| **`Interrupted` ChatCommand** | Not tested | Edge case |

---

## Recommended New Tests

### 1. SystemEventQueue Tests (New File: `crates/chat/src/lib.rs` or separate test file)

```rust
#[test]
fn test_push_tool_update_sync() {
    // Given: completion with sync=true
    // When: push_tool_update()
    // Then: SyncUpdate event emitted
}

#[test]
fn test_push_tool_update_async() {
    // Given: completion with sync=false
    // When: push_tool_update()
    // Then: AsyncUpdate event emitted
}

#[test]
fn test_advance_frontend_events_returns_all() {
    // Given: queue with InlineDisplay, SystemNotice, SyncUpdate, AsyncUpdate
    // When: advance_frontend_events()
    // Then: all 4 events returned
}

#[test]
fn test_advance_llm_events_filters() {
    // Given: queue with InlineDisplay, SystemNotice, SyncUpdate, AsyncUpdate, SystemError
    // When: advance_llm_events()
    // Then: only SyncUpdate, AsyncUpdate, SystemError returned
}

#[test]
fn test_counter_exactly_once_delivery() {
    // Given: queue with events, two consumers
    // When: each calls advance_*_events() twice
    // Then: first call returns events, second call returns empty
}
```

### 2. poll_streams_once Tests (Add to `test_scheduler.rs`)

```rust
#[test]
fn test_poll_streams_once_single_step_removes_stream() {
    // Given: single-step tool in ongoing_streams
    // When: poll_streams_once()
    // Then: stream removed, completion in completed_calls with sync=true
}

#[test]
fn test_poll_streams_once_multi_step_first_chunk() {
    // Given: multi-step tool in ongoing_streams, first_chunk_sent=false
    // When: poll_streams_once()
    // Then: stream kept, first_chunk_sent=true, completion with sync=true
}

#[test]
fn test_poll_streams_once_multi_step_followup() {
    // Given: multi-step tool with first_chunk_sent=true
    // When: poll_streams_once()
    // Then: stream kept, completion with sync=false
}

#[test]
fn test_take_completed_calls_drains() {
    // Given: completed_calls has items
    // When: take_completed_calls()
    // Then: returns items, completed_calls now empty
}
```

### 3. Background Poller Integration Test (Add to `session_tests.rs`)

```rust
#[tokio::test]
async fn test_background_poller_pushes_completions_to_queue() {
    // Given: SessionState with ongoing tool streams
    // When: poller runs (wait ~100ms)
    // Then: SystemEventQueue contains SyncUpdate/AsyncUpdate events
}
```

### 4. Finalization Tests (Add to `completion.rs` tests or separate)

```rust
#[tokio::test]
async fn test_finalization_consumes_sync_updates() {
    // Given: queue with SyncUpdate events, expected_sync_calls populated
    // When: finalization() runs
    // Then: chat_history updated with tool results, expected_sync_calls drained
}

#[tokio::test]
async fn test_finalization_consumes_async_updates() {
    // Given: queue with AsyncUpdate events
    // When: finalization() runs
    // Then: chat_history updated with system hint + result
}
```

---

## Helper Functions Available

| Helper | File | Purpose |
|--------|------|---------|
| `register_mock_tools(scheduler)` | `tests/utils.rs` | Registers all mock tools |
| `unique_call_id(prefix)` | `tests/utils.rs` | Generates unique call IDs |
| `request_and_get_stream(handler, tool, payload, id)` | `tests/utils.rs` | Request and get UI stream |
| `flush_state(state)` | `backend/tests/utils.rs` | Pump `update_state()` until done |

### New Helpers Needed

| Helper | Purpose |
|--------|---------|
| `assert_sync_update(event, call_id)` | Verify event is `SyncUpdate` with expected call_id |
| `assert_async_update(event, call_id)` | Verify event is `AsyncUpdate` with expected call_id |
| `wait_for_queue_events(queue, count, timeout)` | Wait for N events to appear |
| `create_test_completion(call_id, sync)` | Factory for test ToolCompletion |

---

## Summary

### Strongest Coverage
- Scheduler/handler streaming mechanics (first chunk ACK, oneshot vs multi-step routing)
- Session message accumulation for tool results
- Tool name formatting helpers

### Weakest Coverage
- SystemEventQueue counter-based consumption
- Sync vs Async event routing (`push_tool_update`)
- Background poller integration
- Finalization loop event consumption
- LLM event filtering

### Priority Order for New Tests
1. `push_tool_update` sync/async routing
2. `advance_*_events` counter semantics
3. `poll_streams_once` completion handling
4. Background poller integration
5. Finalization loop
