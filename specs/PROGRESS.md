# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**System Response Refactoring** — improve reliability of session management and title generation.

---

## Branch Status

Current branch: `system-response-redoo` (base: `main`)

**Recent Commits** (last 10):
```
75fb52c update problem
c35b557 change to gen_title naming
573bcfc fix title persistant problem
d49a866 move session histories out out SessionState
528432d delete session
0ade48f refactor endpoint
c3fb37a shorten summary interval
3a81285 HistoryBackend update_session_title
177178d last_summarized_msg
52c4804 updates_endpoint
```

---

## Recently Completed Work

### Session Title System Overhaul
| Change | Description |
|--------|-------------|
| **Placeholder format** | Switched from fragile length-based detection to `#[session_id_prefix]` format |
| **Deduplication** | Change detection before broadcasting title updates |
| **Deprecated endpoint** | `/api/chat/stream` now deprecated |
| **DB persistence** | Titles persist via `update_session_title()` |
| **Empty string handling** | Added `.filter(\|t\| !t.is_empty())` |

### Naming Refactor: Summarization → Generation
```
start_title_summarization_task  →  start_title_generation_task
last_summarized_msg             →  last_gen_title_msg
SummarizeTitleRequest           →  GenerateTitleRequest
SummarizeTitle (BAML)           →  GenerateTitle
```

### Session State Separation
- `SessionState<S>`: chat/stream state (messages, processing, tool streams)
- `SessionData`: metadata (title, archive status, history sessions)

### Stability
- Removed intentional panic in `start_title_generation_task`; failures now log with `tracing::error!`.

---

## Files Modified This Sprint

### Core Session Management
| File | Changes |
|------|---------|
| `crates/backend/src/manager.rs` | Title generation task, deduplication, persistence |
| `crates/backend/src/history.rs` | Fallback title format to `#[id]` |
| `crates/backend/src/session.rs` | Split state from metadata |

### API Layer
| File | Changes |
|------|---------|
| `bin/backend/src/endpoint/sessions.rs` | Empty string filter, `#[id]` format |
| `bin/backend/src/endpoint/mod.rs` | Deprecated `chat_stream` |
| `bin/backend/src/endpoint/types.rs` | `last_summarized_msg` → `last_gen_title_msg` |
| `bin/backend/src/main.rs` | Updated task name |

### BAML Client
| File | Changes |
|------|---------|
| `crates/l2beat/baml_src/generate_conversation_summary.baml` | Renamed function |
| `crates/l2beat/baml_client/src/models/generate_title_request.rs` | Renamed file |
| `crates/l2beat/baml_client/src/apis/default_api.rs` | Renamed function |

---

## Pending Tasks

### Immediate Priority

1. End-to-end title generation test:
   - Create session → verify `#[abc123]` fallback
   - Send messages → verify title generated within 5s
   - Verify DB persistence
   - Verify SSE broadcast

### Short-Term

3. Frontend integration
   - Use `/api/updates` SSE for title changes
   - Handle `#[...]` placeholder titles in UI

4. Rate limiting consideration
   - Current: 5-second interval for all sessions
   - Consider: 30–60s intervals, batch processing, activity-based

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Tests may reference old field names | Open | Some tests use `last_summarized_msg` |
| BAML server required for titles | Working | Default: `http://localhost:2024` |

---

## Multi-Step Flow State

Current Position: Complete ✓

| Step | Description | Status |
|------|-------------|--------|
| 1 | Identify issues in session title system | ✓ Done |
| 2 | Refactor metadata from SessionState to SessionData | ✓ Done |
| 3 | Move API types to bin/backend | ✓ Done |
| 4 | Fix issues #3, #5, #6 | ✓ Done |
| 5 | Fix issues #2, #7, #8, #9, #10 | ✓ Done |
| 6 | Rename summarization → generation | ✓ Done (file name pending) |
| 7 | Add title persistence | ✓ Done |
| 8 | Document changes | ✓ Done |

---

## Notes for Next Agent

### Critical Context

1. **BAML server dependency**
   - Title generation requires BAML server running
   - Default URL: `http://localhost:2024`
   - Configure via `BAML_SERVER_URL` env var

2. **Title format change**
   - Placeholder titles use `#[id]` format, NOT truncated UUID
   - Detection: `title.starts_with("#[")`

3. **Test updates needed**
   - Some tests reference old field names (`last_summarized_msg`)

### Quick Start Commands
```bash
# Start backend (from aomi/)
cargo run --bin backend

# Run tests
cargo test --package aomi-backend

# Check compilation
cargo check
```
