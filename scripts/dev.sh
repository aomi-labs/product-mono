#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Load API keys (single source of truth)
ENV_FILE="$PROJECT_ROOT/.env.dev"
if [[ -f "$ENV_FILE" ]]; then
  echo "🔑 Loading API keys from $ENV_FILE (only filling missing values)"
  while IFS='=' read -r key value; do
    [[ -z "$key" || "$key" == \#* ]] && continue
    value="${value%%\#*}"
    value="${value%$'\r'}"
    value="${value%""}"
    value="${value#""}"
    value="${value%'}"
    value="${value#'}"
    value="${value%% }"
    value="${value## }"
    if [[ -z "${!key-}" ]]; then
      export "$key"="$value"
    fi
  done < "$ENV_FILE"
else
  echo "⚠️  No .env.dev file found – relying on existing environment variables"
fi

# Derive configuration using Python helper
python3 "$SCRIPT_DIR/configure.py" dev --check-keys

eval "$(python3 "$SCRIPT_DIR/configure.py" dev --export-network-env)"
echo -e "🌹\n$(python3 "$SCRIPT_DIR/configure.py" dev --export-network-env)"
MCP_NETWORK_URLS_JSON=$(python3 "$SCRIPT_DIR/configure.py" dev --chain-json)
export MCP_NETWORK_URLS_JSON

# Ensure local development services bypass configured proxies (e.g., VPN setups)
if [[ -n "${http_proxy:-}" || -n "${https_proxy:-}" || -n "${HTTP_PROXY:-}" || -n "${HTTPS_PROXY:-}" || -n "${ALL_PROXY:-}" || -n "${all_proxy:-}" ]]; then
  NO_PROXY=$(
    {
      printf '%s\n' localhost 127.0.0.1
      for key in MCP_SERVER_HOST BACKEND_HOST ANVIL_HOST FRONTEND_HOST; do
        value="${!key-}"
        value="${value## }"
        value="${value%% }"
        if [[ -n "${value:-}" ]]; then
          printf '%s\n' "$value"
        fi
      done
      if [[ -n "${NO_PROXY:-}" ]]; then
        IFS=',' read -r -a existing_hosts <<< "$NO_PROXY"
        for host in "${existing_hosts[@]}"; do
          trimmed="${host## }"
          trimmed="${trimmed%% }"
          if [[ -n "$trimmed" ]]; then
            printf '%s\n' "$trimmed"
          fi
        done
      fi
    } | awk '
      $0 != "" && $0 != "0.0.0.0" {
        if (!seen[$0]++) {
          out = out (out ? "," : "") $0
        }
      }
      END { print out }
    '
  )
  export NO_PROXY
  export no_proxy="$NO_PROXY"
fi

# Display summary
echo "🌐 MCP network map: $MCP_NETWORK_URLS_JSON"

echo "🧹 Cleaning previous processes"
"$PROJECT_ROOT/scripts/kill-all.sh" || true
sleep 1

# Ensure Python virtualenv exists for helper tools
if [[ ! -f "$PROJECT_ROOT/.venv/bin/activate" ]]; then
  echo "🐍 Creating Python virtual environment"
  python3 -m venv "$PROJECT_ROOT/.venv"
  "$PROJECT_ROOT/.venv/bin/pip" install -r "$PROJECT_ROOT/requirements.txt"
fi

# Start Anvil unless already running
if ! nc -z "$ANVIL_HOST" "$ANVIL_PORT" 2>/dev/null; then
  if [[ -z "${ETH_RPC_URL:-}" ]]; then
    echo "❌ ETH_RPC_URL is required to launch Anvil"
    exit 1
  fi
  echo "🔧 Starting Anvil at ${ANVIL_HOST}:${ANVIL_PORT}"
  anvil --host "$ANVIL_HOST" --port "$ANVIL_PORT" --fork-url "$ETH_RPC_URL" --no-mining --silent &
  ANVIL_PID=$!
  for _ in {1..20}; do
    if nc -z "$ANVIL_HOST" "$ANVIL_PORT" 2>/dev/null; then
      echo "✅ Anvil ready"
      break
    fi
    sleep 1
  done
else
  echo "✅ Anvil already running"
fi

# Start MCP server
pushd "$PROJECT_ROOT/chatbot" >/dev/null
cargo run -p aomi-mcp -- "$MCP_NETWORK_URLS_JSON" &
MCP_PID=$!
popd >/dev/null

echo "⏳ Waiting for MCP server on ${MCP_SERVER_HOST}:${MCP_SERVER_PORT}"
for _ in {1..30}; do
  if nc -z "$MCP_SERVER_HOST" "$MCP_SERVER_PORT" 2>/dev/null; then
    echo "✅ MCP server ready"
    break
  fi
  sleep 1
done

# Start backend
pushd "$PROJECT_ROOT/chatbot" >/dev/null
cargo build -p backend
for _ in {1..5}; do
  if [[ -n "${NO_PROXY:-}" && -n "${no_proxy:-}" ]]; then
    echo "🔧 Starting backend with NO_PROXY: $NO_PROXY and no_proxy: $no_proxy"
    NO_PROXY="$NO_PROXY" no_proxy="$no_proxy" cargo run -p backend -- --no-docs & BACKEND_PID=$!
  else
    cargo run -p backend -- --no-docs & BACKEND_PID=$!
  fi
  sleep 2
  if nc -z "$BACKEND_HOST" "$BACKEND_PORT" 2>/dev/null; then
    echo "✅ Backend ready"
    break
  fi
  sleep 1
done


popd >/dev/null

echo "⏳ Waiting for backend on ${BACKEND_HOST}:${BACKEND_PORT}"
for _ in {1..40}; do
  if curl -sf "http://${BACKEND_HOST}:${BACKEND_PORT}/health" >/dev/null; then
    echo "✅ Backend healthy"
    break
  fi
  sleep 1
done

# Start frontend with local environment variables
pushd "$PROJECT_ROOT/frontend" >/dev/null
npm install >/dev/null

# Export frontend environment variables to use localhost services
export NEXT_PUBLIC_BACKEND_URL="http://${BACKEND_HOST}:${BACKEND_PORT}"
export NEXT_PUBLIC_ANVIL_URL="http://${ANVIL_HOST}:${ANVIL_PORT}"

npm run dev &
FRONTEND_PID=$!
popd >/dev/null

echo "✅ Frontend running on http://${FRONTEND_HOST}:${FRONTEND_PORT}"
echo "   - Backend URL: http://${BACKEND_HOST}:${BACKEND_PORT}"
echo "   - Anvil URL: http://${ANVIL_HOST}:${ANVIL_PORT}"

echo "🚀 Development environment ready. Press Ctrl+C to stop."
trap 'echo "🛑 Stopping..."; kill $FRONTEND_PID $BACKEND_PID $MCP_PID ${ANVIL_PID:-} 2>/dev/null || true; exit 0' INT TERM
wait
