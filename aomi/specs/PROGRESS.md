# Project Progress: Native BAML FFI Integration

**Branch:** `cecilia/native-baml`
**Last Updated:** 2026-01-04

---

## Sprint Goal

Replace HTTP-based BAML client (OpenAPI + `baml-cli serve`) with native Rust FFI calls to the BAML runtime.

**Status:** ✅ Complete

---

## Architecture Change

### Before (HTTP-based)
```
Rust App → baml_client (OpenAPI) → HTTP POST → baml-cli serve → LLM API
```

### After (Native FFI)
```
Rust App → baml_client (generated) → libbaml_cffi.a → LLM API
```

**Benefits:**
- Eliminated HTTP overhead (~5s latency reduction)
- No external process management (no `baml-cli serve`)
- Single binary deployment
- Native Rust `Result<T, E>` error handling
- Compile-time type safety via `BamlEncode`/`BamlDecode` derive macros

---

## Branch Status

**Current Branch:** `cecilia/native-baml`

**Recent Commits:**
```
3b475e0b   Phase 7 (Cleanup)
8cda37d9  Summary - Native BAML FFI Integration
7c6ab6e1 sync_system_events syncs both advance_frontend_events and advance_llm_events
cc43177d unify take_system_events and take_async_events to advance_frontend_events
71fc43f9 poll_ui_streams
a12049f1 fix async_tool_results_populate_system_events
dae352fc rename functions for better interface
```

---

## Completed Work

### Phase 1: Setup Dependencies & Generator Config
- Added `baml` crate dependency to workspace
- Updated `generators.baml` to output Rust instead of OpenAPI
- Configured path to `libbaml_cffi.a` static library

### Phase 2: Generate Native Client
- Ran `baml-cli generate` producing 18 Rust files
- Files: `mod.rs`, `runtime.rs`, `baml_source_map.rs`, `types/`, `functions/`, `stream_types/`

### Phase 3: Integrate Generated Client
- Updated `crates/baml/Cargo.toml` with `baml` crate dependency
- Added `mod baml_client;` to `src/lib.rs`
- Resolved import/module path issues

### Phase 4: Update Client Wrapper
- Refactored `aomi-baml/src/client.rs` from HTTP to FFI
- Pattern change: `default_api::function(&config, request)` → `B.Function.call(args)`
- Removed `Configuration` struct (no config needed with FFI)

### Phase 5: Update Type Re-exports
- Changed `aomi-baml/src/types.rs` to re-export from `baml_client::types`
- Type mappings:
  - `AbiAnalysisResult` → `ABIAnalysisResult` (case change)
  - `EventActionHandler::*` → `Union2AccessControlConfigOrEventHandlerConfig::*`

### Phase 6: Update Downstream Consumers
All crates migrated to native FFI:

| Crate | File | Status |
|-------|------|--------|
| `aomi-backend` | `src/background.rs` | ✅ Migrated |
| `aomi-backend` | `src/history.rs` | ✅ Migrated |
| `aomi-l2beat` | `src/l2b_tools.rs` | ✅ Migrated |
| `aomi-l2beat` | `src/adapter.rs` | ✅ Migrated |
| `aomi-l2beat` | `src/runner.rs` | ✅ Migrated |
| `aomi-tools` | `src/clients.rs` | ✅ Already using FFI |
| `aomi-forge` | N/A | ✅ No BAML usage |

### Phase 7: Cleanup & Testing
- Removed `baml_client_old_openapi/` backup directory
- Full workspace compiles without errors
- E2E test verified: `test_extract_contract_info` passes with real Anthropic API call

---

## Key Type Mappings

| Old (HTTP) | New (Native) |
|------------|--------------|
| `baml_client::models::*` | `aomi_baml::baml_client::types::*` |
| `AbiAnalysisResult` | `ABIAnalysisResult` |
| `EventActionHandler::EventHandlerConfig(c)` | `Union2AccessControlConfigOrEventHandlerConfig::EventHandlerConfig(c)` |
| `EventActionHandler::AccessControlConfig(c)` | `Union2AccessControlConfigOrEventHandlerConfig::AccessControlConfig(c)` |
| `default_api::analyze_abi(&config, request)` | `B.AnalyzeABI.call(&contract_info, intent)` |
| `default_api::analyze_event(&config, request)` | `B.AnalyzeEvent.call(&contract_info, &abi_result, intent)` |
| `default_api::analyze_layout(&config, request)` | `B.AnalyzeLayout.call(&contract_info, &abi_result, &intent)` |
| `Configuration::new()` | Not needed |

---

## Files Modified

### BAML Core (`crates/baml/`)
- `Cargo.toml` - Added `baml` crate dependency
- `src/lib.rs` - Re-exports from generated client
- `src/client.rs` - Simplified wrapper using FFI
- `src/types.rs` - Type re-exports from `baml_client::types`
- `baml_client/` - Generated native client (18 files)
- `baml_src/generators.baml` - Changed output to Rust

### L2Beat (`crates/l2beat/`)
- `src/adapter.rs` - Updated imports and union type pattern matching
- `src/l2b_tools.rs` - Removed HTTP config, changed to `B.Function.call()`
- `src/runner.rs` - Removed Configuration, added native FFI calls

### Backend (`crates/backend/`)
- `src/background.rs` - Updated to use native client
- `src/history.rs` - Updated to use native client

---

## Test Verification

**E2E Test Command:**
```bash
cargo test -p aomi-baml test_extract_contract_info -- --ignored --nocapture
```

**Result:**
```
test client::tests::test_extract_contract_info ... ok
test result: ok. 1 passed; 0 failed; finished in 5.01s
```

The test successfully:
1. Called `B.ExtractContractInfo.call()` via native FFI
2. Made real API call to Anthropic Claude
3. Received and parsed structured response
4. Verified type deserialization

---

## Environment Variables

**Required:**
- `ANTHROPIC_API_KEY` - For Claude models
- `OPENAI_API_KEY` - For GPT models (if used)

**No Longer Needed:**
- `BAML_API_URL` - Was for HTTP server address
- `BAML_PASSWORD` - Was for server auth

---

## Notes for Next Agent

### Critical Context

1. **Static Library Location:** `libbaml_cffi.a` at `/Users/cecilia/Code/baml/languages/rust/baml-ffi-aarch64-apple-darwin/lib/`

2. **Code Generation:** Run from `crates/baml/baml_src`:
   ```bash
   /Users/cecilia/Code/baml/engine/target/debug/baml-cli generate --from .
   ```

3. **Union Types:** BAML generates union types as `Union2TypeAOrTypeB`. Pattern match on variants.

4. **No Configuration:** Native client auto-initializes from embedded `.baml` files in `baml_source_map.rs`.

5. **Async Client Pattern:** All calls use `B.FunctionName.call(args).await`

### Common Gotchas

1. **Type Case Sensitivity:** BAML may generate different casing (e.g., `ABIAnalysisResult` vs `AbiAnalysisResult`)

2. **Union Pattern Matching:** Use full path `Union2...::Variant(inner)` in match arms

3. **Serialization:** Some BAML types don't implement `Serialize`. Use counts/summaries instead of raw serialization.

4. **Test Skipping:** Tests requiring API keys should check `ANTHROPIC_API_KEY` and skip gracefully.

### Quick Commands

```bash
# Build workspace
cargo build --workspace

# Run all tests (skips API-dependent ones without key)
cargo test --workspace

# Run e2e BAML test
ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY cargo test -p aomi-baml test_extract_contract_info -- --ignored

# Generate BAML client after schema changes
cd crates/baml/baml_src && /Users/cecilia/Code/baml/engine/target/debug/baml-cli generate --from .
```

---

## Rollback Plan

If issues arise, revert to HTTP client:
1. Restore `generators.baml` to `output_type "rest/openapi"`
2. Re-run OpenAPI generator
3. Restore old `src/client.rs` HTTP implementation
4. Revert downstream consumer imports

---

## Design Decisions

### Why Native FFI over HTTP?

| Aspect | HTTP Client | Native FFI |
|--------|-------------|------------|
| Latency | HTTP overhead per call | Direct function calls |
| Process mgmt | Must spawn/monitor `baml-cli serve` | None - in-process |
| Deployment | Ship CLI binary + manage ports | Single binary |
| Error handling | HTTP status codes + JSON | Native Rust `Result<T, E>` |
| Type safety | Runtime JSON parsing | Compile-time derive macros |

**Decision:** Native FFI eliminates operational complexity and improves performance.
