#!/bin/bash

# test-mcp.sh - Launch the MCP server and smoke-test each network service.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ENVIRONMENT="${FORGE_ENV:-dev}"

if [[ $# -gt 0 ]]; then
  case "$1" in
    -h|--help)
      cat <<USAGE
Usage: $(basename "$0") [env]
  env    Target config environment (default: dev). Examples: dev, prod
USAGE
      exit 0
      ;;
    *)
      ENVIRONMENT="$1"
      shift
      ;;
  esac
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "❌ cargo not found. Install Rust toolchain first: https://rustup.rs" >&2
  exit 1
fi

if [[ -f "$PROJECT_ROOT/.venv/bin/activate" ]]; then
  # shellcheck source=/dev/null
  source "$PROJECT_ROOT/.venv/bin/activate"
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "❌ python3 not found. Install Python 3 to continue." >&2
  exit 1
fi

# Load port configuration into the environment
CONFIG_EXPORTS=$(python3 "$SCRIPT_DIR/load_config.py" "$ENVIRONMENT" --export-only)
if [[ -z "$CONFIG_EXPORTS" ]]; then
  echo "❌ Failed to load configuration exports." >&2
  exit 1
fi
# shellcheck disable=SC2086
eval "$CONFIG_EXPORTS"

NETWORK_JSON=$(python3 "$SCRIPT_DIR/load_config.py" "$ENVIRONMENT" --network-urls-only | tr -d '\n')
if [[ -z "$NETWORK_JSON" ]]; then
  echo "❌ Failed to resolve network URLs JSON." >&2
  exit 1
fi

echo "NETWORK_JSON: $NETWORK_JSON"

ACCESS_HOST="$MCP_SERVER_HOST"
if [[ "$ACCESS_HOST" == "0.0.0.0" ]]; then
  ACCESS_HOST="127.0.0.1"
fi

# Determine which port each network will run on (matching server ordering)
NETWORK_PORTS=$(NETWORK_JSON="$NETWORK_JSON" MCP_PORT="$MCP_SERVER_PORT" python3 <<'PY'
import json
import os

network_json = os.environ.get("NETWORK_JSON", "{}")
base_port = int(os.environ.get("MCP_PORT", "5000"))

try:
    data = json.loads(network_json)
except json.JSONDecodeError:
    data = {}

if not data:
    data = {"testnet": "http://127.0.0.1:8545"}

items = sorted(data.items(), key=lambda item: item[0])
if any(name == "testnet" for name, _ in items):
    items = [item for item in items if item[0] != "testnet"]
    items.insert(0, ("testnet", "http://127.0.0.1:8545"))

for index, (name, _) in enumerate(items):
    port = base_port + index
    print(f"{name}:{port}")
PY
)

if [[ -z "${NETWORK_PORTS//[[:space:]]/}" ]]; then
  echo "❌ No networks resolved from configuration." >&2
  exit 1
fi

echo "🌐 Checking MCP networks:"
while IFS=: read -r net_name net_port; do
  [[ -z "$net_name" ]] && continue
  echo "  • $net_name → $net_port"
done <<< "$NETWORK_PORTS"

cd "$PROJECT_ROOT/chatbot"

cargo build -p mcp-server >/dev/null

cargo run -p mcp-server --bin foameow-mcp-server  -- "$NETWORK_JSON" &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

wait_for_port() {
  local host=$1
  local port=$2
  local retries=30
  for ((i=0; i<retries; i++)); do
    if nc -z "$host" "$port" 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  echo "❌ Timed out waiting for $host:$port" >&2
  return 1
}

while IFS=: read -r network port; do
  [[ -z "$network" ]] && continue
  echo "⏳ Waiting for MCP service $network on port $port..."
  wait_for_port "$ACCESS_HOST" "$port"
  echo "🚀 Testing MCP service for $network"
  if ! OUTPUT=$(cargo run -p mcp-server --bin mcp-smoke-client -- "http://$ACCESS_HOST:$port" "$network" 2>&1); then
    echo "$OUTPUT"
    exit 1
  fi
  echo "$OUTPUT"
done <<< "$NETWORK_PORTS"

echo "✅ All MCP network services responded successfully."
