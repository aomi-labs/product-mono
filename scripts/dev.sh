#!/bin/bash

# dev.sh - Start forge-mcp services in development mode

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

# Prefer existing shell environment API keys before loading from .env.dev
API_KEYS=(
    "ANTHROPIC_API_KEY" \
    "BRAVE_SEARCH_API_KEY" \
    "ETHERSCAN_API_KEY" \
    "ZEROX_API_KEY"
)

MISSING_KEYS=()
for key in "${API_KEYS[@]}"; do
    if [ -z "${!key}" ]; then
        MISSING_KEYS+=("$key")
    fi
done

if [ ${#MISSING_KEYS[@]} -eq 0 ]; then
    echo "🔑 Using API keys from current shell environment (not loading .env.dev)"
else
    if [ -f "$PROJECT_ROOT/.env.dev" ]; then
        echo "🔑 Loading missing API keys from .env.dev: ${MISSING_KEYS[*]}"
        # Load only missing keys from .env.dev without overriding existing env vars
        while IFS='=' read -r k v; do
            # skip comments and empty lines
            if [[ -z "$k" || "$k" == \#* ]]; then
                continue
            fi
            if [[ " ${MISSING_KEYS[*]} " == *" $k "* ]]; then
                export "$k"="$v"
            fi
        done < "$PROJECT_ROOT/.env.dev"
    else
        echo "⚠️  No .env.dev file found - some API keys are missing: ${MISSING_KEYS[*]}"
    fi
fi

# Load configuration using Python script (for validation display)
echo "🔧 Loading configuration with Python..."
if ! python3 "$SCRIPT_DIR/load_config.py" dev; then
    echo "❌ Configuration loading failed"
    exit 1
fi

# Export port configuration from YAML using Python
eval $(python3 "$SCRIPT_DIR/load_config.py" dev --export-only)


# Get network URLs JSON from Python config
NETWORK_URLS=$(python3 "$SCRIPT_DIR/load_config.py" dev --network-urls-only)
echo "🌐 Network URLs: $NETWORK_URLS"

echo "🧹 Cleaning up existing processes..."
"$SCRIPT_DIR/kill-all.sh"
sleep 1

echo "🔗 Checking Anvil on port 8545..."
if ! nc -z 127.0.0.1 8545 2>/dev/null; then
    echo "🔧 Starting Anvil (forked mainnet)..."

    ANVIL_FORK_URL="${ETH_RPC_URL:-}"
    if [ -z "$ANVIL_FORK_URL" ]; then
        echo "❌ Missing mainnet RPC URL. Set ETH_RPC_URL before running dev.sh."
        echo "   Example: export ETH_RPC_URL=\"https://...\""
        exit 1
    fi

    anvil --fork-url "$ANVIL_FORK_URL" --no-mining --silent &
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
cargo run -p mcp-server -- "$NETWORK_URLS" &
MCP_PID=$!
cd - > /dev/null

echo "MCP Server PID: $MCP_PID"

# Wait for MCP server to start
echo "⏳ Waiting for MCP server to be ready..."
for i in {1..20}; do
    if nc -z ${MCP_SERVER_HOST} ${MCP_SERVER_PORT} 2>/dev/null; then
        echo "✅ MCP server is ready!"
        break
    fi
    if [[ $i -eq 20 ]]; then
        echo "❌ MCP server failed to start within 20 seconds"
        kill $MCP_PID 2>/dev/null || true
        [[ -n "$ANVIL_PID" ]] && kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo "🚀 Starting backend with --no-docs..."

# Start backend in background with no docs for faster startup
cd "$PROJECT_ROOT/chatbot"
cargo run -p backend -- --no-docs &
BACKEND_PID=$!
cd - > /dev/null

echo "Backend PID: $BACKEND_PID"

# Wait for backend to start (check health endpoint)
echo "⏳ Waiting for backend to be ready..."
for i in {1..30}; do
    if curl -s http://localhost:${BACKEND_PORT}/health >/dev/null 2>&1; then
        echo "✅ Backend is ready!"
        break
    fi
    if [[ $i -eq 30 ]]; then
        echo "❌ Backend failed to start within 30 seconds"
        kill $BACKEND_PID $MCP_PID 2>/dev/null || true
        [[ -n "$ANVIL_PID" ]] && kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo "🌐 Starting frontend..."

cd "$PROJECT_ROOT/frontend"
npm run dev &
FRONTEND_PID=$!
cd - > /dev/null

echo "Frontend PID: $FRONTEND_PID"

# Wait for frontend to start
echo "⏳ Waiting for frontend to be ready..."
for i in {1..20}; do
    if curl -s http://localhost:${FRONTEND_PORT} >/dev/null 2>&1; then
        echo "✅ Frontend is ready on port ${FRONTEND_PORT}!"
        break
    fi
    if [[ $i -eq 20 ]]; then
        echo "❌ Frontend failed to start within 20 seconds"
        kill $BACKEND_PID $FRONTEND_PID $MCP_PID 2>/dev/null || true
        [[ -n "$ANVIL_PID" ]] && kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""
echo "🎉 All services are running in development mode!"
echo ""
echo "🔗 Development URLs:"
echo "   MCP Server: ${MCP_SERVER_URL}"
echo "   Backend:    ${BACKEND_URL}"
echo "   Frontend:   ${FRONTEND_URL}"
echo "   Chat Test:  ${FRONTEND_URL}"
echo ""
echo "🔧 Testing backend connection..."
curl -s ${BACKEND_URL}/health && echo " ✅ Backend health check passed"

echo ""
echo "📝 To stop services:"
if [[ -n "$ANVIL_PID" ]]; then
    echo "   kill $MCP_PID $BACKEND_PID $FRONTEND_PID $ANVIL_PID"
else
    echo "   kill $MCP_PID $BACKEND_PID $FRONTEND_PID"
    echo "   (Anvil was already running - stop manually if needed)"
fi
echo ""
echo "🌐 Opening development chat interface..."

# Open the test page
if command -v open >/dev/null 2>&1; then
    open ${FRONTEND_URL}
elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open ${FRONTEND_URL}
else
    echo "Please manually open: ${FRONTEND_URL}"
fi

echo ""
echo "🎯 Ready for development! Try sending messages in the chat interface."
echo "Press Ctrl+C to stop all services..."

# Wait for user to stop
wait
