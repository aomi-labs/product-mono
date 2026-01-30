#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
COMPOSE_FILE="$PROJECT_ROOT/docker/docker-compose-telegram.yml"
COMPOSE_DIR="$(dirname "$COMPOSE_FILE")"
DEFAULT_PROJECT_NAME="aomi-telegram"
export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-$DEFAULT_PROJECT_NAME}"

print_usage() {
  echo "Usage: $0 <IMAGE_TAG> [options]"
  echo ""
  echo "Arguments:"
  echo "  IMAGE_TAG              Docker image tag (e.g., latest, v1.0.0)"
  echo ""
  echo "Options:"
  echo "  --mini-app-url URL     Public HTTPS URL for Mini App (required for wallet connect)"
  echo "  --help                 Show this help message"
  echo ""
  echo "Examples:"
  echo "  $0 latest --mini-app-url https://connect.aomi.ai"
  echo "  $0 v1.0.0"
  echo ""
  echo "Required environment variables (in .env.prod):"
  echo "  TELEGRAM_BOT_TOKEN     Bot token from @BotFather"
  echo "  ANTHROPIC_API_KEY      Claude API key"
  echo ""
  echo "Optional environment variables:"
  echo "  REOWN_PROJECT_ID       WalletConnect/Reown project ID"
  echo "  TELEGRAM_DM_POLICY     DM policy: open|allowlist|disabled (default: open)"
  echo "  TELEGRAM_GROUP_POLICY  Group policy: mention|always|disabled (default: mention)"
  echo "  TELEGRAM_ALLOW_FROM    Comma-separated list of allowed user IDs"
  echo "  POSTGRES_PASSWORD      Database password (default: aomi_prod_db_2024)"
}

if [[ $# -lt 1 ]] || [[ "$1" == "--help" ]]; then
  print_usage
  exit 0
fi

IMAGE_TAG="$1"
shift
export IMAGE_TAG

# Parse optional arguments
MINI_APP_URL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mini-app-url)
      MINI_APP_URL="$2"
      shift 2
      ;;
    --help)
      print_usage
      exit 0
      ;;
    *)
      echo "❌ Unknown option: $1"
      print_usage
      exit 1
      ;;
  esac
done

echo "🤖 Aomi Telegram Bot Deployment"
echo "================================"
echo "🏷️  Image tag: $IMAGE_TAG"

# Load environment file
ENV_FILE="$PROJECT_ROOT/.env.prod"
if [[ -f "$ENV_FILE" ]]; then
  echo "🔑 Loading environment from $ENV_FILE"
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
else
  echo "❌ No .env.prod file found at $ENV_FILE"
  echo "Please create .env.prod with your API keys:"
  echo ""
  echo "  TELEGRAM_BOT_TOKEN=your_bot_token"
  echo "  ANTHROPIC_API_KEY=your_api_key"
  echo "  REOWN_PROJECT_ID=your_walletconnect_project_id"
  exit 1
fi

# Validate required environment variables
REQUIRED_VARS=("TELEGRAM_BOT_TOKEN" "ANTHROPIC_API_KEY")
MISSING_VARS=()

for var in "${REQUIRED_VARS[@]}"; do
  if [[ -z "${!var:-}" ]]; then
    MISSING_VARS+=("$var")
  fi
done

if [[ ${#MISSING_VARS[@]} -gt 0 ]]; then
  echo "❌ Missing required environment variables:"
  for var in "${MISSING_VARS[@]}"; do
    echo "   - $var"
  done
  exit 1
fi

# Set Mini App URL
if [[ -n "$MINI_APP_URL" ]]; then
  export MINI_APP_URL
  echo "🔗 Mini App URL: $MINI_APP_URL"
elif [[ -n "${MINI_APP_URL:-}" ]]; then
  echo "🔗 Mini App URL: $MINI_APP_URL (from env)"
else
  echo "⚠️  No MINI_APP_URL set - wallet connect will be disabled"
  echo "   Use --mini-app-url to enable wallet connection"
fi

# Configuration summary
echo ""
echo "📋 Configuration:"
echo "   Telegram DM Policy: ${TELEGRAM_DM_POLICY:-open}"
echo "   Telegram Group Policy: ${TELEGRAM_GROUP_POLICY:-mention}"
if [[ -n "${TELEGRAM_ALLOW_FROM:-}" ]]; then
  echo "   Allowed Users: ${TELEGRAM_ALLOW_FROM}"
fi
echo "   WalletConnect: ${REOWN_PROJECT_ID:+configured}${REOWN_PROJECT_ID:-not configured}"
echo ""

# Stop existing containers
echo "🛑 Stopping existing containers..."
docker compose -f "$COMPOSE_FILE" down 2>/dev/null || true

# Pull images
echo "📥 Pulling images..."
docker pull ghcr.io/aomi-labs/product-mono/telegram:$IMAGE_TAG || {
  echo "⚠️  Could not pull telegram:$IMAGE_TAG - will try to build locally"
}
docker pull ghcr.io/aomi-labs/product-mono/miniapp:$IMAGE_TAG || {
  echo "⚠️  Could not pull miniapp:$IMAGE_TAG - will try to build locally"
}

# Clean up
echo "🧹 Cleaning up old containers..."
docker system prune -f || true

# Start services
echo "🚀 Starting Telegram bot stack..."
docker compose -f "$COMPOSE_FILE" up --force-recreate -d

# Wait for services
echo "⏳ Waiting for services to initialize..."
sleep 10

# Check health
echo "🔍 Checking service status..."
docker compose -f "$COMPOSE_FILE" ps

# Check if telegram bot is running
if docker compose -f "$COMPOSE_FILE" ps telegram | grep -q "Up"; then
  echo "✅ Telegram bot is running"
else
  echo "⚠️  Telegram bot may not be running properly"
  echo "   Check logs: docker compose -f \"$COMPOSE_FILE\" logs telegram"
fi

# Check if miniapp is running
if docker compose -f "$COMPOSE_FILE" ps miniapp | grep -q "Up"; then
  echo "✅ Mini App is running on port ${MINIAPP_PORT:-3001}"
else
  echo "⚠️  Mini App may not be running properly"
  echo "   Check logs: docker compose -f \"$COMPOSE_FILE\" logs miniapp"
fi

echo ""
echo "🎉 Deployment complete!"
echo ""
echo "📋 Service endpoints:"
echo "   🤖 Telegram Bot: Running (check @YourBot in Telegram)"
if [[ -n "${MINI_APP_URL:-}" ]]; then
  echo "   🔗 Mini App: $MINI_APP_URL"
else
  echo "   🔗 Mini App: http://localhost:${MINIAPP_PORT:-3001} (local only)"
fi
echo "   🗄️  PostgreSQL: localhost:${POSTGRES_PORT:-5432}"
echo ""
echo "🏷️  Deployed version: $IMAGE_TAG"
echo ""
echo "📋 Useful commands:"
echo "   View logs:     docker compose -f \"$COMPOSE_FILE\" logs -f"
echo "   Bot logs:      docker compose -f \"$COMPOSE_FILE\" logs -f telegram"
echo "   Mini App logs: docker compose -f \"$COMPOSE_FILE\" logs -f miniapp"
echo "   Stop services: docker compose -f \"$COMPOSE_FILE\" down"
echo "   Restart:       docker compose -f \"$COMPOSE_FILE\" restart"
