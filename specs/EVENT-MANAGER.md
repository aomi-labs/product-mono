Event Manager Plan (updated for sync vs async tool updates)

- Mental model
  - System events are generated internally only (tool completions, errors, notices, title changes).
  - SystemEventQueue is the single append-only log, guarded by one mutex; it also tracks per-consumer counters `frontend_event_cnt` and `llm_event_cnt`.
  - Consumers advance their counters for exactly-once delivery; the event list never shrinks.

- Tool completion → queue
  - `ToolResultStream` splits into `ui_stream` (ACK) and `bg_stream` (ongoing).
  - Streams track `first_chunk_sent`; `is_multi_step()` inspects channel type.
  - `ToolCompletion.sync` marks single-step results and the first chunk of a multi-step; later chunks set `sync=false`.
  - `poll_streams_once`:
    - Single-step: emit `sync=true`, remove stream.
    - Multi-step first chunk: emit `sync=true`, keep stream alive, mark `first_chunk_sent`.
    - Multi-step follow-ups: emit `sync=false`, keep stream until channel closes.
    - Validate multi-step payloads.
  - Per-session poller (spawned with SessionState) locks the handler, calls `poll_streams_once`, drains `take_completed_calls`, and pushes each completion via `push_tool_update` into SystemEventQueue. Idle backoff ~50ms; no extra struct.

- SystemEventQueue API (per session)
  - Fields: `events: Vec<SystemEvent>`, `frontend_event_cnt`, `llm_event_cnt` (all under the existing mutex).
  - Methods:
    - `push(event) -> idx`
    - `advance_frontend_events() -> Vec<SystemEvent>`
    - `advance_llm_events() -> Vec<SystemEvent>`
    - `push_tool_update(ToolCompletion)` -> emits `SyncUpdate` or `AsyncUpdate` based on `completion.sync`.

- EventManager responsibilities
  - Sole writer for system events; wraps SystemEventQueue.
  - Uses `push_tool_update` for tool completions; passthroughs for notices/errors/inline displays.
  - No separate poller inside EventManager; the session poller is the only producer for tool completions.

- SessionState/UI path
  - `sync_system_events` uses `advance_frontend_events()` and routes `InlineDisplay/SystemNotice/SystemError/SyncUpdate/AsyncUpdate` to UI buckets.

- stream_completion (LLM path)
  - Uses shared handler (mutex) only for scheduling; does not poll streams directly.
  - After LLM finishes, loop on `advance_llm_events()`:
    - `SyncUpdate`: emit `AsyncToolResult`, call `finalize_sync_tool`.
    - `AsyncUpdate`: emit `AsyncToolResult`, call `finalize_async_completion` (system hint + result).
    - If no new events but handler still has streams/unresolved calls, sleep briefly and retry; otherwise exit.
  - Optional: append compact `[[systems]]` blocks for tool callbacks if desired.

- Concurrency
  - Single mutex in SystemEventQueue for events + counters.
  - Handler in `Arc<Mutex<...>>`; poller uses short lock spans and backoff.
  - One producer for tool completions avoids races.

- Follow-ups
  - Add poller shutdown tied to session lifetime.
  - Add tests for counter semantics and sync-vs-async delivery (UI/LLM exactly once).
  - Ensure remaining `push_async_update` references are migrated to `push_tool_update`.
