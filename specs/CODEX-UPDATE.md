## Update
- Introduced `AomiToolArgs` as a trait for user-only tool args and added `add_topic()` schema helper; default `AomiTool::parameters_schema` now uses `Self::Args::schema()`.
- Updated core tool execution signatures to take `ToolCallCtx` (session + metadata) and removed inline `topic` usage in runtime args.
- Refactored tool parameter structs to drop `topic` fields and added `AomiToolArgs` impls with schema descriptions copied from `_register.rs`.
- Updated tool tests and examples to remove `topic` from parameter structs.
- Restored `RuntimeEnvelope<T>` and wired completion to wrap Aomi tool calls with `{ ctx, args }` while leaving MCP tools unchanged.
- Added an Aomi tool name->namespace map to `CoreApp`/`CoreState` so completion can detect Aomi tools without touching the scheduler.
- Threaded `aomi_tool_namespaces` through app/session/eval entrypoints and updated `CallMetadata::new` call sites for the new namespace field.

## Next
- Remove any remaining compile errors and run the tool/backend tests.
