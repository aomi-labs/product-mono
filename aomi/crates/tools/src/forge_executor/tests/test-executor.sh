#!/usr/bin/env bash
set -euo pipefail

# Simple harness to execute ForgeExecutor fixtures end-to-end.
# Requires: anvil, cargo, BAML_API_KEY, ETHERSCAN_API_KEY.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
PROJECT_ROOT="$(cd "$TOOLS_DIR/../../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures"
FOUNDRY_TOML="$TOOLS_DIR/src/contract/foundry.toml"
# Default to the tools BAML sources (same layout as dev.sh but under crates/tools)
BAML_SRC_DIR="${BAML_SRC_DIR:-$TOOLS_DIR/src/baml/baml_src}"
# Hardcode BAML to local host/port for these tests
BAML_SERVER_HOST="127.0.0.1"
BAML_SERVER_PORT="2024"
BAML_CLI_BIN="${BAML_CLI_BIN:-baml-cli}"
BAML_PID=""

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

export BAML_SERVER_URL="http://${BAML_SERVER_HOST}:${BAML_SERVER_PORT}"

if [[ ! -f "$FOUNDRY_TOML" ]]; then
  echo "Missing foundry config at $FOUNDRY_TOML" >&2
  exit 1
fi

if [[ ! -d "$BAML_SRC_DIR" ]]; then
  echo "Missing BAML sources at $BAML_SRC_DIR" >&2
  exit 1
fi

# Read eth_rpc_url from foundry.toml using POSIX tools (no Python dependency).
# Matches a line like: eth_rpc_url = "https://eth.llamarpc.com"
FORK_URL="$(
  grep -E '^[[:space:]]*eth_rpc_url[[:space:]]*=' "$FOUNDRY_TOML" | \
    head -n 1 | \
    sed -E 's/^[^=]*=[[:space:]]*"([^"]*)".*/\1/'
)"

if [[ -z "${FORK_URL:-}" ]]; then
  echo "eth_rpc_url missing or malformed in $FOUNDRY_TOML" >&2
  exit 1
fi

ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_ARGS=(--fork-url "$FORK_URL" --port "$ANVIL_PORT" --silent)

start_baml() {
  if nc -z "$BAML_SERVER_HOST" "$BAML_SERVER_PORT" 2>/dev/null; then
    echo "✅ BAML server already running on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}"
    return
  fi

  if ! command -v "$BAML_CLI_BIN" >/dev/null 2>&1; then
    echo "baml-cli not found in PATH; install with: npm install -g @boundaryml/baml" >&2
    exit 1
  fi

  echo "Starting BAML server from $BAML_SRC_DIR on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT} (no password)"
  "$BAML_CLI_BIN" serve --from "$BAML_SRC_DIR" --port "$BAML_SERVER_PORT" &
  BAML_PID=$!

  for _ in {1..30}; do
    if nc -z "$BAML_SERVER_HOST" "$BAML_SERVER_PORT" 2>/dev/null; then
      echo "✅ BAML server ready"
      return
    fi
    sleep 1
  done

  echo "BAML server did not start on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}" >&2
  exit 1
}

echo "Starting anvil fork on port ${ANVIL_PORT}..."
anvil "${ANVIL_ARGS[@]}" >/tmp/aomi-anvil.log 2>&1 &
ANVIL_PID=$!
trap '[[ -n "${ANVIL_PID:-}" ]] && kill ${ANVIL_PID} >/dev/null 2>&1 || true; [[ -n "${BAML_PID:-}" ]] && kill ${BAML_PID} >/dev/null 2>&1 || true' EXIT

start_baml

export AOMI_FORK_RPC="http://127.0.0.1:${ANVIL_PORT}"
export RUST_LOG="${RUST_LOG:-info}"

echo "Running ForgeExecutor fixture workflows from ${FIXTURE_DIR}"
(cd "$TOOLS_DIR" && cargo test -p aomi-tools forge_executor::tests::run_fixtures::test_fixture_workflows_via_tools -- --ignored --nocapture)
