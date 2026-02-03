# Codex CLI Architecture (TypeScript)

> **Purpose**: Reference architecture for the original TypeScript implementation of Codex CLI.
>
> **Note**: This documents the legacy TypeScript implementation. The current production version is written in Rust (`codex-rs`).
>
> **Source**: `/Users/cecilia/Code/codex/codex-cli/`

---

## 1. Overview

The TypeScript Codex CLI was a React/Ink-based terminal application providing AI-assisted coding through:

- **UI Framework**: Ink (React for CLI)
- **Runtime**: Node.js
- **API**: OpenAI Responses API
- **Streaming**: Server-Sent Events
- **Sandboxing**: Seatbelt (macOS), Landlock (Linux)

---

## 2. Directory Structure

```
codex-cli/
├── src/
│   ├── cli.tsx                      # Main CLI entry point
│   ├── cli-singlepass.tsx           # Single-pass full-context mode
│   ├── app.tsx                      # Main React/Ink application
│   ├── approvals.ts                 # Approval policy logic
│   ├── format-command.ts            # Command formatting
│   ├── parse-apply-patch.ts         # Patch parsing
│   ├── text-buffer.ts               # Text buffer utilities
│   │
│   ├── components/                  # UI Components (React/Ink)
│   │   ├── chat/                    # Chat interface
│   │   │   ├── terminal-chat.tsx           # Primary chat UI
│   │   │   ├── terminal-chat-input.tsx     # User input handling
│   │   │   ├── terminal-chat-response-item.tsx
│   │   │   ├── terminal-chat-command-review.tsx
│   │   │   ├── terminal-chat-completions.tsx
│   │   │   ├── terminal-message-history.tsx
│   │   │   └── terminal-header.tsx
│   │   │
│   │   ├── onboarding/              # First-run experience
│   │   ├── select-input/            # Selection UI
│   │   └── vendor/                  # Vendored components
│   │
│   ├── hooks/                       # React hooks
│   │   ├── use-confirmation.ts      # Confirmation dialog
│   │   └── use-terminal-size.ts     # Terminal dimensions
│   │
│   └── utils/                       # Utility modules
│       ├── agent/                   # Agent execution core
│       │   ├── agent-loop.ts        # Main execution loop
│       │   ├── handle-exec-command.ts
│       │   └── sandbox/             # Sandbox implementations
│       │       ├── macos-seatbelt.ts
│       │       ├── landlock.ts
│       │       └── raw-exec.ts
│       │
│       ├── singlepass/              # Full-context editing
│       ├── storage/                 # Session persistence
│       └── logger/                  # Logging infrastructure
│
├── tests/                           # Test suite
├── bin/                             # Entry point wrapper
└── package.json
```

---

## 3. Entry Points

### Primary: `cli.tsx`

**Path**: `/codex-cli/src/cli.tsx`

**Responsibilities**:
- Parse CLI arguments (meow)
- Handle authentication (OpenAI API key)
- Load configuration from `~/.codex/`
- Initialize logging
- Route to mode (interactive, quiet, full-context)
- Set up signal handlers

**CLI Flags**:
```
--model/-m         Model selection
--provider/-p      Provider (openai, azure, ollama, etc.)
--approval-mode/-a suggest, auto-edit, full-auto
--quiet/-q         Non-interactive mode
--full-context/-f  Single-pass editing mode
--image/-i         Image inputs
--view             View past rollout
--history          Browse sessions
```

### Secondary: `cli-singlepass.tsx`

Used for "full-context" mode - loads entire repository into context for batch edits.

---

## 4. Core Components

### `app.tsx` - Main Application

**Path**: `/codex-cli/src/app.tsx`

Main React component orchestrating the UI:

```typescript
type Props = {
  prompt?: string;
  config: AppConfig;
  imagePaths?: Array<string>;
  rollout?: AppRollout;
  approvalPolicy: ApprovalPolicy;
  additionalWritableRoots: ReadonlyArray<string>;
  fullStdout: boolean;
};
```

**Responsibilities**:
- Manage session state
- Route between view modes (live chat vs past rollout)
- Handle Git safety warnings
- Integrate with `TerminalChat` component

### `terminal-chat.tsx` - Chat Interface

**Path**: `/codex-cli/src/components/chat/terminal-chat.tsx`

Primary chat interface:

```typescript
const [model, setModel] = useState<string>(config.model);
const [items, setItems] = useState<Array<ResponseItem>>([]);
const [loading, setLoading] = useState<boolean>(false);
const [approvalPolicy, setApprovalPolicy] = useState<ApprovalPolicy>();
const [overlayMode, setOverlayMode] = useState<OverlayModeType>("none");
```

**Features**:
- AgentLoop lifecycle management
- Overlay modes (model selection, approval, help, diff)
- Command confirmation flow
- Real-time streaming display

### Overlay Components

| Component | Purpose |
|-----------|---------|
| `model-overlay.tsx` | Model/provider selection |
| `approval-mode-overlay.tsx` | Approval policy switching |
| `help-overlay.tsx` | Keyboard shortcuts |
| `diff-overlay.tsx` | Git diff viewer |
| `history-overlay.tsx` | Conversation history |
| `sessions-overlay.tsx` | Past session browser |

---

## 5. AgentLoop - Execution Core

**Path**: `/codex-cli/src/utils/agent/agent-loop.ts`

The heart of the agent execution system:

```typescript
class AgentLoop {
  // Configuration
  private model: string;
  private provider: string;
  private instructions?: string;
  private approvalPolicy: ApprovalPolicy;
  
  // State management
  private transcript: Array<ResponseInputItem> = [];
  private pendingAborts: Set<string> = new Set();
  private generation = 0;
  
  // OpenAI client
  private oai: OpenAI;
  
  // Callbacks
  private onItem: (item: ResponseItem) => void;
  private onLoading: (loading: boolean) => void;
  private getCommandConfirmation: (...) => Promise<CommandConfirmation>;
}
```

### Chat Flow Sequence

```
1. User Input
   │
   ▼
2. AgentLoop.run(input)
   │
   ▼
3. OpenAI API Call (Responses API, stream: true)
   │
   ▼
4. Stream Events Processing
   ├── response.output_item.done → stageItem()
   └── response.completed → process function calls
   │
   ▼
5. Function Call Handling
   ├── shell/container.exec → handleExecCommand()
   └── apply_patch → execApplyPatch()
   │
   ▼
6. Approval Check
   ├── auto-approve → execute
   └── ask-user → getCommandConfirmation()
   │
   ▼
7. Command Execution (with/without sandbox)
   │
   ▼
8. Result → Next API Call (loop until done)
```

---

## 6. Tool Execution

### Tool Definitions

```typescript
const shellFunctionTool: FunctionTool = {
  type: "function",
  name: "shell",
  description: "Runs a shell command, and returns its output.",
  parameters: {
    type: "object",
    properties: {
      command: { type: "array", items: { type: "string" } },
      workdir: { type: "string" },
      timeout: { type: "number" },
    },
    required: ["command"],
  },
};
```

### Execution Handler

**Path**: `/codex-cli/src/utils/agent/handle-exec-command.ts`

```typescript
async function handleExecCommand(
  args: ExecInput,
  config: AppConfig,
  policy: ApprovalPolicy,
  additionalWritableRoots: ReadonlyArray<string>,
  getCommandConfirmation: (...) => Promise<CommandConfirmation>,
  abortSignal?: AbortSignal,
): Promise<HandleExecCommandResult>
```

**Flow**:
1. Check if command in `alwaysApprovedCommands` cache
2. Evaluate safety via `canAutoApprove()`
3. Based on result:
   - `auto-approve` → Execute (with/without sandbox)
   - `ask-user` → Request confirmation
   - `reject` → Return error
4. Execute via `exec()` or `execApplyPatch()`
5. Return output and metadata

---

## 7. Sandbox System

**Path**: `/codex-cli/src/utils/agent/sandbox/`

| Module | Platform | Description |
|--------|----------|-------------|
| `macos-seatbelt.ts` | macOS | Apple's Seatbelt (sandbox-exec) |
| `landlock.ts` | Linux | Landlock LSM |
| `raw-exec.ts` | All | Unsandboxed execution |
| `interface.ts` | All | Type definitions |

### Seatbelt Policy (macOS)

```scheme
(version 1)
(deny default)              ; Closed by default
(allow file-read*)          ; Read-only file operations
(allow process-exec)        ; Child processes inherit policy
(allow process-fork)
(allow file-write*          ; Limited write to writable roots
  (subpath (param "WRITABLE_ROOT_0")))
```

---

## 8. Approval System

**Path**: `/codex-cli/src/approvals.ts`

### Approval Policies

| Policy | Description |
|--------|-------------|
| `suggest` | All commands require approval except known-safe reads |
| `auto-edit` | Auto-approve file edits within writable paths |
| `full-auto` | All commands auto-approved but sandboxed |

### Safety Assessment

```typescript
function canAutoApprove(
  command: ReadonlyArray<string>,
  workdir: string | undefined,
  policy: ApprovalPolicy,
  writableRoots: ReadonlyArray<string>,
  env: NodeJS.ProcessEnv = process.env,
): SafetyAssessment
```

### Safe Commands (auto-approved in all modes)

```
Navigation:     cd, ls, pwd, true, echo
File reading:   cat, head, tail, wc, nl
Search:         grep, rg (with restrictions), find (with restrictions)
Git (read):     git status, git branch, git log, git diff, git show
Build:          cargo check
Print-only sed: sed -n <range>p
```

---

## 9. Configuration

**Path**: `/codex-cli/src/utils/config.ts`

### Config Locations

- `~/.codex/config.json` (or `.yaml`)
- `~/.codex/instructions.md` (custom instructions)
- `AGENTS.md` / `codex.md` (project documentation)

### AppConfig Type

```typescript
type AppConfig = {
  apiKey?: string;
  model: string;
  provider?: string;
  instructions: string;
  approvalMode?: AutoApprovalMode;
  fullAutoErrorMode?: FullAutoErrorMode;
  reasoningEffort?: ReasoningEffort;
  notify?: boolean;
  disableResponseStorage?: boolean;
  flexMode?: boolean;
  providers?: Record<string, ProviderConfig>;
  history?: HistoryConfig;
  tools?: ToolsConfig;
  fileOpener?: FileOpenerScheme;
};
```

---

## 10. React/Ink Dependencies

```json
{
  "ink": "^5.2.0",
  "@inkjs/ui": "^2.0.0",
  "react": "^18.2.0",
  "chalk": "^5.2.0",
  "marked": "^15.0.7",
  "marked-terminal": "^7.3.0"
}
```

### Custom Hooks

**`useConfirmation`** - Command approval dialog:
```typescript
const {
  requestConfirmation,
  confirmationPrompt,
  explanation,
  submitConfirmation,
} = useConfirmation();
```

**`useTerminalSize`** - Terminal dimensions for responsive layout.

---

## 11. TypeScript vs Rust Comparison

### Architecture Differences

| Aspect | TypeScript (`codex-cli`) | Rust (`codex-rs`) |
|--------|-------------------------|-------------------|
| **UI Framework** | Ink (React) | Ratatui (native) |
| **Concurrency** | Node.js async/await | Tokio async |
| **Package Size** | ~5MB + Node.js | ~10MB standalone |
| **Startup Time** | ~500ms | ~50ms |
| **Module Count** | ~50 TypeScript files | ~50 crates |

### Module Mapping

| TypeScript | Rust Equivalent |
|------------|-----------------|
| `src/cli.tsx` | `cli/src/main.rs` |
| `src/app.tsx` | `tui/src/app.rs` |
| `src/utils/agent/agent-loop.ts` | `core/src/codex.rs`, `core/src/codex_thread.rs` |
| `src/utils/agent/handle-exec-command.ts` | `core/src/exec.rs`, `core/src/unified_exec/` |
| `src/approvals.ts` | `core/src/safety.rs`, `core/src/command_safety/` |
| `src/utils/config.ts` | `core/src/config_loader/` |
| `src/utils/agent/sandbox/macos-seatbelt.ts` | `core/src/seatbelt.rs` |
| `src/utils/agent/sandbox/landlock.ts` | `core/src/landlock.rs` |
| `src/components/chat/terminal-chat.tsx` | `tui/src/chatwidget.rs` |

### Rust-Only Capabilities

The Rust implementation adds features not in TypeScript:

- **Windows Sandbox** (`windows-sandbox-rs/`)
- **MCP Server Mode** (`mcp-server/`)
- **Multiple TUI Implementations** (`tui/`, `tui2/`)
- **Backend/API Server** (`app-server/`)
- **Skills System** (`core/src/skills/`)
- **Native Login Flow** (`login/`)
- **OpenTelemetry** (`otel/`)
- **Process Hardening** (`process-hardening/`)
- **Starlark Policy Engine** (`execpolicy/`)

---

## 12. Key Takeaways for AOMI

Patterns from Codex applicable to AOMI:

1. **Approval Policies**: Tiered approval (suggest/auto-edit/full-auto) with safe command lists
2. **Sandbox-First**: Default to sandboxed execution, retry without on failure
3. **Streaming Architecture**: SSE-based streaming with incremental UI updates
4. **Tool Orchestration**: Separate registry/router/orchestrator concerns
5. **Config Layers**: User → Project → Defaults with profile support
6. **Session Persistence**: Rollout recording for resume/replay
