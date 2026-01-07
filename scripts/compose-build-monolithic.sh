#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Parse arguments
AOMI_DOMAIN="localhost"
MODE="prod"  # Default to prod

for arg in "$@"; do
    if [[ "$arg" == "-prod" ]]; then
        MODE="prod"
    elif [[ "$arg" == "-dev" ]]; then
        MODE="dev"
    elif [[ "$arg" != "-"* ]]; then
        AOMI_DOMAIN="$arg"
    fi
done

# Set ports based on mode
if [[ "$MODE" == "dev" ]]; then
    export BACKEND_PORT=8080
    # export MCP_PORT=5000
    export FRONTEND_INTERNAL_PORT=3000
    export FRONTEND_EXTERNAL_PORT=3000
    echo "🔧 Running in DEVELOPMENT mode"
else
    export BACKEND_PORT=8081
    # export MCP_PORT=5001
    export FRONTEND_INTERNAL_PORT=3001
    export FRONTEND_EXTERNAL_PORT=80
    echo "🚀 Running in PRODUCTION mode"
fi

export AOMI_DOMAIN

echo "🌐 Using AOMI_DOMAIN: $AOMI_DOMAIN"
echo "📡 Port configuration:"
echo "   Backend: $BACKEND_PORT"
# echo "   MCP: $MCP_PORT"
echo "   Frontend: $FRONTEND_EXTERNAL_PORT (internal: $FRONTEND_INTERNAL_PORT)"

if [[ "$AOMI_DOMAIN" == "localhost" ]]; then
    echo "📝 Note: No domain argument provided, using localhost. Frontend will be accessible at http://localhost:$FRONTEND_EXTERNAL_PORT"
fi

# Check if running on Digital Ocean (optional check)
if [[ -f /etc/digitalocean_hostname ]]; then
    echo "✅ Detected Digital Ocean environment"
fi

# Load API keys from .env.prod
ENV_FILE="$PROJECT_ROOT/.env.prod"
if [[ -f "$ENV_FILE" ]]; then
    echo "🔑 Loading API keys from $ENV_FILE"
    export $(grep -v '^#' "$ENV_FILE" | xargs)
else
    echo "❌ No .env.prod file found at $ENV_FILE"
    echo "Please create .env.prod with your API keys before running deployment"
    exit 1
fi

# Stop any existing containers
echo "🛑 Stopping existing containers..."
docker compose -f "$PROJECT_ROOT/docker/docker-compose-monolithic.yml" down || true

# Clean up old containers and images (optional)
echo "🧹 Cleaning up old containers..."
docker system prune -f || true

# Build and start containers
echo "🔨 Building containers with AOMI_DOMAIN=$AOMI_DOMAIN..."
echo "📍 Using compose file: $PROJECT_ROOT/docker/docker-compose-monolithic.yml"

cd "$PROJECT_ROOT"

# Build and start with configuration
docker compose -f docker/docker-compose-monolithic.yml build --build-arg AOMI_DOMAIN=$AOMI_DOMAIN
docker compose -f docker/docker-compose-monolithic.yml up -d

echo "⏳ Waiting for services to start..."
sleep 10

# Check service status
echo "🔍 Checking service health..."
docker compose -f docker/docker-compose-monolithic.yml ps

# Test if services are responding
echo "🧪 Testing service endpoints..."

# Test frontend
if curl -f -s http://localhost:$FRONTEND_EXTERNAL_PORT > /dev/null; then
    echo "✅ Frontend service is responding on port $FRONTEND_EXTERNAL_PORT"
else
    echo "⚠️  Frontend service not responding on port $FRONTEND_EXTERNAL_PORT"
fi

# Test backend
if curl -f -s http://localhost:$BACKEND_PORT/health > /dev/null; then
    echo "✅ Backend service is responding on port $BACKEND_PORT"
else
    echo "⚠️  Backend service not responding on port $BACKEND_PORT"
fi

# # Test MCP
# if nc -z localhost $MCP_PORT 2>/dev/null; then
#     echo "✅ MCP service is listening on port $MCP_PORT"
# else
#     echo "⚠️  MCP service not listening on port $MCP_PORT"
# fi

# Test Anvil
if nc -z localhost 8545 2>/dev/null; then
    echo "✅ Anvil service is listening on port 8545"
else
    echo "⚠️  Anvil service not listening on port 8545"
fi

echo ""
echo "🎉 Deployment complete!"
echo ""
echo "📡 Your services are available at:"
if [[ "$AOMI_DOMAIN" == "localhost" ]]; then
    echo "   🌐 Frontend:  http://localhost:$FRONTEND_EXTERNAL_PORT"
    echo "   🔧 Backend:   http://localhost:$BACKEND_PORT"
    # echo "   🔧 MCP:       http://localhost:$MCP_PORT"
else
    if [[ "$FRONTEND_EXTERNAL_PORT" == "80" ]]; then
        echo "   🌐 Frontend:  http://$AOMI_DOMAIN"
    else
        echo "   🌐 Frontend:  http://$AOMI_DOMAIN:$FRONTEND_EXTERNAL_PORT"
    fi
    echo "   🔧 Backend:   http://$AOMI_DOMAIN:$BACKEND_PORT"
    # echo "   🔧 MCP:       http://$AOMI_DOMAIN:$MCP_PORT"
fi
echo "   ⛓️  Anvil:     http://${AOMI_DOMAIN}:8545"
echo ""
echo "📋 To monitor logs: docker compose -f docker/docker-compose-monolithic.yml logs -f"
echo "🛑 To stop services: docker compose -f docker/docker-compose-monolithic.yml down"
echo ""
echo "Usage examples:"
echo "  ./scripts/compose-build-monolithic.sh              # prod mode with localhost"
echo "  ./scripts/compose-build-monolithic.sh -dev         # dev mode with localhost"
echo "  ./scripts/compose-build-monolithic.sh aomi.dev     # prod mode with aomi.dev"
echo "  ./scripts/compose-build-monolithic.sh -dev aomi.dev # dev mode with aomi.dev"
