# IDEA.md

## Core Idea

- **Expose ChiselSession (or its components) as MCP resources.**
- **Dynamically fetch smart contracts on-chain, obtain their ABI and source code, and expose callable functions as tools/resources.**
- **Handle proxy contracts by detecting non-callable interfaces and fetching the correct implementation ABI/source.**
- **Append fetched source code to the session (e.g., `top_level_code`), so the LLM client can see and reason about the contract.**
- **Let the LLM (or user) compose `run_code` to interact with the contract, using the session as context.**

## Chisel Session Model

- Chisel is **session-based**, not file-based. Sessions are saved as JSON (e.g., `chisel-1.json`) and loaded by ID.
- Each session contains:
  - `global_code`, `top_level_code`, `run_code` (Solidity code blocks)
  - Full Foundry/Chisel config
  - Session metadata (ID, file name, contract name, etc.)
- Sessions can be loaded, saved, and inspected via the Chisel REPL (e.g., `chisel chisel load 1`, `!source`).

## Workflow Concept

1. **User/LLM requests a contract by address.**
2. **Backend fetches ABI/source (handles proxy logic if needed).**
3. **ABI and source are appended to the session's code blocks.**
4. **LLM is prompted with the updated session and can now compose `run_code` to interact with the contract.**
5. **User/LLM can call MCP tools to execute, simulate, or analyze the contract.**

## Key Discoveries

- Chisel does **not** support running arbitrary script files directly (no `chisel run <file>` or `chisel <file>`). All interaction is via the REPL and session system.
- Sessions are loaded by ID, not by filename. The session file is a JSON object with all state needed to resume work.
- The `.chisel` extension is a convention, not a requirement or enforced by the CLI.
- The Chisel REPL can export session source, but not import or run arbitrary scripts directly.

## Potential Challenges & Risks

- **Session state consistency:** Race conditions if multiple agents/users modify the same session.
- **ABI/source availability:** Not all contracts are verified or have ABIs; proxies can be complex.
- **Session bloat:** Appending many large contracts can make sessions unwieldy and exceed LLM context limits.
- **LLM prompt management:** Too much code/context can confuse the LLM or hit token limits.
- **Security:** Arbitrary code/ABI injection could be abused.
- **LLM correctness:** LLM may generate invalid or unsafe code.
- **Debugging and UX:** Errors may be hard to diagnose for users/LLMs.

## Next Steps (for future design)

- Define MCP resource/tool interfaces for Chisel sessions and contract fetching.
- Implement robust proxy detection and ABI/source fetching logic.
- Design session management for multi-user/agent scenarios.
- Plan for context/pruning strategies to keep sessions and LLM prompts manageable.
- Add validation, error handling, and user feedback mechanisms.

---

**This document records the concept and discoveries so far. It is not a full design, but a foundation for future development.** 