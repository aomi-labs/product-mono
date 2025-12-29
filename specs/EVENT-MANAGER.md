Event Manager Plan (build on current WIP)

- Mental model
  - SystemEvent is generated inside the system only (tool calls, errors, notices, title updates, etc).
  - SystemEventQueue is the single append-only log. It already holds `inner: Arc<Mutex<Vec<SystemEvent>>>`; expand that guarded state to track per-consumer counters: `frontend_event_cnt` and `llm_event_cnt`.
  - Consumers advance their own counters to get exactly-once delivery; the queue remains append-only.

- Scheduler → poller → queue chain
  - Keep the call chain: `ToolScheduler::run` → `run_poll_completion` → `poll_streams` (rename from `poll_streams_to_next_result`).
  - `poll_streams` (non-blocking, single pass):
    - Iterate `ongoing_streams` once, drain ready items, validate multi-step outputs, remove exhausted streams.
    - Append ready items into `completed_calls` under the handler’s mutex; return the batch to the caller.
  - `run_poll_completion` background task (spawned inside `ToolScheduler::run` on the same runtime):
    - Loop: call `poll_streams`, then drain `take_completed_calls()`.
    - For each completion, call `push_async_update` on EventManager/SystemEventQueue.
    - If no work, yield/sleep briefly; respect scheduler shutdown so the task does not leak.
  - ToolApiHandler state: single mutex around `unresolved_calls`, `ongoing_streams`, and `completed_calls`; no locks held across awaits.

- SystemEventQueue API (stateful per session)
  - Fields (all under the existing mutex): `events: Vec<SystemEvent>`, `frontend_event_cnt: usize`, `llm_event_cnt: usize`.
  - Methods:
    - `push(event) -> idx`: append and return index.
    - `advance_frontend_events() -> Vec<SystemEvent>`: clone slice from `frontend_event_cnt` to end; advance `frontend_event_cnt`.
    - `advance_llm_events() -> Vec<SystemEvent>`: same for LLM path; advance `llm_event_cnt`.
  - Naming preference: counters, not “cursors.” If multiple sessions are needed, store per-session counters in SessionState but keep the counter naming.

- EventManager responsibilities
  - Sole writer for system events; wraps SystemEventQueue.
  - Methods: `push_async_update(ToolCompletion)`, plus passthroughs for notices/errors/inline displays as needed.
  - No separate poller inside EventManager; the scheduler poller is the only producer for tool completions, eliminating races on event append.

- SessionState/UI path
  - Hold `frontend_event_cnt` in SessionState.
  - `sync_system_events` calls `advance_frontend_events()`, routes `InlineDisplay/SystemNotice/SystemError/AsyncUpdate` to UI buckets, then the counter is advanced inside the queue method.

- stream_completion (LLM path)
  - Maintain `llm_event_cnt` for the stream instance.
  - Finalization flow:
    - `let sync_completions = handler.take_completed_calls(); finalize_completions(sync_completions);`
    - `let async_events = system_events.advance_llm_events();` for each `AsyncUpdate`, emit `ChatCommand::AsyncToolResult`, call `finalize_tool_result`, and add a concise systems block to the prompt.
  - Systems block format (compact):
    ```
    [[systems]]
    tool_call_id: <call_id>
    tool: <tool_name>
    result: <short JSON or {"error": "..."}>
    [[/systems]]
    ```

- Concurrency
  - SystemEventQueue already uses a mutex; extend it to guard counters and the events vec together.
  - Scheduler poller uses short lock spans; no locks held across awaits.
  - One producer for tool completions (scheduler poller) avoids races with EventManager.

- Cleanup/next steps (when coding)
  - Align naming (`resolve_calls`, `resolve_last_call`, `poll_streams`) with the new chain.
  - Remove unused local completion Vecs once events flow through the queue.
  - Add idle backoff in `run_poll_completion` to prevent spin.
  - Add tests for counter semantics and async completion delivery (UI and LLM each see events exactly once). 
