#!/bin/bash

# Simple script to start backend + MCP server only

set -e

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Load configuration
echo "🔧 Loading configuration..."
eval $(python3 "$SCRIPT_DIR/load_config.py" dev --export-only)

# Get network URLs
NETWORK_URLS=$(python3 "$SCRIPT_DIR/load_config.py" dev --network-urls-only)

# Clean up existing processes
echo "🧹 Cleaning up existing processes..."
"$SCRIPT_DIR/kill-all.sh"
sleep 1

# Start MCP server
echo "🔧 Starting MCP server on port ${MCP_SERVER_PORT}..."
cd "$PROJECT_ROOT/chatbot"
cargo run -p mcp-server -- "$NETWORK_URLS" &
MCP_PID=$!
cd - > /dev/null

echo "MCP Server PID: $MCP_PID"

# Wait for MCP server
echo "⏳ Waiting for MCP server..."
for i in {1..20}; do
    if nc -z ${MCP_SERVER_HOST} ${MCP_SERVER_PORT} 2>/dev/null; then
        echo "✅ MCP server ready!"
        break
    fi
    if [[ $i -eq 20 ]]; then
        echo "❌ MCP server failed to start"
        kill $MCP_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

# Start backend
echo "🚀 Starting backend..."
cd "$PROJECT_ROOT/chatbot"
cargo run -p backend -- --no-docs &
BACKEND_PID=$!
cd - > /dev/null

echo "Backend PID: $BACKEND_PID"

# Wait for backend
echo "⏳ Waiting for backend..."
for i in {1..30}; do
    if curl -s http://localhost:${BACKEND_PORT}/health >/dev/null 2>&1; then
        echo "✅ Backend ready!"
        break
    fi
    if [[ $i -eq 30 ]]; then
        echo "❌ Backend failed to start"
        kill $BACKEND_PID $MCP_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""
echo "🎉 Backend and MCP server are running!"
echo ""
echo "🔗 URLs:"
echo "   MCP Server: ${MCP_SERVER_URL}"
echo "   Backend:    ${BACKEND_URL}"
echo ""
echo "📝 To stop: kill $MCP_PID $BACKEND_PID"
echo "Press Ctrl+C to stop all services..."

# Wait for user to stop
wait
