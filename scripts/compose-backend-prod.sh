#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
COMPOSE_FILE="$PROJECT_ROOT/docker/docker-compose-backend.yml"
COMPOSE_DIR="$(dirname "$COMPOSE_FILE")"
DEFAULT_PROJECT_NAME="$(basename "$COMPOSE_DIR")"
export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-$DEFAULT_PROJECT_NAME}"

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

echo "📡 Port configuration:"
echo "   Backend: $BACKEND_PORT"
echo "🧱 Compose project: $COMPOSE_PROJECT_NAME"

echo "🗄️  Database setup will be handled by Docker containers..."
echo "   - PostgreSQL will auto-initialize with required tables"
echo "   - Contract fetching will run after database is ready"

echo "🛑 Stopping existing containers..."
docker compose -f "$COMPOSE_FILE" down || true

echo "📥 Pulling images with tag: $IMAGE_TAG..."
docker pull ghcr.io/aomi-labs/product-mono/backend:$IMAGE_TAG || { echo "❌ Failed to pull backend:$IMAGE_TAG"; exit 1; }

cd "$PROJECT_ROOT"

echo "🧹 Cleaning up old containers..."
docker system prune -f || true

echo "🚀 Starting backend services stack..."
docker compose -f "$COMPOSE_FILE" up --force-recreate -d

echo "⏳ Waiting for services to start..."
sleep 15

echo "🔍 Checking service health..."
docker compose -f "$COMPOSE_FILE" ps

check_curl() {
  local url="$1"
  if curl -f -s "$url" > /dev/null 2>&1; then
    echo "✅ Responding: $url"
  else
    echo "⚠️  No response: $url"
  fi
}

check_curl "http://127.0.0.1:${BACKEND_PORT}/health"

echo ""
echo "🎉 Backend deployment complete!"
echo ""
echo "📡 Service endpoints:"
echo "   🔧 Backend API:  http://<server-ip>:${BACKEND_PORT}"

echo ""
echo "🏷️  Deployed version: $IMAGE_TAG"
echo ""
echo "📋 To monitor logs: docker compose -f \"$COMPOSE_FILE\" logs -f"
echo "🛑 To stop services: docker compose -f \"$COMPOSE_FILE\" down"
