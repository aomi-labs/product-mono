#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

LOG_DIR="$PROJECT_ROOT/logs"

mkdir -p "$LOG_DIR"
echo "🗂  Logs directory: $LOG_DIR"
echo "📝 MCP service disabled in dev.sh (matching compose-backend-prod)"

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

# Ensure Python virtualenv exists for helper tools
if [[ ! -f "$PROJECT_ROOT/.venv/bin/activate" ]]; then
  echo "🐍 Creating Python virtual environment"
  python3 -m venv "$PROJECT_ROOT/.venv"
  "$PROJECT_ROOT/.venv/bin/pip" install -r "$PROJECT_ROOT/requirements.txt"
fi

# Source the virtualenv if it exists
if [[ -f "$PROJECT_ROOT/.venv/bin/activate" ]]; then
  source "$PROJECT_ROOT/.venv/bin/activate"
fi

# Derive configuration using Python helper
python3 "$SCRIPT_DIR/configure.py" dev --check-keys

eval "$(python3 "$SCRIPT_DIR/configure.py" dev --export-network-env)"
echo -e "🌹\n$(python3 "$SCRIPT_DIR/configure.py" dev --export-network-env)"
CHAIN_NETWORK_URLS_JSON=$(python3 "$SCRIPT_DIR/configure.py" dev --chain-json)
export CHAIN_NETWORK_URLS_JSON

# Default Postgres configuration for local development
POSTGRES_USER="${POSTGRES_USER:-aomi}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-aomi_dev_db_2024}"
POSTGRES_DB="${POSTGRES_DB:-chatbot}"
POSTGRES_HOST="${POSTGRES_HOST:-127.0.0.1}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_HOST_AUTH_METHOD="${POSTGRES_HOST_AUTH_METHOD:-trust}"
export POSTGRES_USER POSTGRES_PASSWORD POSTGRES_DB POSTGRES_HOST POSTGRES_PORT POSTGRES_HOST_AUTH_METHOD
export DATABASE_URL="${DATABASE_URL:-postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}}"

# BAML configuration (defaults allow local CLI usage)
BAML_SERVER_HOST="${BAML_SERVER_HOST:-127.0.0.1}"
BAML_SERVER_PORT="${BAML_SERVER_PORT:-2024}"
BAML_CLI_BIN="${BAML_CLI_BIN:-baml-cli}"
BAML_PASSWORD="${BAML_PASSWORD:-}"
BAML_SRC_DIR="${BAML_SRC_DIR:-$PROJECT_ROOT/aomi/crates/l2beat/baml_src}"

export BAML_SERVER_URL="${BAML_SERVER_URL:-http://${BAML_SERVER_HOST}:${BAML_SERVER_PORT}}"

DOCKER_COMPOSE=()
if command -v docker >/dev/null 2>&1; then
  if docker compose version >/dev/null 2>&1; then
    DOCKER_COMPOSE=(docker compose)
  fi
fi
if [[ ${#DOCKER_COMPOSE[@]} -eq 0 ]] && command -v docker-compose >/dev/null 2>&1; then
  DOCKER_COMPOSE=(docker-compose)
fi
DOCKER_COMPOSE_FILE="$PROJECT_ROOT/docker/docker-compose-backend.yml"
POSTGRES_CONTAINER_STARTED=0
CLEANUP_RAN=0
BAML_PID=""

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
echo "🌐 MCP network map: $CHAIN_NETWORK_URLS_JSON"

echo "🧹 Cleaning previous processes"
"$PROJECT_ROOT/scripts/kill-all.sh" || true
sleep 1

# Prefer local Postgres via psql; fall back to Docker only if unavailable
LOCAL_PSQL="/opt/homebrew/opt/postgresql@17/bin/psql"
if [[ ! -x "$LOCAL_PSQL" ]]; then
  if command -v psql >/dev/null 2>&1; then
    LOCAL_PSQL="$(command -v psql)"
  fi
fi

USE_LOCAL_PG=0
if [[ -x "$LOCAL_PSQL" ]]; then
  if "$LOCAL_PSQL" -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres -c '\q' >/dev/null 2>&1; then
    # Ensure database exists
    if ! "$LOCAL_PSQL" -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname = '$POSTGRES_DB'" | grep -q 1; then
      echo "📦 Creating database '$POSTGRES_DB' (local Postgres detected)"
      "$LOCAL_PSQL" -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres -c "CREATE DATABASE $POSTGRES_DB" >/dev/null 2>&1 || true
    fi

    # Check if schema exists (optional check for informational purposes)
    if "$LOCAL_PSQL" -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tc "SELECT 1 FROM information_schema.tables WHERE table_name='contracts'" | grep -q 1; then
      echo "✅ Local Postgres available with existing schema (contracts table present)"
    else
      echo "✅ Local Postgres available (empty database - schema will be created via sqlx migrations)"
    fi
    USE_LOCAL_PG=1
  fi
fi

if [[ $USE_LOCAL_PG -ne 1 ]]; then
  echo "❌ Local Postgres is not available on ${POSTGRES_HOST}:${POSTGRES_PORT} as user ${POSTGRES_USER}"
  echo "➡️  Please start your local Postgres. Database schema will be created automatically when backend starts."
  exit 1
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

echo "⚙️  Skipping MCP server startup for local dev (see compose-backend-prod.sh)"

# Start BAML server if not already running
if ! nc -z "$BAML_SERVER_HOST" "$BAML_SERVER_PORT" 2>/dev/null; then
  if ! command -v "$BAML_CLI_BIN" >/dev/null 2>&1; then
    echo "❌ Could not find '$BAML_CLI_BIN' in PATH."
    echo "➡️  Install the BAML CLI with: npm install -g @boundaryml/baml"
    exit 1
  fi

  if [[ ! -d "$BAML_SRC_DIR" ]]; then
    echo "❌ Expected BAML source directory at $BAML_SRC_DIR"
    echo "➡️  Ensure your repository has baml_src/ generated before starting dev.sh"
    exit 1
  fi

  echo "🧱 Starting BAML server via ${BAML_CLI_BIN} on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}"

  if [[ -n "$BAML_PASSWORD" ]]; then
    echo "   Using BAML_PASSWORD for authenticated access"
    BAML_PASSWORD="$BAML_PASSWORD" "$BAML_CLI_BIN" serve --from "$BAML_SRC_DIR" --port "$BAML_SERVER_PORT" &
  else
    "$BAML_CLI_BIN" serve --from "$BAML_SRC_DIR" --port "$BAML_SERVER_PORT" &
  fi
  BAML_PID=$!

  BAML_READY=0
  for _ in {1..30}; do
    if nc -z "$BAML_SERVER_HOST" "$BAML_SERVER_PORT" 2>/dev/null; then
      echo "✅ BAML server ready"
      BAML_READY=1
      break
    fi
    sleep 1
  done
  if [[ $BAML_READY -ne 1 ]]; then
    echo "❌ BAML server did not become ready on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}"
    exit 1
  fi
else
  echo "✅ BAML server already running on ${BAML_SERVER_HOST}:${BAML_SERVER_PORT}"
fi

# Start backend
pushd "$PROJECT_ROOT/aomi" >/dev/null
cargo build -p backend
echo "🐛 Starting backend with DEBUG logging enabled (RUST_LOG=debug)"
for _ in {1..5}; do
  if [[ -n "${NO_PROXY:-}" && -n "${no_proxy:-}" ]]; then
    echo "🔧 Starting backend with NO_PROXY: $NO_PROXY and no_proxy: $no_proxy"
    RUST_LOG=debug NO_PROXY="$NO_PROXY" no_proxy="$no_proxy" cargo run -p backend -- --no-docs --skip-mcp & BACKEND_PID=$!
  else
    RUST_LOG=debug cargo run -p backend -- --no-docs --skip-mcp & BACKEND_PID=$!
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
cleanup() {
  if [[ ${CLEANUP_RAN:-0} -eq 1 ]]; then
    return
  fi
  CLEANUP_RAN=1
  echo "🛑 Stopping..."
  local pids=()
  [[ -n "${FRONTEND_PID:-}" ]] && pids+=("$FRONTEND_PID")
  [[ -n "${BACKEND_PID:-}" ]] && pids+=("$BACKEND_PID")
  [[ -n "${MCP_PID:-}" ]] && pids+=("$MCP_PID")
  [[ -n "${ANVIL_PID:-}" ]] && pids+=("$ANVIL_PID")
  [[ -n "${BAML_PID:-}" ]] && pids+=("$BAML_PID")
  if [[ ${#pids[@]} -gt 0 ]]; then
    kill "${pids[@]}" 2>/dev/null || true
  fi
  if [[ ${POSTGRES_CONTAINER_STARTED:-0} -eq 1 && ${#DOCKER_COMPOSE[@]} -gt 0 ]]; then
    echo "🛑 Stopping Postgres container"
    "${DOCKER_COMPOSE[@]}" -f "$DOCKER_COMPOSE_FILE" stop postgres >/dev/null 2>&1 || true
    "${DOCKER_COMPOSE[@]}" -f "$DOCKER_COMPOSE_FILE" rm -f postgres >/dev/null 2>&1 || true
  fi
}
trap 'cleanup; exit 0' INT TERM
wait
cleanup
