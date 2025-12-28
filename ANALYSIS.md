# Tool Calling Analysis

## Scope
This document traces the multi-step tool calling flow function-by-function, identifies potential issues, and proposes detailed solutions.

## Execution Paths (Function-by-Function)

### 1) User input ➜ Chat backend
1. `aomi/crates/backend/src/session.rs::SessionState::process_user_message`
   - Pushes user message and sends it to LLM (`sender_to_llm`).
   - Sets `is_processing` and adds a streaming assistant message.

2. `aomi/crates/backend/src/session.rs::SessionState::update_state`
   - Drains `receiver_from_llm` (commands from `ChatApp`).
   - Routes:
     - `ChatCommand::StreamingText` → append to assistant streaming message.
     - `ChatCommand::ToolCall` → create a tool streaming message and track stream.
     - `ChatCommand::AsyncToolResult` → pushes `SystemEvent::InlineDisplay` with tool display payload.
     - `ChatCommand::Complete` → marks all streaming messages done.
     - `ChatCommand::Error` / `Interrupted` → update state and enqueue system errors.
   - After draining commands:
     - `poll_tool_streams()` polls tool streams and appends chunks to tool-stream messages.
     - `sync_system_events()` reads `SystemEventQueue` and routes to UI/async buckets.

### 2) ChatApp ➜ Streaming completion
3. `aomi/crates/chat/src/app.rs::ChatApp::process_message`
   - Initializes `ToolScheduler` and `ToolApiHandler`.
   - Calls `stream_completion(...)` to get a stream of `ChatCommand`s.
   - Forwards commands to UI and builds the assistant response.

4. `aomi/crates/chat/src/completion.rs::stream_completion`
   - Loops over model streaming responses.
   - Handles tool calls via `process_tool_call(...)`.
   - Yields `ChatCommand::ToolCall { topic, stream }` for UI ACK.
   - After the LLM finishes, it calls:
     - `handler.take_unresolved_calls()`
     - `while let Some(completion) = handler.poll_streams_to_next_result().await { ... }`
       - Emits `ChatCommand::AsyncToolResult` for multi-step chunks.
       - Appends tool results into `chat_history` via `finalize_tool_result(...)`.

### 3) Tool scheduling + stream plumbing
5. `aomi/crates/tools/src/scheduler.rs::ToolScheduler::run`
   - Receives `SchedulerRequest` and dispatches to the appropriate tool.
   - For multi-step tools, `call_with_sender(...)` sends chunks over an mpsc channel.

6. `aomi/crates/tools/src/scheduler.rs::ToolApiHandler::request`
   - Creates `ToolReciever` for oneshot or multi-step (mpsc).
   - Stores it as `unresolved_calls`.

7. `aomi/crates/tools/src/scheduler.rs::ToolApiHandler::take_last_call_as_streams`
   - Converts a `ToolReciever` into two `ToolResultStream`s:
     - UI ACK stream (first chunk)
     - Internal ongoing stream (remaining chunks)

8. `aomi/crates/tools/src/streams.rs::ToolReciever::into_shared_streams`
   - Multi-step path uses `split_first_chunk_and_rest`.
   - First chunk goes to a oneshot (ACK) stream.
   - Remaining chunks go to the fanout mpsc stream.

9. `aomi/crates/tools/src/scheduler.rs::ToolApiHandler::poll_streams_to_next_result`
   - Polls internal ongoing streams and yields the next `ToolCompletion`.

### 4) SystemEvent flow
10. `aomi/crates/chat/src/lib.rs::SystemEventQueue`
    - Thread-safe queue for system events.

11. `aomi/crates/backend/src/session.rs::SessionState::sync_system_events`
    - Pulls events from `SystemEventQueue` and routes by type:
      - `InlineDisplay` / `SystemNotice` / `SystemError` → `active_system_events`
      - `AsyncUpdate` → `pending_async_updates`

## Potential Issues

### A) Multi-step async polling blocks the stream loop
**Where:** `aomi/crates/chat/src/completion.rs::stream_completion` after the LLM finishes.

**Why:**
- `poll_streams_to_next_result().await` waits for tool results in a tight loop.
- This blocks the async stream from yielding other work if a tool is slow.
- For long-running multi-step tools, UI may stall until chunks arrive.

**Impact:**
- Perceived latency, potential dead time in the response stream.
- Inconsistent behavior vs. sync tools (sync tool results finalize immediately, async results can stall).

### B) ACK duplication (resolved by prior PR)
**Where:** `aomi/crates/tools/src/streams.rs::split_first_chunk_and_rest`

**Why:**
- First chunk used to be sent to both the ACK stream and the internal fanout stream.

**Impact:**
- Duplicated first chunk in async processing and/or UI display.

### C) SystemEvent::AsyncUpdate routing happens outside chat crate
**Where:**
- `SystemEvent` is defined in `aomi/crates/chat`.
- `SessionState::handle_system_event` (backend crate) reads and routes events.

**Why it matters:**
- Async update semantics live in chat crate, but the queue is drained by backend.
- This makes it harder to ensure correct semantics are enforced at the source.

**Impact:**
- Tight coupling between backend session and chat events.
- Risk of lost updates if new async types are added without backend updates.

### D) Stream lifecycle inconsistencies between sync/async tools
**Where:**
- Sync tools finalize within the same loop iteration.
- Multi-step tools emit a UI ACK but finalize results later via polling.

**Impact:**
- Divergent behavior makes it harder to reason about system events or history updates.

## Proposed Solution (Detailed)

### 1) Make async tool polling truly background (non-blocking)
**Goal:** Avoid stalling `stream_completion` when waiting for multi-step results.

**Plan:**
- Split the responsibilities:
  1. `stream_completion` should produce the UI ACK immediately.
  2. Background task should poll `handler.poll_streams_to_next_result()` and forward results into an internal channel.

**Implementation Sketch:**
- In `stream_completion`, when tool calls occur:
  - Keep `handler` in an `Arc<Mutex<...>>` or separate it into a poller struct.
  - Spawn a task that loops on `poll_streams_to_next_result()` and pushes `ChatCommand::AsyncToolResult` into a channel.
  - Merge that channel into the main `stream_completion` output stream.

**Expected outcome:**
- UI gets ACK quickly, async results stream independently without blocking.
- Consistent with the intent of multi-step tooling.

### 2) Centralize SystemEvent::AsyncUpdate handling in chat layer
**Goal:** Ensure SystemEvent semantics live where the event type is defined.

**Plan:**
- Add a method in `aomi/crates/chat` that consumes `SystemEventQueue` and splits:
  - Inline events
  - Async updates
- Backend `SessionState` should call this helper instead of hardcoding the routing.

**Expected outcome:**
- More modular event semantics.
- Easier to add new SystemEvent variants without backend edits.

### 3) Normalize sync vs. async tool completion flow
**Goal:** Reduce divergence between sync and async tool paths.

**Plan:**
- Convert sync tools to follow the same stream logic (emit ACK stream and then finalize via the same poller).
- Alternatively, define a unified tool completion handler that always emits results via a single async stream pipeline.

**Expected outcome:**
- Simplified reasoning about chat history updates and UI behavior.
- Uniform handling in tests and in production.

## Suggested Follow-up Checks
- Add regression tests for async polling not blocking streaming text output.
- Verify `SystemEvent::AsyncUpdate` events appear in both CLI and backend flows.
- Validate multi-step tool polling with slow tools (timeout-based tests).
