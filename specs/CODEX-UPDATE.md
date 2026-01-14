# Codex Update

- Aligned tool scheduling around session handlers and metadata-only registry; removed legacy scheduler request path.
- Moved AomiTool Rig wrapper into `aomi/crates/tools/src/wrapper.rs` and stripped wrapper/macro from `aomi-tools-v2`.
- Added session context fields (`session_id`, `namespaces`, `call_id`, `request_id`) to `AomiToolArgs` and wired injection in completion flow.
- Normalized tool call metadata to `CallMetadata` from `aomi-tools-v2` and updated ToolCompletion payloads accordingly.
- Simplified tests to register receivers directly and refreshed scheduler tests to match new handler behavior.
- Replaced completion stream tests with local tool-update parsing tests (no external API).
- Renamed `SessionToolHander` to `SessionToolHandler` and updated backend/session usage.
- Switched backend async tool test helpers to emit `tool_completion` events via `push_tool_update`.
- Simplified `process_tool_call` to only inject `session_id`, call rig tool, and return the Value as a ToolStream.
- Reduced `AomiToolArgs` to require only `session_id` + flattened args; wrapper now derives namespace/async from the tool itself.
