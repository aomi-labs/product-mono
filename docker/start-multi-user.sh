#!/bin/bash
# start-multi-user.sh - Multi-user startup script for Docker deployment

set -e

echo "🚀 Starting forge-mcp multi-user deployment..."

# Load environment variables
if [ -f /home/forge-mcp/.env.prod ]; then
    source /home/forge-mcp/.env.prod
    echo "✅ Loaded production environment"
else
    echo "⚠️  No .env.prod found - using defaults"
fi

# Set production configuration
export FORGE_ENV=production
export RUST_LOG=warn
export NODE_ENV=production

# Validate required API keys
if [ -z "$ANTHROPIC_API_KEY" ]; then
    echo "❌ ANTHROPIC_API_KEY is required but not set"
    exit 1
fi

echo "🔧 Configuration check..."
python3 /home/forge-mcp/scripts/load_config.py production

# Create log directory
mkdir -p /var/log

echo "🌐 Starting multi-network MCP servers..."
echo "   - testnet (port 5000) -> anvil:8545"
echo "   - mainnet (port 5001) -> $ETH_RPC_URL"
echo "   - polygon (port 5002) -> $POLYGON_RPC_URL"
echo "   - base (port 5003) -> $BASE_RPC_URL"
echo "   - arbitrum (port 5004) -> $ARBITRUM_RPC_URL"
echo "   - linea (port 5005) -> $LINEA_RPC_URL"

echo "🔧 Starting session-aware backend (port 8080)..."
echo "🌐 Starting frontend (port 3000)..."

# Start all services via supervisor
exec /usr/bin/supervisord -c /etc/supervisor/supervisord.conf