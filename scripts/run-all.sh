#!/bin/bash

# test-chat-html2.sh - Start backend MCP + agent with aomi-landing frontend

set -e  # Exit on any error

# Load configuration and check environment
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/load-config.sh"

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

# Start MCP server first (backend depends on it)
cd chatbot
cargo run -p mcp-server &
MCP_PID=$!
cd ..

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
        exit 1
    fi
    sleep 1
done


echo "🚀 Starting backend with --no-docs..."

# Start backend in background with no docs for faster startup
cd chatbot
cargo run -p backend -- --no-docs &
BACKEND_PID=$!
cd ..

echo "Backend PID: $BACKEND_PID"

# Wait for backend to start (check health endpoint)
echo "⏳ Waiting for backend to be ready..."
for i in {1..30}; do
    if curl -s ${BACKEND_URL}/health >/dev/null 2>&1; then
        echo "✅ Backend is ready!"
        break
    fi
    if [[ $i -eq 30 ]]; then
        echo "❌ Backend failed to start within 30 seconds"
        kill $BACKEND_PID $MCP_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo "🌐 Starting frontend..."

cd aomi-landing
npm run dev &
FRONTEND_PID=$!
cd ..

echo "Frontend PID: $FRONTEND_PID"

# Wait for frontend to start
echo "⏳ Waiting for frontend to be ready..."
for i in {1..20}; do
    if curl -s ${FRONTEND_URL} >/dev/null 2>&1; then
        echo "✅ Frontend is ready on port ${FRONTEND_PORT}!"
        break
    fi
    if [[ $i -eq 20 ]]; then
        echo "❌ Frontend failed to start within 20 seconds"
        kill $BACKEND_PID $FRONTEND_PID $MCP_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""
echo "🎉 All services are running!"
echo ""
echo "🔗 URLs:"
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
echo "🌐 Opening test chat page..."

# Open the test page
if command -v open >/dev/null 2>&1; then
    open ${FRONTEND_URL}
elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open ${FRONTEND_URL}
else
    echo "Please manually open: ${FRONTEND_URL}"
fi

echo ""
echo "🎯 Ready to test! Try sending messages in the chat interface."
echo "Press Ctrl+C to stop both services..."