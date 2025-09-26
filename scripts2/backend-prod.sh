#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "🚀 Starting backend services deployment..."

# Load API keys from .env.prod
ENV_FILE="$PROJECT_ROOT/.env.prod"
if [[ -f "$ENV_FILE" ]]; then
    echo "🔑 Loading API keys from $ENV_FILE"
    export $(grep -v '^#' "$ENV_FILE" | xargs)
else
    echo "❌ No .env.prod file found at $ENV_FILE"
    echo "Please create .env.prod with your API keys before running production deployment"
    exit 1
fi

# Stop any existing containers
echo "🛑 Stopping existing containers..."
docker compose -f "$PROJECT_ROOT/docker-compose-backend.yml" down || true

# Pull latest images from GitHub Container Registry
echo "📥 Pulling latest backend images..."
docker pull ghcr.io/aomi-labs/product-mono/backend:deployment-ver2 || echo "⚠️  Could not pull backend image"
docker pull ghcr.io/aomi-labs/product-mono/mcp:deployment-ver2 || echo "⚠️  Could not pull mcp image"

# Clean up old containers
echo "🧹 Cleaning up old containers..."
docker system prune -f || true

# Start backend services
echo "🚀 Starting backend services..."
echo "📍 Using compose file: $PROJECT_ROOT/docker-compose-backend.yml"

cd "$PROJECT_ROOT"
docker compose -f docker-compose-backend.yml up -d

echo "⏳ Waiting for services to start..."
sleep 10

# Check service status
echo "🔍 Checking service health..."
docker compose -f docker-compose-backend.yml ps

# Test if services are responding
echo "🧪 Testing service endpoints..."

# Test backend (port 8081)
if curl -f -s http://localhost:8081/health > /dev/null 2>&1; then
    echo "✅ Backend service is responding on port 8081"
else
    echo "⚠️  Backend service not responding on port 8081"
fi

# Test MCP (port 5001)
if nc -z localhost 5001 2>/dev/null; then
    echo "✅ MCP service is listening on port 5001"
else
    echo "⚠️  MCP service not listening on port 5001"
fi

# Test Anvil (port 8545)
if nc -z localhost 8545 2>/dev/null; then
    echo "✅ Anvil service is listening on port 8545"
else
    echo "⚠️  Anvil service not listening on port 8545"
fi

echo ""
echo "🎉 Backend deployment complete!"
echo ""
echo "📡 Your backend services are available at:"
echo "   🔧 Backend:   http://aomi.dev:8081"
echo "   🤖 MCP:       http://aomi.dev:5001"
echo "   ⛓️  Anvil:     http://aomi.dev:8545"
echo ""
echo "📋 To monitor logs: docker compose -f docker-compose-backend.yml logs -f"
echo "🛑 To stop services: docker compose -f docker-compose-backend.yml down"

