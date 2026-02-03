#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

LOG_DIR="$PROJECT_ROOT/logs"

USE_LANDING=0
USE_LOCAL_FE=0
USE_MOCK_ETHEREUM=0
for arg in "$@"; do
  case "$arg" in
    --landing)
      USE_LANDING=1
      ;;
    --local-fe)
      USE_LOCAL_FE=1
      ;;
    --mock-ethereum)
      USE_MOCK_ETHEREUM=1
      ;;
    *)
      echo "❌ Unknown argument: $arg"
      echo "Usage: $0 [--landing] [--local-fe] [--mock-ethereum]"
      echo "  --landing        Use aomi-widget landing page instead of monorepo frontend"
      echo "  --local-fe       Resolve @aomi-labs packages from local aomi-widget repo, AOMI_WIDGET_ROOT=/path/to/aomi-widget"
      echo "  --mock-ethereum  Start with mock Ethereum (anvil on 8545) for wallet testing"
      exit 1
      ;;
  esac
done

mkdir -p "$LOG_DIR"
echo "🗂  Logs directory: $LOG_DIR"

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

# Set default network configuration (from config.yaml defaults)
export BACKEND_HOST="${BACKEND_HOST:-127.0.0.1}"
export BACKEND_PORT="${BACKEND_PORT:-8080}"
export FRONTEND_HOST="${FRONTEND_HOST:-127.0.0.1}"
export FRONTEND_PORT="${FRONTEND_PORT:-3000}"

# Check required API keys
echo "🔍 Checking environment variables"
REQUIRED_KEYS=("ANTHROPIC_API_KEY" "BRAVE_SEARCH_API_KEY" "ETHERSCAN_API_KEY")
MISSING_KEYS=()
for key in "${REQUIRED_KEYS[@]}"; do
  if [[ -z "${!key:-}" ]]; then
    echo "❌ $key (required)"
    MISSING_KEYS+=("$key")
  else
    echo "✅ $key (required)"
  fi
done

OPTIONAL_KEYS=("ZEROX_API_KEY")
for key in "${OPTIONAL_KEYS[@]}"; do
  if [[ -z "${!key:-}" ]]; then
    echo "⚠️  $key (optional)"
  else
    echo "✅ $key (optional)"
  fi
done

if [[ ${#MISSING_KEYS[@]} -gt 0 ]]; then
  echo "❌ Missing required environment variables: ${MISSING_KEYS[*]}"
  exit 1
fi

echo "🔧 Configured services:"
echo "   BACKEND_HOST=${BACKEND_HOST}"
echo "   BACKEND_PORT=${BACKEND_PORT}"
echo "   FRONTEND_HOST=${FRONTEND_HOST}"
echo "   FRONTEND_PORT=${FRONTEND_PORT}"

# Default Postgres configuration for local development
POSTGRES_USER="${POSTGRES_USER:-aomi}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-aomi_dev_db_2024}"
POSTGRES_DB="${POSTGRES_DB:-chatbot}"
POSTGRES_HOST="${POSTGRES_HOST:-127.0.0.1}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_HOST_AUTH_METHOD="${POSTGRES_HOST_AUTH_METHOD:-trust}"
export POSTGRES_USER POSTGRES_PASSWORD POSTGRES_DB POSTGRES_HOST POSTGRES_PORT POSTGRES_HOST_AUTH_METHOD
export DATABASE_URL="${DATABASE_URL:-postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}}"

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

# Ensure local development services bypass configured proxies (e.g., VPN setups)
if [[ -n "${http_proxy:-}" || -n "${https_proxy:-}" || -n "${HTTP_PROXY:-}" || -n "${HTTPS_PROXY:-}" || -n "${ALL_PROXY:-}" || -n "${all_proxy:-}" ]]; then
  NO_PROXY=$(
    {
      printf '%s\n' localhost 127.0.0.1
      for key in BACKEND_HOST FRONTEND_HOST; do
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
echo "🧹 Cleaning previous processes"
# Only pass --landing to kill-all.sh (it doesn't know about --local-fe)
if [[ $USE_LANDING -eq 1 ]]; then
  "$PROJECT_ROOT/scripts/kill-all.sh" --landing || true
else
  "$PROJECT_ROOT/scripts/kill-all.sh" || true
fi
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

# Start mock Ethereum (anvil) if requested
ANVIL_PID=""
if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
  echo "🔨 Starting mock Ethereum (anvil) on port 8545..."
  if ! command -v anvil >/dev/null 2>&1; then
    echo "❌ anvil not found. Install foundry: curl -L https://foundry.paradigm.xyz | bash && foundryup"
    exit 1
  fi
  anvil --port 8545 > "$LOG_DIR/anvil.log" 2>&1 & ANVIL_PID=$!
  sleep 2
  if ! kill -0 "$ANVIL_PID" 2>/dev/null; then
    echo "❌ Failed to start anvil. Check $LOG_DIR/anvil.log"
    exit 1
  fi
  echo "✅ Mock Ethereum running on http://127.0.0.1:8545 (chain ID: 31337)"
  echo "   Configure wallet (MetaMask) to use this RPC URL for testing"
fi

# Start backend
pushd "$PROJECT_ROOT/aomi" >/dev/null
cargo build -p backend
echo "ℹ️  Starting backend with INFO logging enabled (RUST_LOG=info)"

# Build backend args
BACKEND_ARGS="--no-docs --skip-mcp"
if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
  BACKEND_ARGS="$BACKEND_ARGS --providers crates/anvil/providers.test.toml"
  echo "🔧 Using mock Ethereum providers config"
fi

if [[ -n "${NO_PROXY:-}" && -n "${no_proxy:-}" ]]; then
  echo "🔧 Starting backend with NO_PROXY: $NO_PROXY and no_proxy: $no_proxy"
  RUST_LOG=info NO_PROXY="$NO_PROXY" no_proxy="$no_proxy" cargo run -p backend -- $BACKEND_ARGS & BACKEND_PID=$!
else
  RUST_LOG=info cargo run -p backend -- $BACKEND_ARGS & BACKEND_PID=$!
fi


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
if [[ $USE_LANDING -eq 1 ]]; then
  LANDING_ROOT="$PROJECT_ROOT/../aomi-widget"
  if [[ ! -d "$LANDING_ROOT" ]]; then
    echo "❌ aomi-widget not found at $LANDING_ROOT"
    exit 1
  fi
  pushd "$LANDING_ROOT" >/dev/null

  # Export frontend environment variables to use localhost services
  export NEXT_PUBLIC_BACKEND_URL="http://${BACKEND_HOST}:${BACKEND_PORT}"

  # Enable localhost network for mock Ethereum testing
  if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
    export NEXT_PUBLIC_USE_LOCALHOST=true
    echo "🔧 Landing configured for localhost/anvil network"
  fi

  pnpm run dev:landing:live &
  FRONTEND_PID=$!
  popd >/dev/null

  echo "✅ Landing frontend running on http://${FRONTEND_HOST}:${FRONTEND_PORT}"
  echo "   - Backend URL: http://${BACKEND_HOST}:${BACKEND_PORT}"
  if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
    echo "   - Mock Ethereum: http://127.0.0.1:8545 (Localhost network enabled)"
  fi
else
  pushd "$PROJECT_ROOT/frontend" >/dev/null
  npm install >/dev/null

  # Export frontend environment variables to use localhost services
  export NEXT_PUBLIC_BACKEND_URL="http://${BACKEND_HOST}:${BACKEND_PORT}"

  # Enable localhost network for mock Ethereum testing
  if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
    export NEXT_PUBLIC_USE_LOCALHOST=true
    echo "🔧 Frontend configured for localhost/anvil network"
  fi

  # Set up local widget resolution if requested
  if [[ $USE_LOCAL_FE -eq 1 ]]; then
    CODE_DIR="$(dirname "$(dirname "$PROJECT_ROOT")")"
    if [[ -n "${AOMI_WIDGET_ROOT:-}" && -d "$AOMI_WIDGET_ROOT" ]]; then
      WIDGET_ROOT="$AOMI_WIDGET_ROOT"
    elif [[ -d "$CODE_DIR/aomi-widget" ]]; then
      WIDGET_ROOT="$CODE_DIR/aomi-widget"
    elif [[ -d "$CODE_DIR/aomi-widget.worktrees/event-buff-redo" ]]; then
      WIDGET_ROOT="$CODE_DIR/aomi-widget.worktrees/event-buff-redo"
    else
      echo "❌ aomi-widget not found. Tried:"
      echo "   - \$AOMI_WIDGET_ROOT (not set or doesn't exist)"
      echo "   - $CODE_DIR/aomi-widget"
      echo "   - $CODE_DIR/aomi-widget.worktrees/event-buff-redo"
      echo "   Set AOMI_WIDGET_ROOT env var to the correct path"
      exit 1
    fi
    export AOMI_WIDGET_ROOT="$WIDGET_ROOT"
    echo "🔗 Using local widget from: $AOMI_WIDGET_ROOT"
    echo "🧹 Cleaning Next.js cache for local widget..."
    npm run clean:cache
  fi

  npm run dev &
  FRONTEND_PID=$!
  popd >/dev/null

  echo "✅ Frontend running on http://${FRONTEND_HOST}:${FRONTEND_PORT}"
  echo "   - Backend URL: http://${BACKEND_HOST}:${BACKEND_PORT}"
  if [[ $USE_LOCAL_FE -eq 1 ]]; then
    echo "   - Local widget: $AOMI_WIDGET_ROOT"
  fi
  if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
    echo "   - Mock Ethereum: http://127.0.0.1:8545 (Localhost network enabled)"
  fi
fi

echo "🚀 Development environment ready. Press Ctrl+C to stop."
if [[ $USE_MOCK_ETHEREUM -eq 1 ]]; then
  echo ""
  echo "📝 Mock Ethereum Testing:"
  echo "   1. Open wallet (MetaMask) and add custom network:"
  echo "      - RPC URL: http://127.0.0.1:8545"
  echo "      - Chain ID: 31337"
  echo "      - Symbol: ETH"
  echo "   2. Import test account (anvil default):"
  echo "      - Private key: 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
  echo "   3. Select 'Localhost' network in the app"
fi
cleanup() {
  if [[ ${CLEANUP_RAN:-0} -eq 1 ]]; then
    return
  fi
  CLEANUP_RAN=1
  echo "🛑 Stopping..."
  local pids=()
  [[ -n "${FRONTEND_PID:-}" ]] && pids+=("$FRONTEND_PID")
  [[ -n "${BACKEND_PID:-}" ]] && pids+=("$BACKEND_PID")
  [[ -n "${ANVIL_PID:-}" ]] && pids+=("$ANVIL_PID")
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
