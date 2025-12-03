# System Bus Plan

> Separate system-side signals from the LLM/user chat stream so wallet, backend, and external events can flow without polluting chat history.

---

## Goals
- Decouple system notifications from LLM chat flow.
- Dual buffers that `sync_state()` can read independently: `ChatCommand` buffer and `SystemEvent` buffer.
- Make wallet lifecycle explicit: `WalletTxRequest` (agent → system buffer) and `WalletTxResponse` (system → UI/LLM opt-in).
- Keep UI and LLM free to consume only the variants they need.

## Event Model
- New `SystemEvent` enum (non-exhaustive):
  - `SystemNotice(String)` — general info
  - `SystemError(String)` — non-LLM errors
  - `BackendConnecting(String)`
  - `BackendConnected`
  - `MissingApiKey`
  - `WalletTxRequest { payload: Value }`
  - `WalletTxResponse { status: String, tx_hash: Option<String>, detail: Option<String> }`
  - `UserRequest { kind: String, payload: Value }` (e.g., direct ABI fetch)
  - `UserResponse { kind: String, payload: Value }`
- Extensible for other system-side events (RPC status, ABI cache refresh, etc.).

## Buffers and Access Pattern
- Keep the `ChatCommand` channel for the chat stream only; remove all system variants from `ChatCommand`.
- Add a `SystemEventBuffer` shared queue (e.g., `Arc<Mutex<VecDeque<SystemEvent>>>` with a small helper API). System components push into it; `SessionState` drains it explicitly.
- SessionState owns:
  - `receiver_from_llm` (existing ChatCommand stream)
  - `system_event_queue` handle for drain/push (no `mpsc` for system events)
- `sync_state()` on the UI should pull from both buffers so UI can render chat messages and system notices independently. Agents never see system events unless we inject them.

## Routing Rules
- Agent → System: when LLM emits wallet tool call, `handle_wallet_transaction` writes `SystemEvent::WalletTxRequest` into the system buffer (no ChatCommand pollution).
- System → Agent/UI: completion of wallet actions or external fetch emits `SystemEvent::WalletTxResponse`/`UserResponse`; SessionState may inject into LLM history only when needed.
- User → System: direct API endpoints enqueue `UserRequest` without involving LLM (e.g., fetch ABI).
- System → User: SessionState drains system buffer and appends UI-facing system messages; also updates session state flags (e.g., pending wallet tx).

## SessionState Changes
- Add `system_event_queue: SystemEventQueueHandle` (drainable queue, not a channel).
- In `update_state()`, keep the existing `while let Ok(...)` loop for `ChatCommand`, but remove all system variants from `ChatCommand`.
- After the `ChatCommand` loop, run a second drain loop over `system_event_queue`:
  - UI-only: append `MessageSender::System` (e.g., notices/errors).
  - Agent-needed: append to agent history with a clear wrapper (e.g., `[[SYSTEM:...]]`) before the next completion tick.
  - State-only: update flags (`pending_wallet_tx`, connection status).
- `sync_state()` returns both buffers; UI can merge or render separately. Agent history gets only the system events we explicitly inject.

## UI & LLM Consumption
- UI: `sync_state()` reads both buffers; system notices and wallet prompts come from the system buffer.
- LLM: only sees system events explicitly injected into history (wallet result, external data). Do not auto-inject generic notices.

## Integration Points
- `crates/chat/src/completion.rs`: accept a system buffer handle; `handle_wallet_transaction` writes `SystemEvent::WalletTxRequest` into it instead of yielding `ChatCommand::WalletTransactionRequest`.
- `crates/backend/src/session.rs`: hold a system event queue handle; process ChatCommands first, then drain system events into UI/state and optional LLM injections.
- `ToolScheduler`/`ForgeExecutor`: clone/push to the shared system buffer for progress/results without blocking the LLM stream.
- HTTP endpoints: accept direct system actions (e.g., ABI fetch) and enqueue `UserRequest`.

## Migration Steps
1) Define `SystemEvent` + `SystemEventQueue` helpers (shared queue interface; push/drain).
2) Inject the queue handle into ChatApp/SessionManager/SessionState constructors (shared queue cloned where needed).
3) Update `stream_completion` (and wallet handler) to accept the queue handle and route system signals there; delete system variants from `ChatCommand`.
4) Update `SessionState::update_state` to process `ChatCommand` first, then drain `system_event_queue` in a second loop for UI/state/optional LLM injection.
5) Wire wallet flow: `WalletTxRequest` → system buffer → UI pending flag; `WalletTxResponse` → UI + optional LLM injection.
6) Expose any needed endpoints for user→system requests.

## Open Questions
- Which SystemEvents should auto-inject into LLM history vs. require explicit caller choice?
- Do we need persistence for SystemEvents (e.g., crash recovery), or is in-memory sufficient?
- Should UI subscribe separately to SystemBus (SSE) or keep piggybacking on SessionResponse?
