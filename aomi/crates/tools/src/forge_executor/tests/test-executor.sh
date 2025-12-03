#!/usr/bin/env bash
set -euo pipefail

# Simple harness to execute ForgeExecutor fixtures end-to-end.
# Requires: anvil, cargo, BAML_API_KEY, ETHERSCAN_API_KEY.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
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

if [[ -z "${BAML_API_KEY:-}" ]]; then
  echo "BAML_API_KEY must be set for script generation." >&2
  exit 1
fi

if [[ -z "${ETHERSCAN_API_KEY:-}" ]]; then
  echo "ETHERSCAN_API_KEY must be set for contract source fetching." >&2
  exit 1
fi

if [[ ! -f "$FOUNDRY_TOML" ]]; then
  echo "Missing foundry config at $FOUNDRY_TOML" >&2
  exit 1
fi

FORK_URL="$(python - <<PY
import pathlib, tomllib
from sys import exit

toml_path = pathlib.Path("$FOUNDRY_TOML")
data = tomllib.loads(toml_path.read_text())
eth_rpc = (
    data.get("profile", {})
        .get("default", {})
        .get("eth_rpc_url")
    or data.get("eth_rpc_url")
)
if not eth_rpc:
    exit("eth_rpc_url missing in foundry.toml")
print(eth_rpc)
PY
)"

ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_ARGS=(--fork-url "$FORK_URL" --port "$ANVIL_PORT" --silent)

echo "Starting anvil fork on port ${ANVIL_PORT}..."
anvil "${ANVIL_ARGS[@]}" >/tmp/aomi-anvil.log 2>&1 &
ANVIL_PID=$!
trap 'kill ${ANVIL_PID} >/dev/null 2>&1 || true' EXIT

export AOMI_FORK_RPC="http://127.0.0.1:${ANVIL_PORT}"
export RUST_LOG="${RUST_LOG:-info}"

echo "Running ForgeExecutor fixture workflows from ${FIXTURE_DIR}"
(cd "$TOOLS_DIR" && cargo test -p aomi-tools forge_executor::tests::run_fixtures::test_fixture_workflows_via_tools -- --ignored --nocapture)
