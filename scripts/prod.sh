#!/bin/bash

# prod.sh - Start forge-mcp services in production mode

set -e  # Exit on any error

# Check for Python and install dependencies if needed
if ! command -v python3 >/dev/null 2>&1; then
    echo "❌ Python3 is required but not installed"
    echo "Install Python3 first: https://python.org"
    exit 1
fi

# Install Python dependencies if needed
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

if [ ! -f "$PROJECT_ROOT/.venv/bin/activate" ]; then
    echo "🐍 Setting up Python virtual environment..."
    cd "$PROJECT_ROOT"
    python3 -m venv .venv
    source .venv/bin/activate
    pip install -r requirements.txt
    cd - > /dev/null
else
    source "$PROJECT_ROOT/.venv/bin/activate"
fi

# Load API keys from .env.prod using dotenv
if [ -f "$PROJECT_ROOT/.env.prod" ]; then
    echo "🔑 Loading API keys from .env.prod..."
    source "$PROJECT_ROOT/.venv/bin/activate"
    export $(cat "$PROJECT_ROOT/.env.prod" | grep -v '^#' | xargs)
else
    echo "⚠️  No .env.prod file found - API keys not loaded"
fi

# Load configuration using Python script (for validation display)
echo "🔧 Loading configuration with Python..."
if ! python3 "$SCRIPT_DIR/load_config.py" prod; then
    echo "❌ Configuration loading failed"
    exit 1
fi

# Export port configuration from YAML using Python
eval $(python3 "$SCRIPT_DIR/load_config.py" prod --export-only)

# Get network URLs JSON from Python config
NETWORK_URLS=$(python3 "$SCRIPT_DIR/load_config.py" prod --network-urls-only)
echo "🌐 Network URLs: $NETWORK_URLS"

echo "🧹 Cleaning up existing processes..."
lsof -ti:${MCP_SERVER_PORT} | xargs kill -9 2>/dev/null || true  # MCP server
lsof -ti:${BACKEND_PORT} | xargs kill -9 2>/dev/null || true  # Backend
lsof -ti:${FRONTEND_PORT} | xargs kill -9 2>/dev/null || true  # Frontend
sleep 1

echo "🔗 Checking Anvil on port 8545..."
if ! nc -z 127.0.0.1 8545 2>/dev/null; then
    echo "🔧 Starting Anvil (forked mainnet)..."
    anvil --fork-url https://eth-mainnet.public.blastapi.io@22419684 &
    ANVIL_PID=$!
    echo "Anvil PID: $ANVIL_PID"
    
    # Wait for Anvil to start
    echo "⏳ Waiting for Anvil to be ready..."
    for i in {1..15}; do
        if nc -z 127.0.0.1 8545 2>/dev/null; then
            echo "✅ Anvil is ready!"
            break
        fi
        if [[ $i -eq 15 ]]; then
            echo "❌ Anvil failed to start within 15 seconds"
            kill $ANVIL_PID 2>/dev/null || true
            exit 1
        fi
        sleep 1
    done
else
    echo "✅ Anvil is already running on port 8545"
    ANVIL_PID=""
fi

echo "🔧 Starting MCP server on port ${MCP_SERVER_PORT}..."

# Start MCP server with network URLs configuration
cd "$PROJECT_ROOT/chatbot"
cargo run -p mcp-server --release -- "$NETWORK_URLS" &
MCP_PID=$!
cd - > /dev/null

echo "MCP Server PID: $MCP_PID"

# Wait for MCP server to start
echo "⏳ Waiting for MCP server to be ready..."
for i in {1..30}; do
    if nc -z ${MCP_SERVER_HOST} ${MCP_SERVER_PORT} 2>/dev/null; then
        echo "✅ MCP server is ready!"
        break
    fi
    if [[ $i -eq 30 ]]; then
        echo "❌ MCP server failed to start within 30 seconds"
        kill $MCP_PID 2>/dev/null || true
        [[ -n "$ANVIL_PID" ]] && kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo "🚀 Starting backend in production mode..."

# Start backend in production mode (with docs for better experience)
cd "$PROJECT_ROOT/chatbot"
cargo run -p backend --release &
BACKEND_PID=$!
cd - > /dev/null

echo "Backend PID: $BACKEND_PID"

# Wait for backend to start (check health endpoint)
echo "⏳ Waiting for backend to be ready..."
for i in {1..60}; do
    if curl -s http://localhost:${BACKEND_PORT}/health >/dev/null 2>&1; then
        echo "✅ Backend is ready!"
        break
    fi
    if [[ $i -eq 60 ]]; then
        echo "❌ Backend failed to start within 60 seconds"
        kill $BACKEND_PID $MCP_PID 2>/dev/null || true
        [[ -n "$ANVIL_PID" ]] && kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo "🌐 Starting frontend in production mode..."

cd "$PROJECT_ROOT/aomi-landing"
npm run build && npm run preview -- --port ${FRONTEND_PORT} --host 0.0.0.0 &
FRONTEND_PID=$!
cd - > /dev/null

echo "Frontend PID: $FRONTEND_PID"

# Wait for frontend to start
echo "⏳ Waiting for frontend to be ready..."
for i in {1..30}; do
    if curl -s http://localhost:${FRONTEND_PORT} >/dev/null 2>&1; then
        echo "✅ Frontend is ready on port ${FRONTEND_PORT}!"
        break
    fi
    if [[ $i -eq 30 ]]; then
        echo "❌ Frontend failed to start within 30 seconds"
        kill $BACKEND_PID $FRONTEND_PID $MCP_PID 2>/dev/null || true
        [[ -n "$ANVIL_PID" ]] && kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""
echo "🎉 All services are running in production mode!"
echo ""
echo "🔗 Production URLs:"
echo "   MCP Server: ${MCP_SERVER_URL}"
echo "   Backend:    ${BACKEND_URL}"
echo "   Frontend:   ${FRONTEND_URL}"
echo ""
echo "🔧 Testing backend connection..."
curl -s http://localhost:${BACKEND_PORT}/health && echo " ✅ Backend health check passed"

echo ""
echo "📝 To stop services:"
if [[ -n "$ANVIL_PID" ]]; then
    echo "   kill $MCP_PID $BACKEND_PID $FRONTEND_PID $ANVIL_PID"
else
    echo "   kill $MCP_PID $BACKEND_PID $FRONTEND_PID"
    echo "   (Anvil was already running - stop manually if needed)"
fi
echo ""
echo "🌐 Services are accessible externally on all interfaces (0.0.0.0)"
echo ""
echo "🎯 Production ready! Services are optimized and built for performance."
echo "Press Ctrl+C to stop all services..."

# Wait for user to stop
wait