# Repository Guidelines

## Project Structure & Module Organization
- `frontend/` hosts the Next.js UI. Route handlers live in `src/app`, shared UI primitives in `src/components`, data helpers in `src/lib`, and Tailwind settings in `tailwind.config.js`.
- `chatbot/` contains the Rust services (`bin/backend` HTTP API, `bin/tui` terminal chat) with shared crates in `crates/`.
- `scripts/` holds shell automation (`dev.sh`, `prod.sh`, `start-backend.sh`, `kill-all.sh`). These wrap config loading (`load_config.py`) and orchestrate Anvil, backend, and frontend processes.
- Environment files (`.env.dev`, `.env.prod`) pair with `config.yaml` for ports and API keys; seed them from `.env.template`.

## Build, Test, and Development Commands
- `cd frontend && npm install` installs UI dependencies.
- `npm run dev` launches the Next.js dev server (Turbopack via `npm run dev:turbo` if you need faster refreshes).
- `npm run build` creates a production bundle; pair it with `npm run start` for local previews.
- `npm run lint` runs ESLint with the repo config (`eslint.config.mjs`). Fix issues before pushing.
- `./scripts/dev.sh` boots the full stack (Anvil → Rust backend → frontend) after validating config; use this for end-to-end testing.
- `./scripts/start-backend.sh` is a lighter option when you only need the Rust backend running.

## Coding Style & Naming Conventions
- TypeScript/TSX uses 2-space indentation and ES modules. Components stay PascalCase (`ChatPanel.tsx`), hooks/utilities camelCase (`useChatSession`).
- Favor Tailwind utility classes over custom CSS; `class-variance-authority` helpers belong beside the component. Run `npm run lint` and keep editors linting on save.

## Testing Guidelines
- UI smoke checks live in `test-frontend.js` (Playwright). With the dev stack running, execute `node test-frontend.js` to capture screenshots and console logs.
- Keep component-level tests colocated in `src/__tests__` if you add Jest/Vitest—mirror the file name and append `.test.tsx`.
- Validate Rust services with `cd chatbot && cargo test`, and keep manual wallet/SSE notes in PRs when flows change.

## Commit & Pull Request Guidelines
- Follow the existing history: short, present-tense subjects that name the touched area (`frontend: connect SSE`, `backend: tighten retries`). Group related changes rather than stacking many one-line commits.
- PRs should include: objective summary, linked issue or ticket, test evidence (`npm run lint`, `cargo test`, Playwright output), and screenshots/GIFs for UI changes.
- Flag configuration updates (`config.yaml`, `.env.*`) explicitly and note any required secrets so reviewers can sync their environment.
