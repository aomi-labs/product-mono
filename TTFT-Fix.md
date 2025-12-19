## Slow Time To First Token (TTFT) and Deadlock Notes

### Deadlock hazards we identified
- Holding `DashMap` guards across `await` in `SessionManager` (e.g., `set_session_public_key`, `set_session_archived`, existing-session path, and title summarization). Fix: clone the `state` handle, drop the map guard, then `await`.
- Title summarization task held the map guard while awaiting the BAML call. Fix: clone `state`, drop guard, then `await`; do the same when writing back the title.
- General rule: never `await` while a `DashMap` guard or session mutex is held.

### TTFT bottleneck at session creation
- `get_or_create_session` currently blocks on:
  - `history_backend.get_or_create_history` → may hit DB and BAML `summarize_conversation`.
  - `history_backend.get_history_sessions` → DB round-trip.
- These waits happen before `chat_endpoint` can send the user’s first message to the LLM, adding ~seconds to first token.

### Proposed TTFT decoupling
- Return the session immediately on creation; do not await history/summary in-line.
- Spawn a background task to:
  1) Fetch `get_history_sessions` and write into session state when ready.
  2) Fetch `get_or_create_history`/BAML summary, then push the summary to `sender_to_llm` (or add a system/system-like message) once available.
- Add a per-session guard to avoid duplicate background fetch/summarize.
- Outcome: first user message can flow to the LLM immediately; history enrichment arrives asynchronously without gating TTFT.
