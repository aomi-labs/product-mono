#!/usr/bin/env bash
set -euo pipefail

# Simple harness to execute ForgeExecutor fixtures end-to-end.
# Requires: anvil, cargo, ETHERSCAN_API_KEY, ETH_RPC_URL.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
PROJECT_ROOT="$(cd "$TOOLS_DIR/../../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures"
FOUNDRY_TOML="$TOOLS_DIR/src/contract/foundry.toml"
# BAML sources directory
BAML_SRC_DIR="$TOOLS_DIR/src/baml/baml_src"
BAML_CLIENT_DIR="$TOOLS_DIR/src/baml"
# Pinned fork snapshot to speed up repeated runs
STATE_FILE="$SCRIPT_DIR/block-23946169.json"
# Set USE_SNAPSHOT=1 to load the pre-dumped fork state; default is live fork only
USE_SNAPSHOT="${USE_SNAPSHOT:-0}"
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

if [[ -z "${ETH_RPC_URL:-}" ]]; then
  echo "ETH_RPC_URL must be set for forking mainnet." >&2
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

ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_ARGS=(
  --fork-url "$ETH_RPC_URL"
  --fork-block-number 23946169
  --port "$ANVIL_PORT"
  --silent
)
if [[ "$USE_SNAPSHOT" == "1" ]]; then
  if [[ ! -f "$STATE_FILE" ]]; then
    echo "Missing fork snapshot at $STATE_FILE (USE_SNAPSHOT=1)" >&2
    exit 1
  fi
  ANVIL_ARGS=(--load-state "$STATE_FILE" "${ANVIL_ARGS[@]}")
fi

regenerate_baml_client() {
  if [[ "${SKIP_BAML_GENERATE:-0}" == "1" ]]; then
    echo "Skipping BAML client regeneration (SKIP_BAML_GENERATE=1)"
    return
  fi

  # Prefer Homebrew's OpenJDK if installed but not on PATH early enough
  if [[ -x "/opt/homebrew/opt/openjdk/bin/java" ]]; then
    PATH="/opt/homebrew/opt/openjdk/bin:${PATH}"
  fi

  if ! command -v java >/dev/null 2>&1; then
    echo "java not found; skipping BAML client regeneration and using existing baml_client" >&2
    return
  fi

  echo "Regenerating BAML client in $BAML_CLIENT_DIR..."
  # Use npm mirror to avoid 500 errors from npmjs.org
  (cd "$BAML_CLIENT_DIR" && npm config set registry https://registry.npmmirror.com && "$BAML_CLI_BIN" generate)
  echo "✅ BAML client regenerated"
}

start_anvil() {
  # Kill existing anvil processes on the port if running
  if lsof -ti:"$ANVIL_PORT" >/dev/null 2>&1; then
    echo "Killing existing anvil process on port ${ANVIL_PORT}..."
    lsof -ti:"$ANVIL_PORT" | xargs -r kill -9 2>/dev/null || true
    sleep 1
  fi

  echo "Starting anvil fork on port ${ANVIL_PORT}..."
  if [[ "$USE_SNAPSHOT" == "1" ]]; then
    echo "Using snapshot: ${STATE_FILE} @ block 23946169"
  else
    echo "Using live fork (no snapshot) @ block 23946169"
  fi

  # CRITICAL: Start anvil with clean environment (no proxy)
  # Proxy environment variables cause Anvil/Foundry Backend::spawn(fork) to deadlock
  env -i \
    PATH="$PATH" \
    HOME="$HOME" \
    USER="$USER" \
    SHELL="$SHELL" \
    anvil "${ANVIL_ARGS[@]}" >/tmp/aomi-anvil.log 2>&1 &
  ANVIL_PID=$!
  echo "Anvil PID: $ANVIL_PID (started without proxy env vars)"

  # Wait for anvil to be ready
  for i in {1..30}; do
    if curl -s -X POST http://127.0.0.1:${ANVIL_PORT}/ \
      -H "Content-Type: application/json" \
      -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
      >/dev/null 2>&1; then
      echo "✅ Anvil ready on port ${ANVIL_PORT}"
      return
    fi
    sleep 1
  done

  echo "Anvil did not start on port ${ANVIL_PORT}" >&2
  echo "Last 20 lines of anvil log:"
  tail -n 20 /tmp/aomi-anvil.log >&2
  exit 1
}

start_baml() {
  # Kill existing BAML server on the port if running
  if lsof -ti:"$BAML_SERVER_PORT" >/dev/null 2>&1; then
    echo "Killing existing process on port ${BAML_SERVER_PORT}..."
    lsof -ti:"$BAML_SERVER_PORT" | xargs -r kill -9 2>/dev/null || true
    sleep 1
  fi

  if ! command -v "$BAML_CLI_BIN" >/dev/null 2>&1; then
    echo "baml-cli not found in PATH; install with: npm install -g @boundaryml/baml" >&2
    exit 1
  fi

  # Regenerate BAML client before starting server
  regenerate_baml_client

  echo "Starting BAML server from $BAML_SRC_DIR on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}"
  "$BAML_CLI_BIN" serve --from "$BAML_SRC_DIR" --port "$BAML_SERVER_PORT" >/tmp/aomi-baml.log 2>&1 &
  BAML_PID=$!
  echo "BAML server PID: $BAML_PID"

  # Wait for server to be ready
  for i in {1..30}; do
    if curl -s "http://${BAML_SERVER_HOST}:${BAML_SERVER_PORT}/_debug/ping" >/dev/null 2>&1; then
      echo "✅ BAML server ready (ping successful)"
      return
    fi
    sleep 1
  done

  echo "BAML server did not start on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}" >&2
  echo "Last 20 lines of BAML log:"
  tail -n 20 /tmp/aomi-baml.log >&2
  exit 1
}

ANVIL_PID=""
trap '[[ -n "${ANVIL_PID:-}" ]] && kill ${ANVIL_PID} >/dev/null 2>&1 || true; [[ -n "${BAML_PID:-}" ]] && kill ${BAML_PID} >/dev/null 2>&1 || true' EXIT

start_anvil
start_baml

# Override ETH_RPC_URL to use local anvil fork
export ETH_RPC_URL="http://127.0.0.1:${ANVIL_PORT}"
export RUST_LOG="${RUST_LOG:-debug}"
# Avoid expensive binary search gas estimation during tests
export AOMI_SKIP_GAS_ESTIMATION="${AOMI_SKIP_GAS_ESTIMATION:-1}"

# CRITICAL: Unset proxy variables for test execution to prevent Backend::spawn(fork) from hanging
# The test itself spawns forked backends that must not use the system proxy
unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY all_proxy ALL_PROXY

# Bypass proxy for localhost to avoid 502 errors
export no_proxy="localhost,127.0.0.1"
export NO_PROXY="localhost,127.0.0.1"

echo "Running ForgeExecutor fixture workflows from ${FIXTURE_DIR}"
echo "RUST_LOG=${RUST_LOG}"
echo "Proxy vars unset to prevent Backend::spawn deadlock"
(cd "$TOOLS_DIR" && cargo test -p aomi-tools forge_executor::tests::run_fixtures::test_fixture_workflows_via_tools -- --ignored --nocapture)
