#!/usr/bin/env bash
set -euo pipefail

# Simple harness to execute ForgeExecutor fixtures end-to-end.
# Requires: anvil, cargo, ETHERSCAN_API_KEY, providers.toml.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
PROJECT_ROOT="$(cd "$TOOLS_DIR/../../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fixtures"
FOUNDRY_TOML="$TOOLS_DIR/src/contract/foundry.toml"
PROVIDERS_TOML="$PROJECT_ROOT/providers.toml"
# BAML sources directory
BAML_SRC_DIR="$PROJECT_ROOT/aomi/crates/baml/baml_src"
BAML_CLIENT_DIR="$PROJECT_ROOT/aomi/crates/baml/baml_client"
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

if [[ ! -f "$PROVIDERS_TOML" ]]; then
  echo "providers.toml must exist at $PROVIDERS_TOML." >&2
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
FORK_URL="$(python3 - "$PROVIDERS_TOML" <<'PY'
import sys
from pathlib import Path

try:
    import tomllib  # py3.11
except Exception:
    import tomli as tomllib  # type: ignore

path = Path(sys.argv[1])
data = tomllib.loads(path.read_text())
anvil = data.get("anvil-instances", {})
external = data.get("external", {})

for name in ("ethereum", "mainnet"):
    if name in anvil and "fork_url" in anvil[name]:
        print(anvil[name]["fork_url"])
        sys.exit(0)
    if name in external and "rpc_url" in external[name]:
        print(external[name]["rpc_url"])
        sys.exit(0)

for cfg in anvil.values():
    if "fork_url" in cfg:
        print(cfg["fork_url"])
        sys.exit(0)

for cfg in external.values():
    if "rpc_url" in cfg:
        print(cfg["rpc_url"])
        sys.exit(0)

sys.exit(1)
PY
)"

if [[ -z "${FORK_URL:-}" ]]; then
  echo "No fork_url/rpc_url found in $PROVIDERS_TOML." >&2
  exit 1
fi

ANVIL_ARGS=(--fork-url "$FORK_URL" --port "$ANVIL_PORT" --block-time 1 --silent)
PROVIDERS_BACKUP=""

regenerate_baml_client() {
  echo "Regenerating BAML client in $BAML_CLIENT_DIR..."
  # Use npm mirror to avoid 500 errors from npmjs.org
  (cd "$BAML_CLIENT_DIR" && npm config set registry https://registry.npmmirror.com && "$BAML_CLI_BIN" generate --from "$BAML_SRC_DIR")
  echo "✅ BAML client regenerated"
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

if [[ -f "$PROVIDERS_TOML" ]]; then
  PROVIDERS_BACKUP="$(mktemp /tmp/providers.toml.XXXXXX)"
  cp "$PROVIDERS_TOML" "$PROVIDERS_BACKUP"
fi

echo "Starting anvil fork on port ${ANVIL_PORT}..."
anvil "${ANVIL_ARGS[@]}" >/tmp/aomi-anvil.log 2>&1 &
ANVIL_PID=$!
trap '[[ -n "${ANVIL_PID:-}" ]] && kill ${ANVIL_PID} >/dev/null 2>&1 || true; [[ -n "${BAML_PID:-}" ]] && kill ${BAML_PID} >/dev/null 2>&1 || true; if [[ -n "${PROVIDERS_BACKUP:-}" ]]; then mv "${PROVIDERS_BACKUP}" "${PROVIDERS_TOML}"; fi' EXIT

start_baml

cat > "$PROVIDERS_TOML" <<EOF
[external]
local = { chain_id = 1, rpc_url = "http://127.0.0.1:${ANVIL_PORT}" }
EOF
export RUST_LOG="${RUST_LOG:-debug}"

# Bypass proxy for localhost to avoid 502 errors
export no_proxy="localhost,127.0.0.1"
export NO_PROXY="localhost,127.0.0.1"

echo "Running ForgeExecutor fixture workflows from ${FIXTURE_DIR}"
echo "RUST_LOG=${RUST_LOG}"
(cd "$TOOLS_DIR" && cargo test -p aomi-scripts forge_executor::tests::run_fixtures::test_fixture_workflows_via_tools -- --ignored --nocapture)
