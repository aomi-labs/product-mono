#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LOCK_DIR="${TMPDIR:-/tmp}/aomi-telegram-bot.lock"

# Ensure only one launcher instance is active, otherwise Telegram polling conflicts.
acquire_lock() {
  if mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "$$" > "$LOCK_DIR/pid"
    return 0
  fi

  local existing_pid=""
  if [[ -f "$LOCK_DIR/pid" ]]; then
    existing_pid="$(cat "$LOCK_DIR/pid" 2>/dev/null || true)"
  fi

  # Recover from stale lock (e.g., crashed process).
  if [[ -n "$existing_pid" ]] && ! kill -0 "$existing_pid" 2>/dev/null; then
    rm -rf "$LOCK_DIR"
    if mkdir "$LOCK_DIR" 2>/dev/null; then
      echo "$$" > "$LOCK_DIR/pid"
      return 0
    fi
  fi

  echo "❌ Telegram launcher already running${existing_pid:+ (PID: $existing_pid)}"
  echo "   Stop the other instance first to avoid Api(TerminatedByOtherGetUpdates)."
  exit 1
}

echo "🤖 Aomi Telegram Bot + Mini App"
echo "================================"

# Load environment variables
ENV_FILE="${ENV_FILE:-$PROJECT_ROOT/.env}"
if [[ -f "$ENV_FILE" ]]; then
  echo "🔑 Loading environment from $ENV_FILE"
  set -a
  source "$ENV_FILE"
  set +a
elif [[ -f "$PROJECT_ROOT/.env.prod" ]]; then
  echo "🔑 Loading environment from .env.prod"
  set -a
  source "$PROJECT_ROOT/.env.prod"
  set +a
else
  echo "⚠️  No .env file found – relying on existing environment variables"
fi

# Check required environment variables
echo ""
echo "🔍 Checking environment variables..."

REQUIRED_KEYS=("TELEGRAM_BOT_TOKEN" "ANTHROPIC_API_KEY")
MISSING_KEYS=()

for key in "${REQUIRED_KEYS[@]}"; do
  if [[ -z "${!key:-}" ]]; then
    echo "❌ $key (required)"
    MISSING_KEYS+=("$key")
  else
    echo "✅ $key"
  fi
done

# Check optional but recommended keys
OPTIONAL_KEYS=("BRAVE_SEARCH_API_KEY" "ETHERSCAN_API_KEY" "REOWN_PROJECT_ID")
for key in "${OPTIONAL_KEYS[@]}"; do
  if [[ -z "${!key:-}" ]]; then
    echo "⚠️  $key (optional - some features may not work)"
  else
    echo "✅ $key"
  fi
done

if [[ ${#MISSING_KEYS[@]} -gt 0 ]]; then
  echo ""
  echo "❌ Missing required environment variables: ${MISSING_KEYS[*]}"
  echo "   Add them to $ENV_FILE or export them before running."
  exit 1
fi

# Database URL (with sensible default)
export DATABASE_URL="${DATABASE_URL:-postgresql://aomi@localhost:5432/chatbot}"
echo "✅ DATABASE_URL"

# Telegram config (show current settings)
echo ""
echo "📱 Telegram Configuration:"
echo "   DM Policy: ${TELEGRAM_DM_POLICY:-open}"
echo "   Group Policy: ${TELEGRAM_GROUP_POLICY:-mention}"
if [[ -n "${TELEGRAM_ALLOW_FROM:-}" ]]; then
  echo "   Allowlist: ${TELEGRAM_ALLOW_FROM}"
fi

# Build flags
BUILD_FLAGS=""
RUN_FLAGS="--no-docs --skip-mcp"
SKIP_MINI_APP=false
USE_NGROK=true
MINI_APP_PORT=3001

# Parse arguments
for arg in "$@"; do
  case "$arg" in
    --release)
      BUILD_FLAGS="--release"
      ;;
    --no-mini-app)
      SKIP_MINI_APP=true
      ;;
    --no-ngrok)
      USE_NGROK=false
      ;;
    --help|-h)
      echo ""
      echo "Usage: $0 [options]"
      echo ""
      echo "Options:"
      echo "  --release      Build in release mode"
      echo "  --no-mini-app  Don't start the Mini App server"
      echo "  --no-ngrok     Don't start ngrok tunnel (use MINI_APP_URL from env)"
      echo ""
      echo "Environment variables:"
      echo "  TELEGRAM_BOT_TOKEN     (required) Bot token from @BotFather"
      echo "  ANTHROPIC_API_KEY      (required) Claude API key"
      echo "  REOWN_PROJECT_ID       (optional) WalletConnect/Reown project ID"
      echo "  MINI_APP_URL           (optional) HTTPS URL for mini-app (skips ngrok)"
      echo "  DATABASE_URL           (optional) Postgres connection string"
      exit 0
      ;;
    *)
      echo "❌ Unknown argument: $arg (use --help for usage)"
      exit 1
      ;;
  esac
done

# PIDs for cleanup
MINI_APP_PID=""
NGROK_PID=""
BOT_PID=""

cleanup() {
  echo ""
  echo "🛑 Shutting down..."
  [[ -n "$BOT_PID" ]] && kill "$BOT_PID" 2>/dev/null || true
  [[ -n "$NGROK_PID" ]] && kill "$NGROK_PID" 2>/dev/null || true
  [[ -n "$MINI_APP_PID" ]] && kill "$MINI_APP_PID" 2>/dev/null || true
  # Kill any remaining ngrok processes
  pkill -f "ngrok http $MINI_APP_PORT" 2>/dev/null || true
  rm -rf "$LOCK_DIR"
  exit 0
}

trap cleanup SIGINT SIGTERM EXIT

acquire_lock

# Check if MINI_APP_URL is already set to HTTPS (skip ngrok)
if [[ "${MINI_APP_URL:-}" == https://* ]]; then
  echo ""
  echo "✅ MINI_APP_URL already set to HTTPS: $MINI_APP_URL"
  USE_NGROK=false
fi

# Start Mini App (if not skipped)
if [[ "$SKIP_MINI_APP" == "false" ]]; then
  MINI_APP_DIR="$PROJECT_ROOT/mini-app"
  
  if [[ -d "$MINI_APP_DIR" ]]; then
    echo ""
    echo "🌐 Starting Mini App..."
    
    # Check if node_modules exists
    if [[ ! -d "$MINI_APP_DIR/node_modules" ]]; then
      echo "📦 Installing Mini App dependencies..."
      (cd "$MINI_APP_DIR" && npm install)
    fi
    
    # Create .env.local for mini-app
    cat > "$MINI_APP_DIR/.env.local" << EOF
NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID=${REOWN_PROJECT_ID:-}
TELEGRAM_BOT_TOKEN=${TELEGRAM_BOT_TOKEN:-}
BACKEND_URL=${BACKEND_URL:-http://localhost:8080}
DATABASE_URL=${DATABASE_URL:-postgresql://aomi@localhost:5432/chatbot}
EOF
    
    # Start mini-app in background
    (cd "$MINI_APP_DIR" && npm run dev 2>&1 | sed 's/^/[mini-app] /') &
    MINI_APP_PID=$!
    echo "   Mini App PID: $MINI_APP_PID"
    echo "   Local URL: http://localhost:$MINI_APP_PORT"
    
    # Give it a moment to start
    sleep 3
  else
    echo "⚠️  Mini App directory not found at $MINI_APP_DIR"
    SKIP_MINI_APP=true
  fi
fi

# Start ngrok tunnel (if needed)
if [[ "$SKIP_MINI_APP" == "false" && "$USE_NGROK" == "true" ]]; then
  echo ""
  echo "🔗 Starting ngrok tunnel..."
  
  # Check if ngrok is installed
  if ! command -v ngrok &> /dev/null; then
    echo "❌ ngrok not found. Install it with: npm install -g ngrok"
    echo "   Or set MINI_APP_URL to an HTTPS URL in your .env"
    exit 1
  fi
  
  # Kill any existing ngrok on this port
  pkill -f "ngrok http $MINI_APP_PORT" 2>/dev/null || true
  sleep 1
  
  # Start ngrok in background
  ngrok http $MINI_APP_PORT --log=stdout > /tmp/ngrok.log 2>&1 &
  NGROK_PID=$!
  echo "   ngrok PID: $NGROK_PID"
  
  # Wait for ngrok to start and get URL
  echo "   Waiting for ngrok tunnel..."
  sleep 3
  
  # Get the public URL from ngrok API
  for i in {1..10}; do
    NGROK_URL=$(curl -s http://localhost:4040/api/tunnels 2>/dev/null | grep -o '"public_url":"https://[^"]*' | head -1 | cut -d'"' -f4 || true)
    if [[ -n "$NGROK_URL" ]]; then
      break
    fi
    sleep 1
  done
  
  if [[ -z "$NGROK_URL" ]]; then
    echo "❌ Failed to get ngrok URL. Check /tmp/ngrok.log"
    echo "   You may need to authenticate ngrok: ngrok config add-authtoken <token>"
    cat /tmp/ngrok.log | tail -20
    exit 1
  fi
  
  export MINI_APP_URL="$NGROK_URL"
  echo "   ✅ ngrok URL: $MINI_APP_URL"
fi

# Show final Mini App URL
if [[ -n "${MINI_APP_URL:-}" ]]; then
  echo ""
  echo "🔗 Mini App URL: $MINI_APP_URL"
else
  echo ""
  echo "⚠️  No MINI_APP_URL set - wallet connect button will be disabled"
fi

# Build telegram bot
echo ""
echo "🔨 Building telegram bot..."
cd "$PROJECT_ROOT/aomi"
cargo build -p aomi-telegram $BUILD_FLAGS

# Run telegram bot
echo ""
echo "🚀 Starting Telegram bot..."
echo "   Press Ctrl+C to stop all services"
echo ""

RUST_LOG="${RUST_LOG:-info}" cargo run -p aomi-telegram $BUILD_FLAGS -- $RUN_FLAGS &
BOT_PID=$!

# Wait for bot to exit
wait $BOT_PID
