#!/bin/bash
# deploy.sh - Production deployment script for Digital Ocean

set -e

echo "🚀 forge-mcp Multi-User Deployment Script"
echo "=========================================="

# Check if we're on the deployment server
if [ -z "$DIGITAL_OCEAN_DEPLOYMENT" ]; then
    echo "⚠️  This script is designed for Digital Ocean deployment"
    echo "Set DIGITAL_OCEAN_DEPLOYMENT=1 to proceed on this server"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check if .env.prod exists
if [ ! -f .env.prod ]; then
    echo "❌ .env.prod file not found!"
    echo "Please create .env.prod with your API keys and RPC URLs"
    echo "You can copy from .env.template if available"
    exit 1
fi

# Validate critical environment variables
source .env.prod
if [ -z "$ANTHROPIC_API_KEY" ]; then
    echo "❌ ANTHROPIC_API_KEY is required in .env.prod"
    exit 1
fi

echo "✅ Environment file validated"

# Stop existing containers if running
echo "🛑 Stopping existing deployment..."
docker-compose -f docker-compose.multi-user.yml down --remove-orphans || true

# Clean up old images (optional)
echo "🧹 Cleaning up old Docker images..."
docker system prune -f || true

# Build new images
echo "🔨 Building multi-user Docker image..."
docker-compose -f docker-compose.multi-user.yml build --no-cache forge-mcp

# Start services
echo "🚀 Starting multi-user services..."
docker-compose -f docker-compose.multi-user.yml up -d

# Wait for services to be ready
echo "⏳ Waiting for services to start..."
sleep 30

# Health checks
echo "🔍 Performing health checks..."

# Check Anvil
if curl -s http://localhost:8545 >/dev/null 2>&1; then
    echo "✅ Anvil (testnet) is ready"
else
    echo "❌ Anvil failed to start"
fi

# Check backend
if curl -s http://localhost:8080/health >/dev/null 2>&1; then
    echo "✅ Backend is ready"
else
    echo "❌ Backend failed to start"
fi

# Check frontend
if curl -s http://localhost:3000 >/dev/null 2>&1; then
    echo "✅ Frontend is ready"
else
    echo "❌ Frontend failed to start"
fi

# Check MCP servers
networks=("testnet:5000" "mainnet:5001" "polygon:5002" "base:5003" "arbitrum:5004" "linea:5005")
for network_port in "${networks[@]}"; do
    network=$(echo $network_port | cut -d: -f1)
    port=$(echo $network_port | cut -d: -f2)
    if nc -z localhost $port 2>/dev/null; then
        echo "✅ MCP $network server (port $port) is ready"
    else
        echo "⚠️  MCP $network server (port $port) not responding"
    fi
done

echo ""
echo "🎉 Multi-user deployment complete!"
echo ""
echo "🔗 Service URLs:"
echo "   Frontend:  http://localhost:3000"
echo "   Backend:   http://localhost:8080"
echo "   Health:    http://localhost:8080/health"
echo ""
echo "🌐 MCP Servers:"
echo "   Testnet:   http://localhost:5000"
echo "   Mainnet:   http://localhost:5001"
echo "   Polygon:   http://localhost:5002"
echo "   Base:      http://localhost:5003"
echo "   Arbitrum:  http://localhost:5004"
echo "   Linea:     http://localhost:5005"
echo ""
echo "📊 Monitor deployment:"
echo "   docker-compose -f docker-compose.multi-user.yml logs -f"
echo ""
echo "🛑 Stop deployment:"
echo "   docker-compose -f docker-compose.multi-user.yml down"
echo ""

# Optional: Set up nginx if requested
read -p "Set up nginx reverse proxy? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "🌐 Starting nginx reverse proxy..."
    docker-compose -f docker-compose.multi-user.yml --profile production up -d nginx
    echo "✅ Nginx is now running on port 80"
    echo "   Access your app at: http://your-server-ip"
fi

echo "✨ Deployment successful! Your multi-user forge-mcp is now running."