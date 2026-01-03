# Multi-Step Tool Result Streaming to SystemEventQueue

## Goal
Route multi-step tool results to `SystemEventQueue` so frontend receives async tool progress as system events.

---

## Architecture

```
completion.rs
  --> mid-loop drain when has_pending_streams()
      --> poll_streams_to_next_result() yields ToolCompletion { call_id, tool_name, is_multi_step, result }
      --> if is_multi_step
            yield ChatCommand::AsyncToolResult { call_id, tool_name, result }
      --> always finalize_tool_result for chat history
  --> finalization loop after take_futures()
      --> same handling so late chunks are surfaced
session.rs matches AsyncToolResult
  --> validate with MultiStepResults::from_value before pushing
  --> push SystemEvent::SystemToolDisplay { tool_name, call_id, result }
  --> handle_system_event pushes SystemToolDisplay into active_system_events so UI sees it
```

---

## Implementation Phases (5 steps)

### Phase 1: Add MultiStepResults to AomiApiTool
**File:** `aomi/crates/tools/src/types.rs`

```rust
pub trait AomiApiTool: Send + Sync {
    type ApiRequest: Send + Sync + Clone;
    type ApiResponse: Send + Sync + Clone;
    type MultiStepResults: Send + Sync + Clone + DeserializeOwned + Serialize + 'static;  // NEW
    type Error: std::error::Error + Send + Sync + 'static;
    // ... existing methods ...
}
```

Default for non-multi-step tools: `type MultiStepResults = ();`

> Because stable Rust does not allow default associated types here, set `type MultiStepResults = ();` explicitly in the blanket `AomiApiTool` impl (Rig tools) so existing tools keep compiling.

**Add validation method to AnyApiTool:**
```rust
pub trait AnyApiTool: Send + Sync {
    // ... existing methods ...

    /// Validate and type-check a multi-step result value
    /// Returns Ok(value) if valid, Err if validation fails
    fn validate_multi_step_result(&self, value: &Value) -> EyreResult<Value> {
        Ok(value.clone())  // Default: pass through
    }
}
```

---

### Phase 2: Update Enums in chat/src/lib.rs
**File:** `aomi/crates/chat/src/lib.rs`

**Add SystemToolDisplay variant:**
```rust
pub enum SystemEvent {
    // ... existing variants ...
    SystemToolDisplay {
        tool_name: String,
        call_id: String,
        result: Value,
    },
}
```

**Add AsyncToolResult variant (holds Value, NOT stream):**
```rust
pub enum ChatCommand<S> {
    StreamingText(String),
    ToolCall { topic: String, stream: S },
    AsyncToolResult {           // NEW
        call_id: String,
        tool_name: String,
        result: Value,          // NOT a stream - just the result
    },
    Complete,
    Error(String),
    Interrupted,
}
```

> Also: teach `handle_system_event` in `aomi/crates/backend/src/session.rs` to push `SystemToolDisplay` into `active_system_events` so the UI actually receives it.

---

### Phase 3: Add metadata to ToolResultStream and create ToolCompletion
**File:** `aomi/crates/tools/src/tool_stream.rs`

**Add metadata fields to ToolResultStream struct:**
```rust
#[derive(Default)]
pub struct ToolResultStream {
    inner: Option<StreamInner>,
    pub tool_name: String,      // NEW - metadata for routing
    pub is_multi_step: bool,    // NEW - metadata for routing
}
```

**Add ToolCompletion type for poll return:**
```rust
/// Result from polling a tool stream - includes metadata for routing
#[derive(Debug, Clone)]
pub struct ToolCompletion {
    pub call_id: String,
    pub tool_name: String,
    pub is_multi_step: bool,
    pub result: Result<Value, String>,
}
```

**Stream::Item stays as `(String, Result<Value, String>)`** - no change to consumers like `poll_ui_streams` in session.rs.

**Updates needed:**
- `ToolResultFuture::new_single()` and `new_multi_step()` take `tool_name` and `is_multi_step` params.
- `into_shared_streams()` propagates metadata to resulting `ToolResultStream`.
- `split_first_chunk_and_rest()` passes metadata through.
- `ToolResultStream::from_*` constructors accept metadata params.
- `handler.request()` passes tool_name and is_multi_step when creating futures.
- `take_last_future_as_streams()` and any UI/MCP ack streams must set `tool_name`/`is_multi_step`; avoid `ToolResultStream::default()` without metadata.
- Add a helper to attach metadata when registering externally created streams (if any) before enqueueing.

**Update poll_streams_to_next_result:**
```rust
/// Await the next item from any pending stream. Returns ToolCompletion with metadata.
pub async fn poll_streams_to_next_result(&mut self) -> Option<ToolCompletion> {
    // ... existing poll logic ...
    // When stream yields (call_id, result), wrap with stream's metadata:
    Some(ToolCompletion {
        call_id,
        tool_name: stream.tool_name.clone(),
        is_multi_step: stream.is_multi_step,
        result,
    })
}
```
> When `is_multi_step` is true, look up the tool by name (from the scheduler/handler cache) and run `tool.validate_multi_step_result(&value)` before returning `ToolCompletion`. If validation fails, return an error result so the UI sees the failure instead of dropping it silently.

---

### Phase 4: Update completion.rs finalization loops
**File:** `aomi/crates/chat/src/completion.rs`

**Mid-loop drain (around line 150-155):**
```rust
if handler.has_pending_streams() {
    if let Some(completion) = handler.poll_streams_to_next_result().await {
        let ToolCompletion { call_id, tool_name, is_multi_step, result } = completion;

        if is_multi_step {
            yield Ok(ChatCommand::AsyncToolResult {
                call_id: call_id.clone(),
                tool_name,
                result: result.clone().unwrap_or_else(|e| json!({ "error": e })),
            });
        }
        finalize_tool_result(&mut chat_history, call_id, result);
    }
    continue;
}
```
> Add `use serde_json::json;` near the top for the error helper.

**Final drain (around line 221-224):**
```rust
handler.take_futures();
while let Some(completion) = handler.poll_streams_to_next_result().await {
    let ToolCompletion { call_id, tool_name, is_multi_step, result } = completion;

    if is_multi_step {
        yield Ok(ChatCommand::AsyncToolResult {
            call_id: call_id.clone(),
            tool_name,
            result: result.clone().unwrap_or_else(|e| json!({ "error": e })),
        });
    }
    finalize_tool_result(&mut chat_history, call_id, result);
}
```

> Also update other `ChatCommand` matches (e.g., `aomi/crates/chat/src/app.rs` streaming loop and backend/eval test mocks) to handle or explicitly ignore `AsyncToolResult` so matches stay exhaustive and the variant isn’t dropped.

---

### Phase 5: Match AsyncToolResult in session.rs with validation
**File:** `aomi/crates/backend/src/session.rs`

In the existing `match msg` block around line 251:
```rust
ChatCommand::AsyncToolResult { call_id, tool_name, result } => {
    // Type-check with MultiStepResults::from_value via scheduler
    // (validation happens in scheduler, here we just push)
    self.system_event_queue.push(SystemEvent::SystemToolDisplay {
        tool_name,
        call_id,
        result,
    });
}
```

**Update handle_system_event to surface SystemToolDisplay:**
```rust
async fn handle_system_event(&mut self, event: SystemEvent) {
    match event {
        // ... existing variants ...

        SystemEvent::SystemToolDisplay { .. } => {
            // Inline event - push to active_system_events for immediate UI rendering
            self.active_system_events.push(event);
        }
    }
}
```

---

## Files to Modify

| File | Change |
|------|--------|
| `aomi/crates/tools/src/types.rs` | Add `MultiStepResults` to AomiApiTool, add `validate_multi_step_result` to AnyApiTool |
| `aomi/crates/tools/src/tool_stream.rs` | Add `tool_name`/`is_multi_step` fields to `ToolResultStream`, add `ToolCompletion` type |
| `aomi/crates/tools/src/scheduler.rs` | Pass tool_name/is_multi_step when creating futures, update `poll_streams_to_next_result` to return `ToolCompletion` |
| `aomi/crates/chat/src/lib.rs` | Add `AsyncToolResult` to `ChatCommand`, add `SystemToolDisplay` to `SystemEvent` |
| `aomi/crates/chat/src/completion.rs` | Yield `AsyncToolResult` in both drain points using `ToolCompletion` |
| `aomi/crates/backend/src/session.rs` | Match `AsyncToolResult`, push to queue, handle `SystemToolDisplay` in `handle_system_event` |
| `aomi/crates/chat/src/app.rs` + tests | Make `ChatCommand` matches exhaustive for `AsyncToolResult` (handle or ignore explicitly) |

---

## Implementation Order

1. Phase 1 (types.rs) - add `MultiStepResults` associated type and `validate_multi_step_result` method
2. Phase 2 (lib.rs) - add `AsyncToolResult` and `SystemToolDisplay` enum variants
3. Phase 3 (tool_stream.rs) - add metadata fields to `ToolResultStream`, add `ToolCompletion` type, update constructors
4. Phase 4 (completion.rs) - yield `AsyncToolResult` for multi-step in both drain points
5. Phase 5 (session.rs) - match `AsyncToolResult`, push to queue, render `SystemToolDisplay`

---

## Key Design Decisions

1. **Metadata on struct, not stream item** - `ToolResultStream` holds `tool_name` and `is_multi_step` as fields, `Stream::Item` stays as tuple `(call_id, Result)`. No changes needed in session.rs `poll_ui_streams`.
2. **ToolCompletion for handler return** - `poll_streams_to_next_result` returns `ToolCompletion` struct instead of a 4-tuple, cleaner API.
3. **MultiStepResults type-checked before system event** - `validate_multi_step_result` ensures type safety before pushing to `SystemToolDisplay`.
4. **SystemToolDisplay propagates to UI** - handled inline via `active_system_events`.
5. **AsyncToolResult emitted for every chunk** - mid-loop and final drains both yield, so no chunks are swallowed.
6. **Error payloads stay visible** - serialize errors into the result instead of `Value::Null`.
