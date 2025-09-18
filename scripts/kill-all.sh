#!/bin/bash

# kill-all.sh - Stop all forge-mcp services

set -e

# Load configuration to get the ports
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Prefer Python config loader to export ports; ignore failures and fall back
if command -v python3 >/dev/null 2>&1 && [ -f "$SCRIPT_DIR/load_config.py" ]; then
    # Export only, for development environment
    eval $(python3 "$SCRIPT_DIR/load_config.py" dev --export-only) || true
fi

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}🛑 Stopping all forge-mcp services...${NC}"

# Function to kill processes on a port
kill_port() {
    local port=$1
    local service_name=$2
    
    local pids=$(lsof -ti:${port} 2>/dev/null || true)
    if [[ -n "$pids" ]]; then
        echo -e "  ${YELLOW}Stopping ${service_name} (port ${port})...${NC}"
        echo "$pids" | xargs kill -TERM 2>/dev/null || true
        sleep 1
        # Force kill if still running
        local remaining_pids=$(lsof -ti:${port} 2>/dev/null || true)
        if [[ -n "$remaining_pids" ]]; then
            echo -e "  ${RED}Force killing ${service_name}...${NC}"
            echo "$remaining_pids" | xargs kill -9 2>/dev/null || true
        fi
        echo -e "  ${GREEN}✅ ${service_name} stopped${NC}"
    else
        echo -e "  ${GREEN}✅ ${service_name} (port ${port}) - not running${NC}"
    fi
}

# Provide defaults if not set in environment
: "${MCP_SERVER_PORT:=5000}"
: "${BACKEND_PORT:=8080}"
: "${FRONTEND_PORT:=3000}"
: "${ANVIL_PORT:=8545}"

# Kill services by port
kill_port ${MCP_SERVER_PORT} "MCP Server"
kill_port ${BACKEND_PORT} "Backend API"  
kill_port ${FRONTEND_PORT} "Frontend"
kill_port ${ANVIL_PORT} "Anvil (Ethereum)"

# Also kill any cargo processes that might be running our services
echo -e "${YELLOW}🔍 Checking for remaining cargo processes...${NC}"
CARGO_PIDS=$(pgrep -f "cargo run.*mcp-server\|cargo run.*backend" 2>/dev/null || true)
if [[ -n "$CARGO_PIDS" ]]; then
    echo -e "  ${YELLOW}Stopping cargo processes...${NC}"
    echo "$CARGO_PIDS" | xargs kill -TERM 2>/dev/null || true
    sleep 1
    # Force kill remaining cargo processes
    REMAINING_CARGO=$(pgrep -f "cargo run.*mcp-server\|cargo run.*backend" 2>/dev/null || true)
    if [[ -n "$REMAINING_CARGO" ]]; then
        echo -e "  ${RED}Force killing cargo processes...${NC}"
        echo "$REMAINING_CARGO" | xargs kill -9 2>/dev/null || true
    fi
    echo -e "  ${GREEN}✅ Cargo processes stopped${NC}"
else
    echo -e "  ${GREEN}✅ No cargo processes running${NC}"
fi

# Kill any remaining vite processes (frontend)
echo -e "${YELLOW}🔍 Checking for Vite processes...${NC}"
VITE_PIDS=$(pgrep -f "vite.*dev" 2>/dev/null || true)
if [[ -n "$VITE_PIDS" ]]; then
    echo -e "  ${YELLOW}Stopping Vite processes...${NC}"
    echo "$VITE_PIDS" | xargs kill -TERM 2>/dev/null || true
    sleep 1
    # Force kill remaining vite processes
    REMAINING_VITE=$(pgrep -f "vite.*dev" 2>/dev/null || true)
    if [[ -n "$REMAINING_VITE" ]]; then
        echo -e "  ${RED}Force killing Vite processes...${NC}"
        echo "$REMAINING_VITE" | xargs kill -9 2>/dev/null || true
    fi
    echo -e "  ${GREEN}✅ Vite processes stopped${NC}"
else
    echo -e "  ${GREEN}✅ No Vite processes running${NC}"
fi

echo ""
echo -e "${GREEN}🎉 All forge-mcp services have been stopped!${NC}"
echo ""
echo -e "${YELLOW}📋 Stopped services:${NC}"
echo -e "   MCP Server (port ${MCP_SERVER_PORT})"
echo -e "   Backend API (port ${BACKEND_PORT})"
echo -e "   Frontend (port ${FRONTEND_PORT})"
echo -e "   Anvil Ethereum (port ${ANVIL_PORT})"
echo ""
echo -e "${YELLOW}💡 To start services again, run:${NC}"
echo -e "   ${GREEN}./test-chat-html2.sh${NC}"