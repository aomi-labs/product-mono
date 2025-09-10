#!/bin/bash

# test-chat-html.sh - Start backend and frontend for HTML chat testing

set -e  # Exit on any error

echo "🔍 Checking API keys..."

# Check if ANTHROPIC_API_KEY is set
if [[ -z "$ANTHROPIC_API_KEY" ]]; then
    echo "❌ ERROR: ANTHROPIC_API_KEY environment variable is not set"
    echo "Please set it with: export ANTHROPIC_API_KEY=sk-ant-api03-..."
    exit 1
fi

echo "✅ ANTHROPIC_API_KEY is set: ${ANTHROPIC_API_KEY:0:20}..."

# Kill any existing processes on these ports
echo "🧹 Cleaning up existing processes..."
lsof -ti:3000 | xargs kill -9 2>/dev/null || true  # MCP server
lsof -ti:8080 | xargs kill -9 2>/dev/null || true  # Backend
lsof -ti:3001 | xargs kill -9 2>/dev/null || true  # Frontend (alt port)
sleep 1

echo "🔧 Starting MCP server on port 3000..."

# Start MCP server first (backend depends on it)
cd chatbot
cargo run -p mcp-server &
MCP_PID=$!
cd ..

echo "MCP Server PID: $MCP_PID"

# Wait for MCP server to start
echo "⏳ Waiting for MCP server to be ready..."
for i in {1..20}; do
    if nc -z localhost 3000 2>/dev/null; then
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
    if curl -s http://localhost:8080/health >/dev/null 2>&1; then
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

# Start frontend in background
cd frontend
npm run dev &
FRONTEND_PID=$!
cd ..

echo "Frontend PID: $FRONTEND_PID"

# Wait for frontend to start (should be on port 3001)
echo "⏳ Waiting for frontend to be ready..."
FRONTEND_PORT="3001"
for i in {1..20}; do
    if curl -s http://localhost:3001 >/dev/null 2>&1; then
        echo "✅ Frontend is ready on port 3001!"
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
echo "🎉 Both services are running!"
echo ""
echo "🔗 URLs:"
echo "   MCP Server: http://localhost:3000"
echo "   Backend:    http://localhost:8080"
echo "   Frontend:   http://localhost:$FRONTEND_PORT"
echo "   Chat Test:  http://localhost:$FRONTEND_PORT/test-chat.html"
echo ""
echo "🔧 Testing backend connection..."
curl -s http://localhost:8080/health && echo " ✅ Backend health check passed"

echo ""
echo "📝 To stop services:"
echo "   kill $MCP_PID $BACKEND_PID $FRONTEND_PID"
echo ""
echo "🌐 Opening test chat page..."

# Open the test page
if command -v open >/dev/null 2>&1; then
    open http://localhost:$FRONTEND_PORT/test-chat.html
elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open http://localhost:$FRONTEND_PORT/test-chat.html
else
    echo "Please manually open: http://localhost:$FRONTEND_PORT/test-chat.html"
fi

echo ""
echo "🎯 Ready to test! Try sending messages in the chat interface."
echo "Press Ctrl+C to stop both services..."

# Function to cleanup on script exit
cleanup() {
    echo ""
    echo "🛑 Stopping services..."
    kill $MCP_PID $BACKEND_PID $FRONTEND_PID 2>/dev/null || true
    echo "✅ Services stopped."
}

trap cleanup EXIT

# Wait for user to stop
wait