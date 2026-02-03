# Codex Architecture (Rust)

> **Purpose**: Reference architecture for OpenAI's Codex CLI - a terminal-based AI coding agent.
>
> **Source**: `/Users/cecilia/Code/codex/codex-rs/`

---

## 1. System Overview

**Codex CLI** is a local coding agent that runs in the terminal, enabling AI-assisted software development through natural language. Key capabilities:

- Execute shell commands with approval workflows
- Read/write files with unified diff patches
- Search codebases (grep, file search)
- Integrate with external tools via MCP (Model Context Protocol)
- Sandbox command execution for safety

**Platforms**: macOS (Seatbelt), Linux (Landlock), Windows (experimental)

---

## 2. Workspace Structure

The Rust codebase is organized as a Cargo workspace with ~50 crates:

### Core Business Logic
| Crate | Path | Purpose |
|-------|------|---------|
| `codex-core` | `/core` | Central engine: agent, client, tools, config, MCP |
| `codex-protocol` | `/protocol` | Shared types (SQ/EQ pattern), models, approvals |
| `codex-common` | `/common` | CLI arg types, config overrides |

### Entry Points (Binaries)
| Crate | Binary | Purpose |
|-------|--------|---------|
| `codex-cli` | `codex` | Main CLI multitool (routes to subcommands) |
| `codex-tui` | `codex-tui` | Interactive fullscreen TUI (Ratatui) |
| `codex-tui2` | `codex-tui2` | Next-gen TUI (experimental) |
| `codex-exec` | `codex-exec` | Headless non-interactive execution |

### Server Components
| Crate | Purpose |
|-------|---------|
| `codex-app-server` | JSON-RPC server for IDE integrations (VSCode) |
| `codex-mcp-server` | Exposes Codex as an MCP tool |
| `codex-responses-api-proxy` | Internal proxy for OpenAI Responses API |

### Sandbox & Security
| Crate | Platform | Purpose |
|-------|----------|---------|
| `codex-linux-sandbox` | Linux | Landlock + seccomp sandbox |
| `codex-windows-sandbox` | Windows | Restricted token sandbox |
| `codex-process-hardening` | All | Process security utilities |
| `codex-execpolicy` | All | Starlark-based policy engine |

### MCP Integration
| Crate | Purpose |
|-------|---------|
| `codex-rmcp-client` | MCP client (connects to external servers) |
| `mcp-types` | MCP protocol type definitions |

### Authentication
| Crate | Purpose |
|-------|---------|
| `codex-login` | OAuth/device code flows |
| `codex-keyring-store` | OS keyring credential storage |
| `codex-chatgpt` | ChatGPT-specific auth |

### Utilities
| Crate | Purpose |
|-------|---------|
| `codex-apply-patch` | Unified diff parser and applier |
| `codex-file-search` | Fuzzy file search (nucleo-matcher) |
| `codex-ansi-escape` | ANSI escape utilities |
| `codex-feedback` | Telemetry collection |
| `codex-otel` | OpenTelemetry instrumentation |

---

## 3. Core Architecture (SQ/EQ Pattern)

The core uses a **Submission Queue / Event Queue** pattern for async message passing:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           CLI / TUI                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                │
│  │   TUI    │  │   Exec   │  │   MCP    │  │   App    │                │
│  │ (codex)  │  │ (exec)   │  │ (server) │  │ (server) │                │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘                │
└───────┼─────────────┼─────────────┼─────────────┼───────────────────────┘
        │             │             │             │
        ▼             ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          codex-core                                      │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                      ThreadManager                               │   │
│  │  - Creates/resumes conversation threads                         │   │
│  │  - Maintains AuthManager, ModelsManager, SkillsManager          │   │
│  └─────────────────────────────┬───────────────────────────────────┘   │
│                                │                                        │
│  ┌─────────────────────────────▼───────────────────────────────────┐   │
│  │                       CodexThread                                │   │
│  │  - Wraps Codex instance                                         │   │
│  │  - submit(Op) -> Result<EventId>                                │   │
│  │  - next_event() -> Event                                        │   │
│  └─────────────────────────────┬───────────────────────────────────┘   │
│                                │                                        │
│  ┌─────────────────────────────▼───────────────────────────────────┐   │
│  │                          Codex                                   │   │
│  │  - SQ/EQ pattern (async task processes submissions)             │   │
│  │  - Routes to SessionTask handlers                               │   │
│  └─────────────────────────────┬───────────────────────────────────┘   │
│                                │                                        │
│  ┌─────────────────────────────▼───────────────────────────────────┐   │
│  │                       ModelClient                                │   │
│  │  - API calls (Responses API, Chat Completions)                  │   │
│  │  - Auth, retries, streaming                                     │   │
│  └─────────────────────────────┬───────────────────────────────────┘   │
│                                │                                        │
│  ┌─────────────────────────────▼───────────────────────────────────┐   │
│  │                 ToolRouter / ToolOrchestrator                    │   │
│  │  - Dispatch tool calls to handlers                              │   │
│  │  - Approval flow + sandbox selection                            │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### Key Types

```rust
// Submission (from client)
Op::UserTurn {
    items: Vec<UserInput>,
    cwd: PathBuf,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
    model: String,
    effort: Option<ReasoningEffort>,
    summary: ReasoningSummary,
}

// Events (to client)
EventMsg::TurnStarted { ... }
EventMsg::AgentMessageContentDelta { text }
EventMsg::ExecApprovalRequest { command, ... }
EventMsg::ItemCompleted { item: TurnItem }
EventMsg::TurnCompleted { ... }
EventMsg::Error { ... }
```

---

## 4. Completion/Chat Flow

```
User Input
    │
    ▼
Op::UserTurn { items, cwd, approval_policy, sandbox_policy, model }
    │
    ▼
┌───────────────────────────────────────────────────────────────┐
│                    Codex.process_submission()                  │
│  1. Build prompt (system + user instructions, env context,    │
│     conversation history)                                     │
│  2. Create ModelClientSession                                 │
└───────────────────────────────┬───────────────────────────────┘
                                │
                                ▼
┌───────────────────────────────────────────────────────────────┐
│              ModelClientSession.send_prompt()                  │
│  - Wire API: Responses API or Chat Completions               │
│  - Includes tools JSON specification                         │
│  - Streams SSE events                                        │
└───────────────────────────────┬───────────────────────────────┘
                                │
    ┌───────────────────────────┴───────────────────────────┐
    │                                                       │
    ▼                                                       ▼
ResponseEvent::Item                            ResponseEvent::Done
    │
    ├── Message { content }  ──► emit text delta events
    │
    └── FunctionCall { name, arguments, call_id }
            │
            ▼
        ToolRouter.dispatch()
            │
            ▼
        ToolOrchestrator.run()
            │
            ├── Check approval requirement
            ├── Select sandbox
            ├── Execute handler
            └── Retry without sandbox on denial
            │
            ▼
        FunctionResult { output }
            │
            ▼
        Loop: Send result back to model
```

---

## 5. Tool System

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          Tool System                                     │
│                                                                         │
│  ┌─────────────────────┐      ┌─────────────────────┐                  │
│  │ ToolRegistryBuilder │ ──►  │    ToolRegistry     │                  │
│  │  - push_spec()      │      │  - handlers: Map    │                  │
│  │  - register_handler │      │  - dispatch()       │                  │
│  └─────────────────────┘      └──────────┬──────────┘                  │
│                                          │                              │
│  ┌───────────────────────────────────────▼──────────────────────────┐  │
│  │                        ToolOrchestrator                           │  │
│  │  1. Check ExecApprovalRequirement                                │  │
│  │     - Skip (auto-approve)                                        │  │
│  │     - Forbidden (deny)                                           │  │
│  │     - NeedsApproval (prompt user)                                │  │
│  │  2. Select sandbox via SandboxManager                            │  │
│  │  3. Execute handler                                              │  │
│  │  4. Retry without sandbox on denial (if policy allows)           │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

### Built-in Tool Handlers

| Handler | File | Purpose |
|---------|------|---------|
| `ShellHandler` | `handlers/shell.rs` | Execute shell commands |
| `ApplyPatchHandler` | `handlers/apply_patch.rs` | Apply unified diffs |
| `ReadFileHandler` | `handlers/read_file.rs` | Read file contents |
| `ListDirHandler` | `handlers/list_dir.rs` | List directory contents |
| `GrepFilesHandler` | `handlers/grep_files.rs` | Search files with regex |
| `McpHandler` | `handlers/mcp.rs` | Execute MCP tool calls |
| `McpResourceHandler` | `handlers/mcp_resource.rs` | Read MCP resources |
| `PlanHandler` | `handlers/plan.rs` | Task/plan management |
| `ViewImageHandler` | `handlers/view_image.rs` | Attach local images |

---

## 6. Sandbox Architecture

### Platform Selection

```
SandboxManager.select_initial(policy, preference)
    │
    ├── policy = ReadOnly        → Seatbelt/Landlock/None
    ├── policy = WorkspaceWrite  → Based on platform + config
    └── policy = DangerFullAccess → SandboxType::None

Platform Detection:
    - macOS: Seatbelt (sandbox-exec)
    - Linux: Landlock + seccomp
    - Windows: Restricted token (experimental, feature-gated)
```

### Approval Flow

```rust
enum ExecApprovalRequirement {
    Skip { reason: Option<String> },           // Auto-approve
    Forbidden { reason: String },              // Always deny
    NeedsApproval { reason: Option<String> },  // Prompt user
}

enum ReviewDecision {
    Approved,                                   // Execute once
    ApprovedForSession,                         // Remember for session
    ApprovedExecpolicyAmendment { ... },       // Add to execpolicy
    Denied,                                     // Reject this call
    Abort,                                      // Abort entire turn
}
```

### macOS Seatbelt Policy

```scheme
(version 1)
(deny default)              ; Closed by default
(allow file-read*)          ; Read-only operations
(allow process-exec)        ; Child processes inherit
(allow process-fork)
(allow file-write*          ; Limited write to writable roots
  (subpath (param "WRITABLE_ROOT")))
```

---

## 7. MCP Integration

Codex supports MCP in two directions:

### As MCP Client (connecting to external servers)

```
┌──────────────────────────────────────────────────────────────┐
│                   McpConnectionManager                        │
│  - Spawns/connects to configured MCP servers                 │
│  - Handles OAuth authentication                              │
│  - Discovers tools/resources                                 │
└────────────────────────────┬─────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│                      RmcpClient                              │
│  - Wraps the `rmcp` crate                                   │
│  - Lists tools/resources                                    │
│  - Calls tools                                              │
└──────────────────────────────────────────────────────────────┘

Tool Call Flow:
1. Model requests tool "mcp__servername__toolname"
2. McpHandler extracts server/tool names
3. RmcpClient.call_tool(tool_name, arguments)
4. Result returned to model
```

### As MCP Server (exposing Codex as a tool)

```
codex mcp-server
    │
    ▼
┌──────────────────────────────────────────────────────────────┐
│              MCP Server (stdio transport)                     │
│                                                              │
│  stdin  ───► JSONRPCMessage ───► MessageProcessor            │
│  stdout ◄─── JSONRPCMessage ◄───────────┘                    │
│                                                              │
│  Exposed Tool: "codex"                                       │
│  - Accepts prompt as input                                   │
│  - Runs Codex agent internally                               │
│  - Returns result                                            │
└──────────────────────────────────────────────────────────────┘
```

### Config Example

```toml
[mcp_servers.my-server]
transport = "stdio"
command = ["npx", "my-mcp-server"]

# OR

[mcp_servers.remote]
transport = "streamable_http"
url = "https://..."
```

---

## 8. Configuration System

### Layers (in precedence order)

```
1. CLI Overrides       (-c key=value flags)
2. Profile Overrides   (profiles.<name> in config.toml)
3. Project Config      (.codex/config.toml in repo)
4. User Config         (~/.codex/config.toml)
5. Requirements        (requirements.toml - enterprise)
6. Built-in Defaults
```

### Key Files

| File | Location | Purpose |
|------|----------|---------|
| `config.toml` | `~/.codex/` | User-level configuration |
| `config.toml` | `.codex/` (project) | Project-level overrides |
| `requirements.toml` | `.codex/` | Enterprise policy constraints |
| `auth.json` | `~/.codex/` | Stored authentication |
| `AGENTS.md` | Project root | Project instructions |
| `SKILL.md` | Any directory | Skill definitions |

### ConfigToml Structure

```rust
pub struct ConfigToml {
    // Model settings
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub model_context_window: Option<i64>,
    pub model_reasoning_effort: Option<ReasoningEffort>,
    
    // Approval & Sandbox
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    
    // Instructions
    pub instructions: Option<String>,
    pub developer_instructions: Option<String>,
    
    // MCP servers
    pub mcp_servers: HashMap<String, McpServerConfig>,
    
    // Model providers
    pub model_providers: HashMap<String, ModelProviderInfo>,
    
    // Profiles
    pub profiles: HashMap<String, ConfigProfile>,
    
    // Feature flags
    pub features: Option<FeaturesToml>,
}
```

### Feature Flags

```rust
enum Feature {
    ApplyPatchTool,
    WebSearchRequest,
    UnifiedExec,
    WindowsSandbox,
    Tui2,
    // ...
}

enum Stage {
    Experimental,
    Beta { rollout_percent: u8 },
    Stable,
    Deprecated,
    Removed,
}
```

---

## 9. Key File Paths

### Entry Points
- `/cli/src/main.rs` - Main CLI entry
- `/tui/src/lib.rs` - TUI library
- `/exec/src/lib.rs` - Headless exec
- `/mcp-server/src/lib.rs` - MCP server
- `/app-server/src/lib.rs` - App server for IDEs

### Core
- `/core/src/lib.rs` - Core library root
- `/core/src/codex.rs` - Main Codex engine
- `/core/src/thread_manager.rs` - Thread lifecycle
- `/core/src/client.rs` - Model API client
- `/core/src/config/mod.rs` - Configuration

### Tools
- `/core/src/tools/mod.rs` - Tools root
- `/core/src/tools/registry.rs` - Registration
- `/core/src/tools/orchestrator.rs` - Approval/sandbox
- `/core/src/tools/handlers/*.rs` - Implementations

### Sandbox
- `/core/src/seatbelt.rs` - macOS sandbox
- `/core/src/landlock.rs` - Linux sandbox
- `/linux-sandbox/` - Linux sandbox crate
- `/windows-sandbox-rs/` - Windows sandbox

### MCP
- `/core/src/mcp/mod.rs` - MCP integration
- `/rmcp-client/src/lib.rs` - MCP client
- `/mcp-types/src/lib.rs` - MCP types
