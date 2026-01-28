#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "🤖 Aomi Telegram Bot"
echo "===================="

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
    # Mask the value for display
    echo "✅ $key"
  fi
done

# Check optional but recommended keys
OPTIONAL_KEYS=("BRAVE_SEARCH_API_KEY" "ETHERSCAN_API_KEY")
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
RUN_FLAGS=""

# Parse arguments
for arg in "$@"; do
  case "$arg" in
    --release)
      BUILD_FLAGS="--release"
      ;;
    --no-docs)
      RUN_FLAGS="$RUN_FLAGS --no-docs"
      ;;
    --skip-mcp)
      RUN_FLAGS="$RUN_FLAGS --skip-mcp"
      ;;
    --help|-h)
      echo ""
      echo "Usage: $0 [options]"
      echo ""
      echo "Options:"
      echo "  --release    Build in release mode"
      echo "  --no-docs    Skip loading Uniswap documentation"
      echo "  --skip-mcp   Skip MCP server connection"
      echo ""
      echo "Environment variables:"
      echo "  TELEGRAM_BOT_TOKEN     (required) Bot token from @BotFather"
      echo "  ANTHROPIC_API_KEY      (required) Claude API key"
      echo "  BRAVE_SEARCH_API_KEY   (optional) For web search"
      echo "  DATABASE_URL           (optional) Postgres connection string"
      echo "  TELEGRAM_DM_POLICY     (optional) open|allowlist|disabled (default: open)"
      echo "  TELEGRAM_GROUP_POLICY  (optional) mention|always|disabled (default: mention)"
      echo "  TELEGRAM_ALLOW_FROM    (optional) Comma-separated user IDs for allowlist"
      exit 0
      ;;
    *)
      echo "❌ Unknown argument: $arg (use --help for usage)"
      exit 1
      ;;
  esac
done

# Build
echo ""
echo "🔨 Building telegram bot..."
cd "$PROJECT_ROOT/aomi"
cargo build -p aomi-telegram $BUILD_FLAGS

# Run
echo ""
echo "🚀 Starting Telegram bot..."
echo "   Press Ctrl+C to stop"
echo ""

RUST_LOG="${RUST_LOG:-info}" cargo run -p aomi-telegram $BUILD_FLAGS -- $RUN_FLAGS
