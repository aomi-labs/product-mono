# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Title Generation System Enhancement** — Add user title protection, anonymous session handling, and comprehensive integration testing.

---

## Branch Status

Current branch: `system-response-redoo-kev` (base: `main`)

**Recent Commits** (last 10):
```
bda26d2 added integration test for title generation
60e75ac cleanup md
39cb190 cleanup-md
4df9c15 .md udpates
5996cd4 rename generate summary
546180c claud commands
223de24 add specs and claude commands
75fb52c update problem
c35b557 change to gen_title naming
573bcfc fix title persistant problem
```

---

## Recently Completed Work

### User Title Protection System
| Change | Description |
|--------|-------------|
| **`is_user_title` flag** | Added to `SessionData` and `SessionMetadata` to distinguish user vs auto-generated titles |
| **Title generation filter** | Skip sessions where `is_user_title = true` in periodic task (manager.rs:395) |
| **Race condition protection** | Double-check `is_user_title` before applying auto-generated title (manager.rs:465-471) |
| **Session creation logic** | Detect user vs placeholder titles via `!title.starts_with("#[")` (manager.rs:334-340) |
| **Rename endpoint** | Sets `is_user_title = true` when user manually renames session (manager.rs:174) |

### Anonymous Session Privacy
| Change | Description |
|--------|-------------|
| **DB persistence guard** | Title generation skips DB writes for sessions without pubkey (manager.rs:457-468) |
| **Privacy preservation** | Anonymous sessions get titles in-memory only, never persisted |

### Integration Test Suite
Created comprehensive E2E test at `crates/backend/tests/title_generation_integration_test.rs`:
- **Real dependencies**: Uses PostgreSQL database and BAML server (localhost:2024)
- **Test coverage**: 4 scenarios covering pubkey sessions, anonymous sessions, user title protection, and re-generation
- **Verification**: DB persistence, broadcasts, metadata flags, title updates

### Code Quality
| Change | Description |
|--------|-------------|
| **Clippy fixes** | Fixed 4 clippy warnings in non-generated code |
| **BAML client** | Added `#![allow(clippy::needless_return)]` for auto-generated code |
| **API types** | Added `#[allow(clippy::too_many_arguments)]` for `FullSessionState::from_chat_state` |

---

## Files Modified This Sprint

### Core Session Management
| File | Key Changes |
|------|-------------|
| `crates/backend/src/manager.rs` | Added `is_user_title` field, filter logic, race condition check, anonymous session guards |
| `crates/backend/tests/title_generation_integration_test.rs` | **NEW**: 340+ line E2E test with 4 scenarios |

### API Layer
| File | Key Changes |
|------|-------------|
| `bin/backend/src/endpoint/types.rs` | Added `is_user_title` to `FullSessionState`, clippy allow |
| `bin/backend/src/endpoint/sessions.rs` | Pass `is_user_title` from metadata to response |
| `bin/backend/src/endpoint/db.rs` | Clippy fix: `is_err()` pattern |
| `bin/backend/src/endpoint/system.rs` | Clippy fix: redundant closure |

### Code Quality
| File | Key Changes |
|------|-------------|
| `crates/backend/tests/history_tests.rs` | Clippy fix: vec! to array |
| `crates/l2beat/baml_client/src/lib.rs` | Added clippy allow for generated code |

---

## Pending Tasks

### Immediate Priority

1. **Frontend integration** (remaining from sprint)
   - Update frontend to listen to `/api/updates` SSE endpoint
   - Handle `SystemUpdate::TitleChanged` events
   - Update UI when titles change
   - Display `#[...]` placeholder titles appropriately

### Short-Term

2. **Rate limiting consideration**
   - Current: 5-second interval for all sessions
   - Consider: 30–60s intervals, batch processing, activity-based triggers

3. **Test coverage expansion**
   - Unit tests for `is_user_title` flag edge cases
   - Test session deletion with user titles
   - Test concurrent rename scenarios

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Frontend not listening to `/api/updates` | Open | Backend sends `TitleChanged` events but frontend uses deprecated `/api/chat/stream` |
| BAML server required for titles | Working | Default: `http://localhost:2024`, configure via env |

---

## Multi-Step Flow State

Current Position: Backend Complete, Frontend Pending

| Step | Description | Status |
|------|-------------|--------|
| 1 | Add `is_user_title` flag to SessionData | ✓ Done |
| 2 | Update session creation to detect user titles | ✓ Done |
| 3 | Update rename endpoint to set flag | ✓ Done |
| 4 | Update title generation task to respect flag | ✓ Done |
| 5 | Add anonymous session persistence guards | ✓ Done |
| 6 | Create integration test suite | ✓ Done |
| 7 | Fix clippy warnings | ✓ Done |
| 8 | Update frontend to use `/api/updates` SSE | ⏳ Pending |

---

## Test Results

### Integration Test: `test_title_generation_with_baml`
**Location**: `crates/backend/tests/title_generation_integration_test.rs:117`

**Run command**:
```bash
cargo test --package aomi-backend test_title_generation_with_baml -- --ignored --nocapture
```

**Latest Results** (all passed):
- ✅ Test 1: Title generated for pubkey session in 6.5s ("Getting Started")
- ✅ Test 2: Anonymous session title NOT persisted to DB
- ✅ Test 3: User title "My Custom Trading Strategy" protected from auto-generation
- ✅ Test 4: Title re-generated as conversation grew ("Getting Started" → "Blockchain Discussion")

**Prerequisites**:
- PostgreSQL running at `postgresql://aomi@localhost:5432/chatbot`
- BAML server running at `http://localhost:2024`

---

## Notes for Next Agent

### Critical Context

1. **User Title Protection**
   - `is_user_title` flag distinguishes manual vs auto-generated titles
   - Detection: titles starting with `#[` are placeholders, everything else is user-provided
   - Auto-generation NEVER overwrites when `is_user_title = true`

2. **Anonymous Session Privacy**
   - Sessions without `public_key` get titles in-memory only
   - Title generation task checks for pubkey before DB writes
   - This prevents unintentional data collection from anonymous users

3. **Integration Test**
   - Requires real PostgreSQL and BAML server
   - Tests all 4 critical scenarios
   - Use `#[ignore]` attribute, run with `--ignored` flag

4. **Frontend Work Required**
   - Backend sends `SystemUpdate::TitleChanged` via `/api/updates`
   - Frontend needs to subscribe to SSE endpoint
   - Currently frontend uses deprecated `/api/chat/stream`

### Quick Start Commands
```bash
# Run integration test (requires BAML + DB)
cargo test --package aomi-backend test_title_generation_with_baml -- --ignored --nocapture

# Run all tests
cargo test --package aomi-backend

# Check clippy
cargo clippy --all-targets --all-features -- -D warnings

# Start backend (from aomi/)
cargo run --bin backend

# Start BAML server (from aomi/crates/l2beat)
npx @boundaryml/baml serve
```

### Code References

**Key files and line numbers**:
- User title flag: `manager.rs:334-340` (detection), `manager.rs:174` (set on rename)
- Title generation filter: `manager.rs:395` (skip user titles)
- Race condition check: `manager.rs:465-471`
- Anonymous session guard: `manager.rs:457-468`
- Integration test: `title_generation_integration_test.rs:117`
