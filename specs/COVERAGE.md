# Test Coverage Analysis

## Test Files Overview

| File | Location | Purpose | Requires API Key |
|------|----------|---------|------------------|
| `tools/src/test.rs` | `crates/tools/src/test.rs` | Scheduler/handler level streaming & mock tool tests | No |
| `session_tests.rs` | `crates/backend/tests/session_tests.rs` | Session state loop & UI message accumulation | No |
| `utils.rs` | `crates/backend/tests/utils.rs` | Shared mock backends & test helpers | N/A (helper) |
| `completion.rs` (tests) | `crates/chat/src/completion.rs` | End-to-end LLM completion with real tools | **Yes** |

---

## Mock Types Available

| Mock Type | File | `multi_steps()` | Behavior | Used In |
|-----------|------|-----------------|----------|---------|
| `MockMultiStepTool` | `tools/src/test.rs:10` | `true` | Sends 3 chunks via `call_with_sender`: `{step:1}`, `{step:2}`, `{step:3,finished:true}` with 10ms delays | `test_multi_step_tool_first_chunk`, `test_multi_step_flag_detection` |
| `MockSingleTool` | `tools/src/test.rs:61` | `false` | Returns `{result:"single"}` via `call_with_json` (oneshot) | `test_single_tool_uses_oneshot`, `test_multi_step_flag_detection` |
| `MockSlowSingleTool` | `tools/src/test.rs:89` | `false` | 50ms delay, returns `{result:"slow"}` via `call_with_json` | `test_single_tool_waits_for_completion` |
| `MockMultiStepErrorTool` | `tools/src/test.rs:123` | `true` | Sends `{step:1}` then `Err("step 2 failed")` via sender | (registered but not actively tested) |
| `MockBackend` | `utils.rs:26` / `session_tests.rs:12` | N/A | Scripted interactions with expected input/output, sends `StreamingText`, `ToolCall`, `Complete` | `streaming_tool_content_is_accumulated` (via StreamingToolBackend) |
| `StreamingToolBackend` | `utils.rs:157` / `session_tests.rs:133` | N/A | Sends `StreamingText("Thinking...")`, one `ToolCall` with `from_result()`, then `Complete` | `streaming_tool_content_is_accumulated` |
| `MockInteraction` | `utils.rs:32` | N/A | Data struct for scripted backend responses | `MockBackend` |

---

## Tests by Code Path Coverage

| Test Name | File:Line | Code Path Tested | Assertions |
|-----------|-----------|------------------|------------|
| **Scheduler/Handler Level** ||||
| `test_typed_scheduler_unknown_tool_and_streaming` | `test.rs:227` | Unknown tool → error via streaming ACK | Stream yields error containing "Unknown tool" |
| `test_multi_step_tool_first_chunk` | `test.rs:253` | Multi-step tool → first chunk via UI stream | First chunk `{step:1}` arrives within 5s timeout |
| `test_single_tool_uses_oneshot` | `test.rs:283` | Single tool → oneshot result via stream | `{result:"single"}`, then stream exhausted |
| `test_single_tool_waits_for_completion` | `test.rs:314` | Single tool (slow) → no premature yield | Stream pending at 20ms, yields `{result:"slow"}` after |
| `test_multi_step_flag_detection` | `test.rs:342` | `handler.is_multi_step()` metadata cache | Correct `true`/`false` for registered tools |
| **Session/Backend Level** ||||
| `streaming_tool_content_is_accumulated` | `session_tests.rs:194` | `ChatCommand::ToolCall` → session message accumulation | Tool message contains `"first chunk second chunk"`, `is_streaming=false` |
| `rehydrated_session_keeps_agent_history_in_sync` | `session_tests.rs:177` | (ignored) History restoration | — |
| `multiple_sessions_store_and_retrieve_history_by_public_key` | `session_tests.rs:182` | (ignored) Multi-session persistence | — |
| `public_key_history_rehydrates_new_session_context` | `session_tests.rs:188` | (ignored) History rehydration | — |
| **Format Helpers** ||||
| `test_format_tool_name_snake_case` | `test.rs:203` | Snake case → human readable | `"encode_function_call"` → `"Encode function call"` |
| `test_format_tool_name_non_snake_case` | `test.rs:213` | CamelCase → human readable | `"MyTool"` → `"My tool"` |
| `test_format_tool_name_caching` | `test.rs:220` | Memoization (pointer equality) | Same pointer for repeated calls |
| **Completion Tests (API Key Required)** ||||
| `test_scheduler_setup` | `completion.rs:347` | Tool registration in scheduler | Time, ABI, wallet tools registered |
| `test_basic_tool_call` | `completion.rs:376` | Real LLM + `get_current_time` | Response non-empty, possibly has tool calls |
| `test_multi_round_conversation` | `completion.rs:410` | Real LLM with history context | Response with history non-empty |
| `test_multiple_tool_calls` | `completion.rs:436` | Real LLM + multiple tools | Response mentions time and ABI |
| `test_error_handling` | `completion.rs:479` | Nonexistent tool request | Meaningful error response |

---

## ChatCommand Variants Coverage

| Variant | Where Generated | Test Coverage |
|---------|-----------------|---------------|
| `StreamingText(String)` | `completion.rs:173,176` | `session_tests.rs` via `StreamingToolBackend` |
| `ToolCall { topic, stream }` | `completion.rs:207` | `test.rs` (handler level), `session_tests.rs` (session level) |
| `AsyncToolResult { call_id, tool_name, result }` | `completion.rs:241` | **NOT TESTED** — only emitted for `is_multi_step` completions after `poll_streams_to_next_result()` |
| `Complete` | `session_tests.rs:106,168` (mock) | `session_tests.rs` via mocks |
| `Error(String)` | `completion.rs:53` | `completion.rs::test_error_handling` (API key required) |
| `Interrupted` | (not found in codebase) | **NOT TESTED** |

---

## SystemEvent Variants Coverage

| Variant | Where Generated | Test Coverage |
|---------|-----------------|---------------|
| `SystemBroadcast(String)` | (not found) | **NOT TESTED** |
| `SystemNotice(String)` | `connections.rs:23,41,53,84,89,100`, `app.rs:224` | **NOT TESTED** (integration only) |
| `SystemError(String)` | `connections.rs:28,47,96`, `completion.rs:52`, `app.rs:115,234` | **NOT TESTED** (integration only) |
| `SystemDisplay(Value)` | (not found) | **NOT TESTED** |
| `SystemToolDisplay { tool_name, call_id, result }` | **Should be generated from `AsyncToolResult`** | **NOT TESTED** — this is the gap identified |
| `WalletTxRequest { payload }` | `completion.rs:47` | **NOT TESTED** |
| `WalletTxResponse { ... }` | (not found in generation) | **NOT TESTED** |
| `UserRequest { ... }` | (not found) | **NOT TESTED** |
| `UserResponse { ... }` | (not found) | **NOT TESTED** |

---

## Helper Functions Available

| Helper | File | Purpose | Usage |
|--------|------|---------|-------|
| `register_mock_tools(scheduler)` | `test.rs:164` | Registers all 4 mock tools into scheduler's tool map | All scheduler tests |
| `unique_call_id(prefix)` | `test.rs:180` | Generates unique call IDs with atomic counter | Multi-step/single tests |
| `request_and_get_stream(handler, tool, payload, id)` | `test.rs:186` | Calls `handler.request()`, takes streams, adds internal to handler | All streaming tests |
| `flush_state(state)` | `utils.rs:147` / `session_tests.rs:123` | Pumps `update_state()` up to 8 times until not processing | Session tests |
| `test_message(sender, content)` | `utils.rs:133` | Creates `ChatMessage` for test assertions | (available) |
| `history_snapshot(messages, last_activity)` | `utils.rs:143` | Creates `UserHistory` struct | (available) |
| `MockInteraction::streaming_only(input, reply)` | `utils.rs:40` | Creates text-only interaction | Mock backend tests |
| `MockInteraction::with_tool_call(input, reply, tool, args)` | `utils.rs:49` | Creates interaction with tool call | Mock backend tests |

---

## Gap Analysis — Missing Test Coverage

| Gap | Description | Blocking New Feature? |
|-----|-------------|----------------------|
| **`AsyncToolResult` → `SystemToolDisplay` propagation** | No test verifies that multi-step tool completions emit `SystemToolDisplay` to `SystemEventQueue` | Yes — this is the key gap |
| **Multi-step tool full lifecycle** | Tests only check first chunk; no test polls through all 3 chunks and verifies `ToolCompletion` | Partial |
| **`MockMultiStepErrorTool` actual error handling** | Registered but no test exercises the error path mid-stream | Minor |
| **Session-level multi-step tool** | No session test uses a multi-step mock backend | Yes |
| **`SystemEventQueue` assertions in session tests** | `_system_events` parameter is ignored in mock backends | Yes |

---

## Summary

### Strongest Coverage
- Scheduler/handler streaming mechanics (first chunk ACK, oneshot vs multi-step routing)
- Session message accumulation for single-result tools
- Tool name formatting helpers

### Weakest Coverage
- `AsyncToolResult` emission and handling
- `SystemToolDisplay` propagation to UI
- Full multi-step tool lifecycle (beyond first chunk)
- All `SystemEvent` variants except via real API integration tests

---

## Duplication Analysis

### Exact Duplicates (Should Consolidate)

| Duplicate Code | Location A | Location B | Recommendation |
|----------------|------------|------------|----------------|
| `MockInteraction` struct | `session_tests.rs:20-25` | `utils.rs:32-37` | **Keep in `utils.rs` only**, delete from `session_tests.rs` |
| `MockInteraction::streaming_only()` | `session_tests.rs:29-36` | `utils.rs:40-47` | **Keep in `utils.rs` only** |
| `MockInteraction::with_tool_call()` | `session_tests.rs:38-45` | `utils.rs:49-56` | **Keep in `utils.rs` only** |
| `MockChatBackend` / `MockBackend` | `session_tests.rs:13-16` | `utils.rs:26-29` | **Keep `MockBackend` in `utils.rs`**, rename consistently |
| `MockBackend::new()` | `session_tests.rs:50-55` | `utils.rs:60-65` | **Keep in `utils.rs` only** |
| `MockBackend::history_lengths()` | `session_tests.rs:57-59` | `utils.rs:67-69` | **Keep in `utils.rs` only** |
| `AomiBackend for MockBackend` impl | `session_tests.rs:63-121` | `utils.rs:73-131` | **Keep in `utils.rs` only** (identical line-by-line) |
| `flush_state()` | `session_tests.rs:123-131` | `utils.rs:147-155` | **Keep in `utils.rs` only** |
| `StreamingToolBackend` | `session_tests.rs:134-173` | `utils.rs:158-197` | **Keep in `utils.rs` only** |

### Line-by-Line Comparison

**`session_tests.rs` duplicated lines:** ~160 lines (lines 11-173)
**`utils.rs` total lines:** 198 lines

**Result:** `session_tests.rs` is 85% duplicate code from `utils.rs`

---

## Recommended Test Organization

### 1. Shared Test Infrastructure (`crates/test-utils/` or `crates/backend/tests/utils.rs`)

**Should contain (keep here):**

| Item | Purpose | Current Location |
|------|---------|------------------|
| `MockBackend` | Scripted interaction backend | `utils.rs:26` |
| `MockInteraction` | Interaction data struct | `utils.rs:32` |
| `StreamingToolBackend` | Single-shot streaming tool backend | `utils.rs:158` |
| `flush_state()` | Session state pump helper | `utils.rs:147` |
| `test_message()` | ChatMessage factory | `utils.rs:133` |
| `history_snapshot()` | UserHistory factory | `utils.rs:143` |
| `UserHistory` | History snapshot struct | `utils.rs:12` |

### 2. Unit Tests (Keep Scoped in Their Crates)

| Test | Crate | Why Keep Scoped |
|------|-------|-----------------|
| `test_format_tool_name_*` | `tools` | Tests internal `format_tool_name` function |
| `test_multi_step_flag_detection` | `tools` | Tests `handler.is_multi_step()` internal cache |
| `test_future_error_handling` | `tools` | Tests async combinator behavior |

### 3. Integration Tests (Keep in `backend/tests/`)

| Test | Why Integration |
|------|-----------------|
| `streaming_tool_content_is_accumulated` | Tests `DefaultSessionState` + `ChatCommand` flow |
| `rehydrated_session_*` (ignored) | Tests session + history persistence |

### 4. Handler/Scheduler Tests (Keep in `tools/src/test.rs`)

| Test | Why Here |
|------|----------|
| `test_typed_scheduler_unknown_tool_and_streaming` | Tests `ToolScheduler` + `ToolApiHandler` |
| `test_multi_step_tool_first_chunk` | Tests multi-step stream splitting |
| `test_single_tool_uses_oneshot` | Tests oneshot path |
| `test_single_tool_waits_for_completion` | Tests async timing |

---

## Refactoring Actions

### Immediate (Low Risk)

1. **Delete duplicates from `session_tests.rs`**
   - Remove lines 11-131 (MockChatBackend, MockInteraction, flush_state)
   - Remove lines 133-173 (StreamingToolBackend duplicate)
   - Add `mod utils;` or `use super::utils::*;`

2. **Rename for consistency**
   - `MockChatBackend` → `MockBackend` (already done in utils.rs)

### Future (Medium Risk)

3. **Create `crates/test-utils`** (optional)
   - Move shared mocks to dedicated test utilities crate
   - Allows reuse across `backend`, `chat`, `tools` test suites

4. **Add missing mock variants to `tools/src/test.rs`**
   - `MockMultiStepBackend` - backend that emits `AsyncToolResult`
   - `MockSystemEventBackend` - backend that pushes to `SystemEventQueue`

---

## Test Scope Classification

### Unit Tests (Pure Logic, No I/O)

| Test | What It Tests | Mock Dependencies |
|------|---------------|-------------------|
| `test_format_tool_name_snake_case` | String transformation | None |
| `test_format_tool_name_non_snake_case` | String transformation | None |
| `test_format_tool_name_caching` | Memoization | None |
| `test_multi_step_flag_detection` | HashMap lookup | None |
| `test_future_error_handling` | `TryFutureExt` combinator | None |

### Component Tests (Single Component + Mocks)

| Test | Component Under Test | Mocked |
|------|---------------------|--------|
| `test_typed_scheduler_unknown_tool_and_streaming` | `ToolScheduler` | Tool registry (empty) |
| `test_multi_step_tool_first_chunk` | `ToolApiHandler` | `MockMultiStepTool` |
| `test_single_tool_uses_oneshot` | `ToolApiHandler` | `MockSingleTool` |
| `test_single_tool_waits_for_completion` | `ToolApiHandler` | `MockSlowSingleTool` |

### Integration Tests (Multiple Components)

| Test | Components | Mocked |
|------|------------|--------|
| `streaming_tool_content_is_accumulated` | `DefaultSessionState` + `ChatMessage` + `ToolResultStream` | `StreamingToolBackend` |

### E2E Tests (Real External Services)

| Test | External Dependency | Skip Condition |
|------|---------------------|----------------|
| `test_basic_tool_call` | Anthropic API | `ANTHROPIC_API_KEY` not set |
| `test_multi_round_conversation` | Anthropic API | `ANTHROPIC_API_KEY` not set |
| `test_multiple_tool_calls` | Anthropic API | `ANTHROPIC_API_KEY` not set |
| `test_error_handling` | Anthropic API | `ANTHROPIC_API_KEY` not set |
