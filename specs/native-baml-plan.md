# Native BAML FFI Integration Plan

## Overview

Replace the HTTP-based BAML client (OpenAPI + `baml-cli serve`) with native Rust FFI calls to the BAML runtime.

## Architecture Comparison

### Current Architecture (HTTP-based)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Rust Application                          │
│  ┌─────────────────┐    ┌───────────────────────────────────┐   │
│  │   aomi-baml     │    │  baml_client (generated OpenAPI)  │   │
│  │   (wrapper)     │───▶│  - reqwest HTTP calls             │   │
│  │                 │    │  - serde JSON serialization       │   │
│  └─────────────────┘    └───────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    │ HTTP POST /call/...
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│               baml-cli serve (External Process)                  │
│  Started via: BAML_CLI_BIN serve --from BAML_SRC_DIR --port 2024│
│  - Reads .baml schema files from disk                           │
│  - Exposes REST API endpoints                                   │
│  - Requires process lifecycle management                        │
└─────────────────────────────────────────────────────────────────┘
```

### Target Architecture (Native FFI)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Your Project (aomi)                              │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  baml_client/ (GENERATED - replaces current OpenAPI client)     │    │
│  │  ├── mod.rs           - exports B client, types, Error          │    │
│  │  ├── runtime.rs       - BamlRuntime initialization              │    │
│  │  ├── baml_source_map.rs - embedded .baml files as HashMap       │    │
│  │  ├── types/           - generated structs (BamlEncode/Decode)   │    │
│  │  │   └── classes.rs   - ContractInfo, ExtractedContractInfo...  │    │
│  │  ├── functions/       - async_client.rs, sync_client.rs         │    │
│  │  │   └── B.ExtractContractInfo.call(...), B.GenerateScript...   │    │
│  │  └── stream_types/    - streaming response types                │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                              │                                           │
│                              │ uses (in-process)                         │
│                              ▼                                           │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  baml crate (from /Users/cecilia/Code/baml/languages/rust/baml) │    │
│  │  ├── BamlRuntime::new(dir, files, env) -> runtime handle        │    │
│  │  ├── runtime.call_function_async::<T>(name, args) -> T          │    │
│  │  ├── runtime.call_function_stream_async() -> AsyncStreamingCall │    │
│  │  ├── #[derive(BamlEncode, BamlDecode)] - serialization macros   │    │
│  │  └── FunctionArgs, ClientRegistry, Collector, BamlError         │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                              │                                           │
│                              │ FFI calls (C ABI)                         │
│                              ▼                                           │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  libbaml_cffi.a (prebuilt static library)                       │    │
│  │  Location: baml-ffi-aarch64-apple-darwin/lib/libbaml_cffi.a     │    │
│  │  - create_baml_runtime(), call_function_from_c()                │    │
│  │  - Protobuf encoding for values (CffiValueHolder)               │    │
│  │  - Callback-based async result delivery                         │    │
│  └─────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
```

## Benefits of Native FFI

| Aspect | HTTP Client | Native FFI |
|--------|-------------|------------|
| **Latency** | HTTP overhead per call | Direct function calls |
| **Process mgmt** | Must spawn/monitor `baml-cli serve` | None - in-process |
| **Deployment** | Ship CLI binary + manage ports | Single binary |
| **Error handling** | HTTP status codes + JSON | Native Rust Result<T, E> |
| **Streaming** | Polling/WebSocket | Native async streams |
| **Type safety** | Runtime JSON parsing | Compile-time derive macros |

## File Structure Changes

```
crates/baml/
├── Cargo.toml                    # UPDATE: add baml dependency
├── baml_src/                     # KEEP: .baml schema files
│   ├── clients.baml
│   ├── generators.baml           # UPDATE: change to "rust" output
│   ├── forge_executor.baml
│   ├── analyze_*.baml
│   └── ...
├── baml_client/                  # REPLACE: generated native client
│   ├── Cargo.toml                # REMOVE (becomes module, not crate)
│   ├── mod.rs                    # GENERATED
│   ├── runtime.rs                # GENERATED
│   ├── baml_source_map.rs        # GENERATED (embeds .baml files)
│   ├── types/                    # GENERATED
│   ├── functions/                # GENERATED
│   └── stream_types/             # GENERATED
└── src/
    ├── lib.rs                    # UPDATE: re-export from baml_client
    ├── client.rs                 # SIMPLIFY: thin wrapper or remove
    └── types.rs                  # SIMPLIFY: use generated types
```

---

## Implementation Phases

### Phase 1: Setup Dependencies & Generator Config

**Goal:** Configure the project to use native BAML crate and Rust code generator.

**Tasks:**
1. Add `baml` crate dependency to workspace `Cargo.toml`
2. Update `generators.baml` to output Rust instead of OpenAPI
3. Verify the baml-cli can be invoked

**Files to modify:**
- `aomi/Cargo.toml` (workspace)
- `crates/baml/baml_src/generators.baml`

---

### Phase 2: Generate Native Client

**Goal:** Run `baml-cli generate` to produce the native Rust client code.

**Tasks:**
1. Run baml-cli generate command
2. Verify generated files structure
3. Fix any generation issues

**Command:**
```bash
/Users/cecilia/Code/baml/engine/target/debug/baml-cli generate \
    --from /Users/cecilia/Code/product-mono/aomi/crates/baml/baml_src
```

---

### Phase 3: Integrate Generated Client

**Goal:** Make the generated `baml_client` module compile within the crate.

**Tasks:**
1. Update `crates/baml/Cargo.toml` to depend on `baml` crate
2. Remove old `baml_client` crate dependency from workspace
3. Add `mod baml_client;` to `src/lib.rs`
4. Fix any import/module path issues

**Files to modify:**
- `crates/baml/Cargo.toml`
- `crates/baml/src/lib.rs`
- `aomi/Cargo.toml` (remove old baml-client workspace member)

---

### Phase 4: Update Client Wrapper

**Goal:** Update `src/client.rs` to use the native client instead of HTTP.

**Current API:**
```rust
impl BamlClient {
    pub fn new() -> Result<Self>;
    pub async fn extract_contract_info(&self, ops, contracts) -> Result<Vec<ExtractedContractInfo>>;
    pub async fn generate_script(&self, ops, extracted) -> Result<ScriptBlock>;
}
```

**New implementation:**
```rust
use crate::baml_client::{async_client::B, types};

impl BamlClient {
    pub fn new() -> Result<Self> {
        // No config needed - runtime auto-initializes with embedded .baml files
        Ok(Self {})
    }

    pub async fn extract_contract_info(&self, ops: &[String], contracts: &[ContractSource]) -> Result<Vec<ExtractedContractInfo>> {
        let baml_contracts: Vec<types::ContractInfo> = contracts.iter().map(Into::into).collect();
        B.ExtractContractInfo
            .call(&ops.to_vec(), &baml_contracts)
            .await
            .map_err(|e| anyhow!("ExtractContractInfo failed: {}", e))
    }

    pub async fn generate_script(&self, ops: &[String], extracted: &[ExtractedContractInfo]) -> Result<ScriptBlock> {
        B.GenerateScript
            .call(&ops.to_vec(), extracted)
            .await
            .map_err(|e| anyhow!("GenerateScript failed: {}", e))
    }
}
```

---

### Phase 5: Update Type Re-exports

**Goal:** Update `src/types.rs` to re-export generated types instead of OpenAPI models.

**Current:**
```rust
pub use baml_client::models::{
    CodeLine, ContractInfo, Event, ExtractContractInfoRequest, ...
};
```

**New:**
```rust
pub use crate::baml_client::types::{
    CodeLine, ContractInfo, Event, ExtractedContractInfo, ...
};
```

---

### Phase 6: Update Downstream Consumers

**Goal:** Update any crates that depend on `aomi-baml` to use new types.

**Crates to check:**
- `crates/tools` (forge_executor uses BamlClient)
- `crates/backend` (if it uses BAML directly)
- `crates/l2beat` (if it uses BAML directly)

---

### Phase 7: Cleanup & Testing

**Goal:** Remove old HTTP client code and verify everything works.

**Tasks:**
1. Remove `baml_client/` crate from workspace (old OpenAPI client)
2. Remove `reqwest` dependency if no longer needed
3. Run all tests
4. Update PROGRESS.md

---

## Key API Mapping

| BAML Function | HTTP Endpoint | Native Call |
|---------------|---------------|-------------|
| `ExtractContractInfo` | `POST /call/ExtractContractInfo` | `B.ExtractContractInfo.call(ops, contracts)` |
| `GenerateScript` | `POST /call/GenerateScript` | `B.GenerateScript.call(ops, extracted)` |
| `AnalyzeABI` | `POST /call/AnalyzeABI` | `B.AnalyzeABI.call(request)` |
| `AnalyzeLayout` | `POST /call/AnalyzeLayout` | `B.AnalyzeLayout.call(request)` |
| `GenerateTitle` | `POST /call/GenerateTitle` | `B.GenerateTitle.call(request)` |
| ... | ... | ... |

---

## Environment Variables

The native client reads env vars automatically. Key ones:
- `ANTHROPIC_API_KEY` - for Claude models
- `OPENAI_API_KEY` - for GPT models

No longer needed:
- `BAML_API_URL` - was for HTTP server address
- `BAML_PASSWORD` - was for server auth

---

## Rollback Plan

If issues arise, revert to HTTP client by:
1. Restore `generators.baml` to `output_type "rest/openapi"`
2. Re-run OpenAPI generator
3. Restore old `src/client.rs` implementation

---

## Progress Tracking

- [x] Phase 1: Setup Dependencies & Generator Config
- [x] Phase 2: Generate Native Client (18 files generated)
- [x] Phase 3: Integrate Generated Client
- [x] Phase 4: Update Client Wrapper (`aomi-baml/src/client.rs`)
- [x] Phase 5: Update Type Re-exports (`aomi-baml/src/types.rs`)
- [x] Phase 6: Update Downstream Consumers
  - [x] `aomi-backend/src/background.rs` - migrated
  - [x] `aomi-backend/src/history.rs` - migrated
  - [x] `aomi-l2beat/src/l2b_tools.rs` - migrated
  - [x] `aomi-l2beat/src/adapter.rs` - migrated
  - [x] `aomi-l2beat/src/runner.rs` - migrated
  - [x] `aomi-tools/src/clients.rs` - already using native FFI
  - [x] `aomi-forge` - no BAML usage
- [x] Phase 7: Cleanup & Testing
  - [x] Removed `baml_client_old_openapi/` backup directory
  - [x] Full workspace compiles without errors

## Completed Type Mappings

Old (HTTP) → New (Native):
- `baml_client::models::*` → `aomi_baml::baml_client::types::*`
- `AbiAnalysisResult` → `ABIAnalysisResult` (case change)
- `EventActionHandler` → `Union2AccessControlConfigOrEventHandlerConfig`
- `Configuration` → removed (no config needed)
- `default_api::analyze_abi()` → `B.AnalyzeABI.call()`
- `default_api::analyze_event()` → `B.AnalyzeEvent.call()`
- `default_api::analyze_layout()` → `B.AnalyzeLayout.call()`
- `default_api::analyze_contract_for_handlers()` → `B.AnalyzeContractForHandlers.call()`

## Migration Complete

The native BAML FFI integration is now complete. All downstream consumers have been migrated from HTTP-based BAML calls to native FFI calls using the `B.FunctionName.call()` pattern.
