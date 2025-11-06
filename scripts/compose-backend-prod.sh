#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ $# -lt 1 ]]; then
  echo "❌ Error: IMAGE_TAG is required"
  echo "Usage: $0 <IMAGE_TAG>"
  echo "Example: $0 deployment-ver3"
  echo "Example: $0 latest"
  exit 1
fi

IMAGE_TAG="$1"
export IMAGE_TAG

echo "🚀 Starting backend services deployment..."
echo "🏷️  Using IMAGE_TAG: $IMAGE_TAG"

ENV_FILE="$PROJECT_ROOT/.env.prod"
if [[ -f "$ENV_FILE" ]]; then
  echo "🔑 Loading API keys from $ENV_FILE"
  # shellcheck disable=SC2046
  export $(grep -v '^#' "$ENV_FILE" | xargs)
else
  echo "❌ No .env.prod file found at $ENV_FILE"
  echo "Please create .env.prod with your API keys before running production deployment"
  exit 1
fi

BACKEND_PORT="${BACKEND_PORT:-8081}"
# BAML-over-HTTP default port
BAML_PORT="${BAML_SERVER_PORT:-2024}"
# MCP_PORT="${MCP_SERVER_PORT:-5001}"  # MCP disabled for emergency deployment
ANVIL_PORT="${ANVIL_PORT:-8545}"

echo "📡 Port configuration:"
echo "   Backend: $BACKEND_PORT"
echo "   BAML:    $BAML_PORT"
echo "   Anvil: $ANVIL_PORT"
echo "   (MCP service disabled for simplified deployment)"

echo "🗄️  Database setup will be handled by Docker containers..."
echo "   - PostgreSQL will auto-initialize with required tables"
echo "   - Contract fetching will run after database is ready"

echo "🛑 Stopping existing containers..."
docker compose -f "$PROJECT_ROOT/docker/docker-compose-backend.yml" down || true

echo "📥 Pulling images with tag: $IMAGE_TAG..."
docker pull ghcr.io/aomi-labs/product-mono/backend:$IMAGE_TAG || { echo "❌ Failed to pull backend:$IMAGE_TAG"; exit 1; }
# docker pull ghcr.io/aomi-labs/product-mono/mcp:$IMAGE_TAG || { echo "❌ Failed to pull mcp:$IMAGE_TAG"; exit 1; }  # MCP disabled
docker pull ghcr.io/foundry-rs/foundry:latest || true

cd "$PROJECT_ROOT"

echo "🧹 Cleaning up old containers..."
docker system prune -f || true

echo "🚀 Starting backend services stack (including BAML over HTTP)..."
docker compose -f docker/docker-compose-backend.yml up --build --force-recreate -d

echo "⏳ Waiting for services to start..."
sleep 15

echo "🔍 Checking service health..."
docker compose -f docker/docker-compose-backend.yml ps

check_curl() {
  local url="$1"
  if curl -f -s "$url" > /dev/null 2>&1; then
    echo "✅ Responding: $url"
  else
    echo "⚠️  No response: $url"
  fi
}

check_tcp() {
  local host="$1"
  local port="$2"
  if command -v nc >/dev/null 2>&1; then
    if nc -z "$host" "$port" 2>/dev/null; then
      echo "✅ Port open: $host:$port"
    else
      echo "⚠️  Port closed: $host:$port"
    fi
  else
    echo "ℹ️  nc not available; skipped check for $host:$port"
  fi
}

check_curl "http://127.0.0.1:${BACKEND_PORT}/health"
check_curl "http://127.0.0.1:${BAML_PORT}/_debug/ping"
# check_tcp 127.0.0.1 "$MCP_PORT"  # MCP disabled
check_tcp 127.0.0.1 "$ANVIL_PORT"

echo ""
echo "🎉 Backend deployment complete!"
echo ""
echo "📡 Service endpoints:"
echo "   🔧 Backend API:  http://<server-ip>:${BACKEND_PORT}"
echo "   ⛓️  Anvil RPC:    http://<server-ip>:${ANVIL_PORT}"
echo "   (MCP service disabled for simplified deployment)"

echo ""
echo "🏷️  Deployed version: $IMAGE_TAG"
echo ""
echo "📋 To monitor logs: docker compose -f docker/docker-compose-backend.yml logs -f"
echo "🛑 To stop services: docker compose -f docker/docker-compose-backend.yml down"
