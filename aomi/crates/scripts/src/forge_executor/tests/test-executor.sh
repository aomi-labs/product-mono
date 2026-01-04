#!/usr/bin/env bash
set -euo pipefail

# Simple harness to execute ForgeExecutor fixtures end-to-end.
# Requires: anvil, cargo, ETHERSCAN_API_KEY, ETH_RPC_URL, ANTHROPIC_API_KEY or OPENAI_API_KEY.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
PROJECT_ROOT="$(cd "$TOOLS_DIR/../../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures"
FOUNDRY_TOML="$TOOLS_DIR/src/contract/foundry.toml"

if ! command -v anvil >/dev/null 2>&1; then
  echo "anvil is required (cargo install foundry-cli)." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to run the executor tests." >&2
  exit 1
fi

if [[ -z "${ETHERSCAN_API_KEY:-}" ]]; then
  echo "ETHERSCAN_API_KEY must be set for contract source fetching." >&2
  exit 1
fi

if [[ -z "${ETH_RPC_URL:-}" ]]; then
  echo "ETH_RPC_URL must be set for forking mainnet." >&2
  exit 1
fi

if [[ -z "${ANTHROPIC_API_KEY:-}" && -z "${OPENAI_API_KEY:-}" ]]; then
  echo "ANTHROPIC_API_KEY or OPENAI_API_KEY must be set for BAML native FFI." >&2
  exit 1
fi

if [[ ! -f "$FOUNDRY_TOML" ]]; then
  echo "Missing foundry config at $FOUNDRY_TOML" >&2
  exit 1
fi

ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_ARGS=(--fork-url "$ETH_RPC_URL" --port "$ANVIL_PORT" --block-time 1 --silent)

echo "Starting anvil fork on port ${ANVIL_PORT}..."
anvil "${ANVIL_ARGS[@]}" >/tmp/aomi-anvil.log 2>&1 &
ANVIL_PID=$!
trap '[[ -n "${ANVIL_PID:-}" ]] && kill ${ANVIL_PID} >/dev/null 2>&1 || true' EXIT

# Override ETH_RPC_URL to use local anvil fork
export ETH_RPC_URL="http://127.0.0.1:${ANVIL_PORT}"
export RUST_LOG="${RUST_LOG:-debug}"

# Bypass proxy for localhost to avoid 502 errors
export no_proxy="localhost,127.0.0.1"
export NO_PROXY="localhost,127.0.0.1"

echo "Running ForgeExecutor fixture workflows from ${FIXTURE_DIR}"
echo "RUST_LOG=${RUST_LOG}"
(cd "$TOOLS_DIR" && cargo test -p aomi-scripts forge_executor::tests::run_fixtures::test_fixture_workflows_via_tools -- --ignored --nocapture)
