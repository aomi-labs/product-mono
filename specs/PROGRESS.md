# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Authentication & Authorization** — Implement API key-based authentication with namespace-level authorization, session ID management, and admin CLI for key management.

---

## Branch Status

Current branch: `feat/auth` (base: `main`)

**Recent Commits** (last 10):
```
ecfb14e2 Adjust api-key usage for /api/chat non-default only; add session id in header
462f311b Formatting
0a7b526f Add curl test script
e1650610 Merge remote-tracking branch 'origin/main' into feat/auth
791a44f7 Add shorts for command args
b1fcfb78 Rename chatbot to namespace
aaf346aa Remove ununsed api key adding script
6386b039 Add admin cli for db management
1bbc4afe Merge pull request #111 from aomi-labs/system-event-buff
40a38b5e ci: install Foundry (anvil) and protoc in CI
```

---

## Recently Completed Work

### Authentication System Implementation (ecfb14e2, 462f311b, 0a7b526f)
| Change | Description |
|--------|-------------|
| **API Key Middleware** | Implemented `api_key_middleware` in `auth.rs` with namespace-based authorization |
| **Session ID Header** | Added `X-Session-Id` header requirement for protected endpoints |
| **Namespace Auth** | API keys required only for non-default namespaces (`/api/chat` with namespace != "default") |
| **AuthorizedKey** | Stores allowed namespaces per API key, validates access |
| **Query Parameter Support** | Supports both `namespace` and `chatbot` query params (backward compatible) |
| **Auth Tests** | Comprehensive test suite in `auth.rs` covering all auth scenarios |

### Admin CLI Tool (6386b039, aaf346aa)
| Change | Description |
|--------|-------------|
| **New Binary** | Created `bin/admin-cli` for database management operations |
| **API Key Management** | `api-keys` command: create, list, update, delete API keys |
| **Session Management** | `sessions` command: list, get, delete sessions |
| **User Management** | `users` command: manage user records |
| **Contract Management** | `contracts` command: manage contract data |
| **CLI Structure** | Modular command structure with shared database utilities |

### Database Migrations
| Change | Description |
|--------|-------------|
| **API Keys Table** | Created `api_keys` table with `api_key`, `label`, `allowed_namespaces` (JSONB), `is_active` |
| **Namespace Migration** | Renamed `allowed_chatbots` → `allowed_namespaces` for consistency |
| **Indexes** | Added index on `is_active` for efficient key lookups |

### Endpoint Updates
| Change | Description |
|--------|-------------|
| **Session ID Required** | `/api/chat`, `/api/state`, `/api/interrupt`, `/api/updates`, `/api/system`, `/api/events`, `/api/memory-mode`, `/api/sessions/*`, `/api/db/sessions/*` now require `X-Session-Id` header |
| **API Key Enforcement** | `/api/chat` requires API key only for non-default namespaces |
| **Auth Middleware Integration** | All endpoints updated to use `api_key_middleware` and extract `SessionId` |
| **Namespace Parameter** | Endpoints updated to use `namespace` query param (backward compatible with `chatbot`) |

### Test Infrastructure
| Change | Description |
|--------|-------------|
| **test-api-auth.sh** | Comprehensive auth test script covering all endpoints and scenarios |
| **Test Coverage** | Tests for public endpoints, session header requirements, API key validation, namespace authorization |
| **Admin CLI Integration** | Test script uses admin CLI to create test keys dynamically |

### Query Parameter Standardization (b1fcfb78)
| Change | Description |
|--------|-------------|
| **Renamed Parameter** | Changed `chatbot` query parameter to `namespace` throughout codebase |
| **Backward Compatibility** | Still supports `chatbot` parameter for backward compatibility |
| **Consistent Naming** | All references now use "namespace" terminology consistently |

---

## Files Modified This Sprint

### Authentication & Authorization (This Branch)
| File | Description |
|------|-------------|
| `aomi/bin/backend/src/auth.rs` | **NEW** — API key middleware, session ID handling, namespace authorization (425 lines) |
| `aomi/bin/backend/src/endpoint/mod.rs` | Updated to use auth middleware, namespace parameter handling |
| `aomi/bin/backend/src/endpoint/sessions.rs` | Updated to require session ID header |
| `aomi/bin/backend/src/endpoint/system.rs` | Updated to require session ID header |
| `aomi/bin/backend/src/endpoint/db.rs` | Updated to require session ID header for session-specific endpoints |
| `aomi/bin/backend/src/main.rs` | Integrated auth middleware into router |

### Admin CLI (This Branch)
| File | Description |
|------|-------------|
| `aomi/bin/admin-cli/src/main.rs` | **NEW** — Main CLI entry point |
| `aomi/bin/admin-cli/src/cli.rs` | **NEW** — CLI argument parsing with clap |
| `aomi/bin/admin-cli/src/commands/api_keys.rs` | **NEW** — API key CRUD operations |
| `aomi/bin/admin-cli/src/commands/sessions.rs` | **NEW** — Session management commands |
| `aomi/bin/admin-cli/src/commands/users.rs` | **NEW** — User management commands |
| `aomi/bin/admin-cli/src/commands/contracts.rs` | **NEW** — Contract management commands |
| `aomi/bin/admin-cli/src/db.rs` | **NEW** — Database connection utilities |
| `aomi/bin/admin-cli/src/models.rs` | **NEW** — Database model definitions |
| `aomi/bin/admin-cli/src/util.rs` | **NEW** — Utility functions (JSON printing, etc.) |

### Database Migrations
| File | Description |
|------|-------------|
| `aomi/bin/backend/migrations/20250115000000_add_api_keys.sql` | **NEW** — Creates api_keys table |
| `aomi/bin/backend/migrations/20250116000000_rename_api_keys_namespaces.sql` | **NEW** — Renames allowed_chatbots to allowed_namespaces |

### Test Scripts
| File | Description |
|------|-------------|
| `scripts/test-api-auth.sh` | **NEW** — Comprehensive auth test script (268 lines) |
| `scripts/test-backend-curl.sh` | Updated for new auth requirements |
| `scripts/test-proxy-curl.sh` | Updated for new auth requirements |
| `scripts/test-sse.sh` | Updated for new auth requirements |
| `scripts/curl-integration.sh` | Updated for new auth requirements |

### Configuration & Documentation
| File | Description |
|------|-------------|
| `.env.template` | Updated with auth-related environment variables |
| `README.md` | Updated with auth documentation |
| `aomi/Cargo.toml` | Added admin-cli binary |
| `aomi/bin/backend/Cargo.toml` | Added auth dependencies |

---

## Pending Tasks

### Immediate Priority

1. **Merge to main**:
   - Review all changes in feat/auth branch
   - Ensure all tests pass (`./scripts/test-api-auth.sh`)
   - Verify admin CLI works correctly
   - Create PR to merge auth system to main

2. **Frontend Integration**:
   - Update frontend to send `X-Session-Id` header on all API requests
   - Update frontend to send `X-API-Key` header for non-default namespaces
   - Handle 401/403 errors appropriately in UI
   - Add UI for API key management (if needed)

3. **Documentation**:
   - Document API key creation and management workflow
   - Update API documentation with auth requirements
   - Document session ID generation and management

### Completed (this sprint: feat/auth)

1. **Authentication System** ✓:
   - API key middleware with namespace-based authorization (ecfb14e2)
   - Session ID header requirement for protected endpoints
   - Comprehensive test coverage

2. **Admin CLI** ✓:
   - Full CLI tool for managing API keys, sessions, users, contracts (6386b039)
   - Database utilities and model definitions

3. **Database Migrations** ✓:
   - API keys table with namespace support
   - Migration to rename chatbot → namespace

4. **Endpoint Updates** ✓:
   - All endpoints updated to use auth middleware
   - Session ID extraction and validation
   - Namespace parameter standardization (b1fcfb78)

5. **Test Infrastructure** ✓:
   - Comprehensive auth test script (0a7b526f)
   - Updated existing test scripts for auth requirements

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Frontend needs auth integration | Pending | Frontend must send X-Session-Id and X-API-Key headers |
| API key rotation | Pending | No built-in key rotation mechanism yet |
| Session expiration | Pending | Sessions don't expire automatically |
| No known blockers in feat/auth | Clean | All tests passing, ready for review |

---

## Multi-Step Flow State

Current Position: Migration Phase (Steps 1-8 done, Step 9 pending)

| Step | Description | Status |
|------|-------------|--------|
| 1 | Define SystemEvent + SystemEventQueue | ✓ Done |
| 2 | Inject queue into ChatApp/SessionState constructors | ✓ Done |
| 3 | Update stream_completion to route system signals | ✓ Done |
| 4 | Update SessionState::update_state to drain queue | ✓ Done |
| 5 | Refactor ToolScheduler for multi-step tool results | ✓ Done |
| 5a | Separate tool_stream.rs module (ToolResultFuture/Stream) | ✓ Done |
| 5b | Fix premature null results for single-result tools | ✓ Done |
| 5c | Implement into_shared_streams() with fanout for multi-step | ✓ Done |
| 5d | All scheduler tests passing | ✓ Done |
| 6 | Route multi-step results to SystemEventQueue | ✓ Done |
| 6a | Add `ToolCompletion` type (call_id, tool_name, is_multi_step, result) | ✓ Done |
| 6b | Add metadata fields to `ToolStreamream` (tool_name, is_multi_step) | ✓ Done |
| 6c | Add `AsyncToolResult` to CoreCommand, `SystemToolDisplay` to SystemEvent | ✓ Done |
| 6d | `poll_streams_to_next_result()` returns `ToolCompletion` | ✓ Done |
| 6e | Finalization loop yields `AsyncToolResult` for multi-step tools | ✓ Done |
| 6f | session.rs matches `AsyncToolResult` → pushes `SystemToolDisplay` | ✓ Done |
| 7 | Wire wallet flow through system events | Pending |
| 8 | Update sync_state() to return system events | ✓ Done |
| 9 | Frontend integration | Pending |
| **NEW** | Async event-driven refactor (finalization to session layer) | ✓ Done |
| **NEW** | Async spawn for tool calls in scheduler | ✓ Done |
| **NEW** | Forge executor multi-plan support | ✓ Done |
| **NEW** | CLI system events tests | ✓ Done |

---

## Notes for Next Agent

### Critical Context

1. **Current Branch: feat/auth**
   - This branch implements API key-based authentication with namespace-level authorization
   - All tests passing, ready for review and merge to main
   - Frontend integration pending

2. **Recent Changes (This Branch)**
   - **Authentication System**: Complete API key middleware with namespace-based auth (`auth.rs`)
   - **Session ID Headers**: All protected endpoints require `X-Session-Id` header
   - **Admin CLI**: New CLI tool for managing API keys, sessions, users, contracts
   - **Database Migrations**: API keys table with namespace support
   - **Query Parameter**: Standardized on `namespace` (backward compatible with `chatbot`)

3. **Authentication Architecture**
   - **API Key Middleware**: Validates API keys from `X-API-Key` header
   - **Namespace Authorization**: API keys have `allowed_namespaces` (JSONB array)
   - **Default Namespace**: No API key required for `namespace=default`
   - **Protected Endpoints**: `/api/chat` requires API key only for non-default namespaces
   - **Session ID**: Required for all session-scoped endpoints (`/api/chat`, `/api/state`, `/api/interrupt`, etc.)

4. **Admin CLI Usage**
   ```bash
   # Create API key
   cargo run -p admin-cli -- api-keys create -n "l2beat" -l "L2Beat Key" -k "my-key-value"
   
   # List API keys
   cargo run -p admin-cli -- api-keys list
   
   # Update API key
   cargo run -p admin-cli -- api-keys update <key-id> --namespaces "default,l2beat"
   ```

5. **Testing**
   - Run auth tests: `./scripts/test-api-auth.sh`
   - Tests cover: public endpoints, session headers, API key validation, namespace authorization
   - Test script uses admin CLI to create test keys dynamically

6. **What's Missing**
   - Frontend needs to send `X-Session-Id` header on all API requests
   - Frontend needs to send `X-API-Key` header for non-default namespaces
   - API key rotation mechanism
   - Session expiration logic

### Key Files (Updated This Branch)
```
aomi/bin/backend/src/auth.rs                    # API key middleware, session ID handling
aomi/bin/backend/src/endpoint/mod.rs            # Auth middleware integration
aomi/bin/backend/src/endpoint/sessions.rs       # Session endpoints with auth
aomi/bin/backend/src/endpoint/system.rs         # System endpoints with auth
aomi/bin/admin-cli/src/                         # Admin CLI tool (NEW)
aomi/bin/backend/migrations/                    # API keys table migrations
scripts/test-api-auth.sh                        # Comprehensive auth tests (NEW)
```

### Authentication Flow Diagrams

> **See also**: [`specs/auth-flow.md`](./auth-flow.md) for detailed Mermaid sequence diagrams showing all authentication flows, including API key creation, authorization scenarios, and error cases.

### Authentication Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         AUTHENTICATION ARCHITECTURE                          │
└─────────────────────────────────────────────────────────────────────────────┘

┌──────────────┐
│   Client     │
│  (Frontend)  │
└──────┬───────┘
       │
       │ HTTP Request
       │ Headers: X-Session-Id, X-API-Key (optional)
       │
       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    api_key_middleware()                                    │
│                         (auth.rs)                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. Check if path starts with /api/                                         │
│     └─ NO → Skip middleware, allow request                                  │
│     └─ YES → Continue                                                       │
│                                                                              │
│  2. Check if endpoint requires Session ID                                   │
│     Required for:                                                            │
│     - /api/chat, /api/state, /api/interrupt                                 │
│     - /api/updates, /api/system, /api/events                                │
│     - /api/memory-mode                                                       │
│     - /api/sessions/:id, /api/db/sessions/:id                               │
│                                                                              │
│     └─ Extract X-Session-Id header                                          │
│        └─ Missing → Return 400 BAD_REQUEST                                  │
│        └─ Present → Insert SessionId extension                              │
│                                                                              │
│  3. Check if endpoint requires API Key                                      │
│     Only /api/chat with non-default namespace requires API key              │
│                                                                              │
│     ┌──────────────────────────────────────────────────────────┐            │
│     │ requires_api_key() check:                                │            │
│     │  - Path == /api/chat?                                    │            │
│     │  - Extract namespace from query params                   │            │
│     │  - namespace != "default" → requires API key             │            │
│     └──────────────────────────────────────────────────────────┘            │
│                                                                              │
│     └─ API Key NOT required → Skip to step 4                                │
│     └─ API Key required → Continue                                          │
│                                                                              │
│        ┌──────────────────────────────────────────────────────┐             │
│        │ Extract X-API-Key header                             │             │
│        │  └─ Missing/Empty → Return 401 UNAUTHORIZED          │             │
│        │  └─ Present → Continue                               │             │
│        └──────────────────────────────────────────────────────┘             │
│                                                                              │
│        ┌──────────────────────────────────────────────────────┐             │
│        │ Query Database: api_keys table                       │             │
│        │  SELECT allowed_namespaces                            │             │
│        │  FROM api_keys                                        │             │
│        │  WHERE api_key = $1 AND is_active = TRUE             │             │
│        │                                                       │             │
│        │  └─ Key not found → Return 403 FORBIDDEN             │             │
│        │  └─ Key found → Parse allowed_namespaces (JSONB)     │             │
│        └──────────────────────────────────────────────────────┘             │
│                                                                              │
│        ┌──────────────────────────────────────────────────────┐             │
│        │ Validate Namespace Authorization                     │             │
│        │  - Normalize namespace to lowercase                  │             │
│        │  - Check if namespace in allowed_namespaces          │             │
│        │                                                       │             │
│        │  └─ Not allowed → Return 403 FORBIDDEN               │             │
│        │  └─ Allowed → Insert AuthorizedKey extension         │             │
│        └──────────────────────────────────────────────────────┘             │
│                                                                              │
│  4. Request passes middleware → Continue to endpoint handler                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
       │
       │ Request with extensions:
       │ - SessionId (if required)
       │ - AuthorizedKey (if API key validated)
       │
       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Endpoint Handler                                    │
│  (chat_endpoint, state_endpoint, etc.)                                      │
│                                                                              │
│  - Extract SessionId from Extension<SessionId>                              │
│  - Extract AuthorizedKey from Option<Extension<AuthorizedKey>>              │
│  - Validate namespace access (if needed)                                    │
│  - Process request                                                          │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                         DATABASE SCHEMA                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  api_keys table:                                                            │
│  ┌─────────────┬──────────────┬─────────────────────┬──────────┬─────────┐ │
│  │ api_key     │ label        │ allowed_namespaces  │ is_active│created_at│
│  ├─────────────┼──────────────┼─────────────────────┼──────────┼─────────┤ │
│  │ "key-123"   │ "L2Beat Key" │ ["l2beat"]          │ true     │ ...     │ │
│  │ "key-456"   │ "Multi NS"   │ ["default","l2beat"]│ true     │ ...     │ │
│  │ "key-789"   │ "Inactive"   │ ["default"]         │ false    │ ...     │ │
│  └─────────────┴──────────────┴─────────────────────┴──────────┴─────────┘ │
│                                                                              │
│  allowed_namespaces: JSONB array of strings (normalized to lowercase)       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                         ADMIN CLI FLOW                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  admin-cli api-keys create -n "l2beat" -l "Label" -k "key-value"           │
│       │                                                                      │
│       ▼                                                                      │
│  ┌──────────────────────────────────────────────────────────┐               │
│  │ Parse namespaces: "l2beat" → ["l2beat"]                  │               │
│  │ Generate key (if not provided): random 32-byte hex       │               │
│  └──────────────────────────────────────────────────────────┘               │
│       │                                                                      │
│       ▼                                                                      │
│  ┌──────────────────────────────────────────────────────────┐               │
│  │ INSERT INTO api_keys                                     │               │
│  │   (api_key, label, allowed_namespaces, is_active)        │               │
│  │ VALUES ($1, $2, $3::jsonb, TRUE)                         │               │
│  └──────────────────────────────────────────────────────────┘               │
│       │                                                                      │
│       ▼                                                                      │
│  Return created key info (JSON)                                             │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Authentication Decision Tree

```
                    HTTP Request
                         │
                         ▼
              ┌──────────────────────┐
              │ Path starts with     │
              │ /api/ ?              │
              └──────────┬───────────┘
                         │
            ┌────────────┴────────────┐
            │ NO                      │ YES
            ▼                         ▼
    ┌───────────────┐      ┌─────────────────────┐
    │ Allow Request │      │ Requires Session ID? │
    │ (Public)      │      └──────────┬──────────┘
    └───────────────┘                 │
                            ┌─────────┴─────────┐
                            │ YES               │ NO
                            ▼                   ▼
                  ┌──────────────────┐  ┌──────────────┐
                  │ X-Session-Id     │  │ Allow Request│
                  │ header present?  │  └──────────────┘
                  └────────┬─────────┘
                           │
              ┌────────────┴────────────┐
              │ NO                      │ YES
              ▼                         ▼
    ┌──────────────────┐      ┌──────────────────────┐
    │ Return 400       │      │ Endpoint is /api/chat  │
    │ BAD_REQUEST      │      │ with non-default      │
    └──────────────────┘      │ namespace?            │
                              └──────────┬───────────┘
                                         │
                           ┌─────────────┴─────────────┐
                           │ NO                         │ YES
                           ▼                            ▼
                  ┌──────────────────┐      ┌──────────────────────┐
                  │ Allow Request    │      │ X-API-Key header     │
                  └──────────────────┘      │ present?             │
                                            └──────────┬───────────┘
                                                       │
                                         ┌─────────────┴─────────────┐
                                         │ NO                         │ YES
                                         ▼                            ▼
                                ┌──────────────────┐      ┌──────────────────────┐
                                │ Return 401       │      │ Query api_keys table │
                                │ UNAUTHORIZED     │      └──────────┬───────────┘
                                └──────────────────┘                 │
                                                         ┌───────────┴───────────┐
                                                         │ Key found & active?   │
                                                         └──────────┬───────────┘
                                                                    │
                                                    ┌───────────────┴───────────────┐
                                                    │ NO                             │ YES
                                                    ▼                                ▼
                                        ┌──────────────────┐      ┌──────────────────────────┐
                                        │ Return 403       │      │ Namespace in              │
                                        │ FORBIDDEN        │      │ allowed_namespaces?       │
                                        └──────────────────┘      └──────────┬───────────────┘
                                                                              │
                                                                    ┌─────────┴─────────┐
                                                                    │ NO                 │ YES
                                                                    ▼                    ▼
                                                        ┌──────────────────┐  ┌──────────────────┐
                                                        │ Return 403       │  │ Allow Request    │
                                                        │ FORBIDDEN        │  │ (with extensions)│
                                                        └──────────────────┘  └──────────────────┘
```

### API Key Requirements

| Endpoint | Session ID | API Key |
|----------|------------|---------|
| `/health` | No | No |
| `/api/chat?namespace=default` | Yes | No |
| `/api/chat?namespace=l2beat` | Yes | Yes (must allow "l2beat") |
| `/api/state` | Yes | No |
| `/api/interrupt` | Yes | No |
| `/api/updates` | Yes | No |
| `/api/system` | Yes | No |
| `/api/sessions` | No | No |
| `/api/db/sessions/:id` | Yes | No |

### Message Flow Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              MESSAGE FLOW                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  UI                                                                          │
│   │                                                                          │
│   ├─▶ send_user_input() ───▶ [sender_to_llm] ───────────────-──────────────┐ │
│   │                                                                        │ │
│   └─▶ send_ui_event() ───▶ SystemEventQueue.push()                         │ │
│                                    │                                       │ │
│                                    ▼                                       │ │
│                            [append-only log]                               │ │
│                             events: Vec<E>                                 │ │
│                             frontend_cnt: N                                │ │
│                             llm_cnt: M                                     │ │
│                                    │                                       │ │
│  ┌─────────────────────────────────┼───────────────────────────────────────┤ │
│  │ Background LLM Task             ▼                                       │ │
│  │  receiver_from_ui.recv() ◀──────┘                                       │ │
│  │         │                                                               │ │
│  │         ▼                                                               │ │
│  │  AomiBackend.process_message()                                          │ │
│  │         │                                                               │ │
│  │         ▼                                                               │ │
│  │  CompletionRunner.stream()                                              │ │
│  │         │                                                               │ │
│  │         ├─▶ consume_stream_item() ─▶ CoreCommand::StreamingText         │ │
│  │         │                                      │                         │ │
│  │         └─▶ consume_tool_call() ──────────────┐│                        │ │
│  │                                               ││                        │ │
│  │  ToolHandler                               ││                        │ │
│  │         │                                     ││                        │ │
│  │         ▼ request()                           ││                        │ │
│  │         ▼ resolve_last_call() ──▶ ui_stream   ││                        │ │
│  │                │                      │       ││                        │ │
│  │                ▼ bg_stream            │       ││                        │ │
│  │    [ongoing_streams]                  │       ││                        │ │
│  └────────────────│──────────────────────┼───────┼┼────────────────────────┤ │
│                   │                      │       ││                        │ │
│  ┌────────────────┼──────────────────────┼───────┼┼────────────────────────┤ │
│  │ Background Poller Task                │       ││                        │ │
│  │         │                             │       ││                        │ │
│  │         ▼ poll_streams_once()         │       ││                        │ │
│  │         ▼ take_completed_calls()      │       ││                        │ │
│  │         │                             │       ││                        │ │
│  │         ▼                             │       ││                        │ │
│  │  SystemEventQueue.push_tool_update()  │       ││                        │ │
│  └─────────────────│─────────────────────┼───────┼┼────────────────────────┤ │
│                    │                     │       ││                        │ │
│                    ▼                     │       ▼▼                        │ │
│             [append-only log]            │  [command_sender]                 │ │
│                    │                     │       │                         │ │
│                    │                     │       ▼                         │ │
│  SessionState      │                     │  sync_state()                   │ │
│         │          │                     │       │                         │ │
│         │          │                     │       ├─▶ update messages[]     │ │
│         │          │                     │       │                         │ │
│         │          │                     └───────┼─▶ poll_ui_streams()   │ │
│         │          │                             │                         │ │
│         │          └─────────────────────────────┼─▶ sync_system_events()  │ │
│         │                                        │        │                │ │
│         │  advance_frontend_events() ◀───────────┘        ▼                │ │
│         │          │                             send_events_to_history()  │ │
│         ▼          ▼                                      │                │ │
│       API Response (ChatState)                            ▼                │ │
│                                                   agent_history.push()     │ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Method Naming Conventions**:

| Layer | Pattern | Methods |
|-------|---------|---------|
| SessionState | `send_*`, `sync_*` | `send_user_input`, `send_ui_event`, `send_system_prompt`, `sync_state`, `send_events_to_history` |
| AomiBackend | `process_*` | `process_message` |
| CompletionRunner | `consume_*` | `consume_stream_item`, `consume_tool_call` |
| ToolHandler | `request`, `resolve_*`, `poll_*` | `request`, `resolve_last_call`, `poll_streams_once` |

### Quick Start Commands
```bash
# Check compilation
cargo check --all

# Run clippy
cargo clippy --all

# Run tests
cargo test --all

# Run auth tests
./scripts/test-api-auth.sh

# Create API key
cargo run -p admin-cli -- api-keys create -n "l2beat" -l "Test Key"

# List API keys
cargo run -p admin-cli -- api-keys list
```

### Implementation Next Steps

**This Branch (feat/auth)**:
1. **Merge preparation**:
   - Run full test suite: `cargo test --all`
   - Run auth tests: `./scripts/test-api-auth.sh`
   - Run clippy: `cargo clippy --all`
   - Create PR for review
   - Merge to main

2. **Frontend Integration**:
   - Update frontend to generate and send `X-Session-Id` header
   - Update frontend to send `X-API-Key` header for non-default namespaces
   - Handle 401/403 errors with appropriate UI feedback
   - Test end-to-end auth flow

**Next Sprint**:
1. **API Key Management UI** (optional):
   - Add UI for creating/managing API keys
   - Show current API key status
   - Allow key rotation

2. **Session Management**:
   - Implement session expiration
   - Add session cleanup for inactive sessions
   - Add session metadata (last activity, etc.)

**Testing patterns**:
- Use `./scripts/test-api-auth.sh` for comprehensive auth testing
- Admin CLI for key management in tests
- All endpoints require session ID where appropriate
- API keys validated against namespace requirements
