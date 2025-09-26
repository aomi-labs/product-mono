#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Get IMAGE_TAG from argument (required)
if [[ $# -lt 1 ]]; then
    echo "❌ Error: IMAGE_TAG is required"
    echo "Usage: $0 <IMAGE_TAG>"
    echo "Example: $0 deployment-ver2"
    echo "Example: $0 latest"
    echo "Example: $0 sha-abc123"
    exit 1
fi

IMAGE_TAG="$1"
export IMAGE_TAG

echo "🚀 Starting backend services deployment..."
echo "🏷️  Using IMAGE_TAG: $IMAGE_TAG"
echo "📡 Using production ports: Backend=8081, MCP=5001, Anvil=8545"

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
docker compose -f "$PROJECT_ROOT/docker/docker-compose-backend.yml" down || true

# Pull latest images from GitHub Container Registry
echo "📥 Pulling images with tag: $IMAGE_TAG..."
docker pull ghcr.io/aomi-labs/product-mono/backend:$IMAGE_TAG || { echo "❌ Failed to pull backend:$IMAGE_TAG"; exit 1; }
docker pull ghcr.io/aomi-labs/product-mono/mcp:$IMAGE_TAG || { echo "❌ Failed to pull mcp:$IMAGE_TAG"; exit 1; }

# Clean up old containers
echo "🧹 Cleaning up old containers..."
docker system prune -f || true

# Start backend services
echo "🚀 Starting backend services..."
echo "📍 Using compose file: $PROJECT_ROOT/docker/docker-compose-backend.yml"

cd "$PROJECT_ROOT"
docker compose -f docker/docker-compose-backend.yml up -d

echo "⏳ Waiting for services to start..."
sleep 10

# Check service status
echo "🔍 Checking service health..."
docker compose -f docker/docker-compose-backend.yml ps

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
echo "   🔧 Backend:   http://localhost:8081"
echo "   🤖 MCP:       http://localhost:5001"
echo "   ⛓️  Anvil:     http://localhost:8545"
echo ""
echo "🏷️  Deployed version: $IMAGE_TAG"
echo ""
echo "📋 To monitor logs: docker compose -f docker/docker-compose-backend.yml logs -f"
echo "🛑 To stop services: docker compose -f docker/docker-compose-backend.yml down"