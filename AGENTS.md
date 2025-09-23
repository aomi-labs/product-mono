# Repository Guidelines

## Project Structure & Module Organization
- `chatbot/` houses the Rust workspace. `crates/agent` exposes the on-chain tools, `crates/mcp-server` wraps MCP integrations, and `crates/rag` handles embeddings and document indexing.
- `chatbot/bin/backend` serves the HTTP API; `chatbot/bin/tui` renders the terminal UI. Shared assets, including Uniswap docs, live under `chatbot/documents/`.
- `frontend/` is a Next.js client for the chat experience (components in `src/`, assets in `public/`).
- `scripts/` orchestrates environment bootstrapping; `dev.sh` and `prod.sh` start Anvil, the MCP server, backend, and frontend using `config.yaml`.

## Build, Test & Development Commands
- `./scripts/dev.sh` launches all local services with hot reload and dummy docs disabled; stop via `scripts/kill-all.sh`.
- `cargo run -p backend -- --no-docs` starts the API only; omit `--no-docs` to preload Uniswap material.
- `cargo run -p mcp-server` bootstraps the MCP worker; add the JSON network payload emitted by `load_config.py`.
- `npm run dev` (from `frontend/`) serves the Next.js UI; `npm run build` produces the optimized bundle.

## Coding Style & Naming Conventions
- Format Rust with `cargo fmt --all` (configured for 120 character lines via `chatbot/rustfmt.toml`); lint with `cargo clippy --all-targets -- -D warnings`.
- Prefer descriptive `snake_case` for modules/functions and `PascalCase` for types; keep async tasks and tool structs in dedicated files.
- Frontend code follows Next.js defaults: TypeScript, JSX components in `PascalCase`, utility modules in `camelCase`. Run `npm run lint` before pushing.

## Testing Guidelines
- Execute `cargo test` at the workspace root; scope runs with `cargo test -p rag` when iterating on vector store logic.
- When touching async services, add unit tests alongside the module (`mod tests` blocks already exist under `chatbot/crates/**`).
- Verify API health after major changes with `curl http://localhost:$BACKEND_PORT/health` once the backend is running.

## Commit & Pull Request Guidelines
- Match the existing history: short, imperative commit subjects (`fix anvil + cleanup old FE`, `plans`). Group unrelated changes into separate commits.
- Every PR should describe the change, list manual/automated test output, and link the tracking issue. Include screenshots or terminal snippets when altering the frontend or CLI UX.
- Rebase onto `main` before requesting review, and ensure lint/tests pass in CI logs.

## Environment & Secrets
- Copy `.env.template` to `.env.dev` or `.env.prod`, provide keys (Anthropic required), and avoid committing `.env.*`.
- Use `scripts/load_config.py <env>` to validate `config.yaml` edits; it exports port variables and the network JSON consumed by the MCP server.
