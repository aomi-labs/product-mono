#!/bin/bash
set -e

# ERC-4337 Alto POC Runner Script
# This script starts Anvil, Alto bundler, and runs the POC

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}=====================================${NC}"
echo -e "${BLUE}  ERC-4337 Alto POC Runner${NC}"
echo -e "${BLUE}=====================================${NC}"
echo ""

# Check for .env.aa file
if [ ! -f ".env.aa" ]; then
    echo -e "${RED}Error: .env.aa file not found${NC}"
    echo ""
    echo "Please create .env.aa with:"
    echo "  FORK_URL=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY"
    echo ""
    exit 1
fi

# Load environment variables
export $(grep -v '^#' .env.aa | xargs)

# Verify FORK_URL is set
if [ -z "$FORK_URL" ]; then
    echo -e "${RED}Error: FORK_URL not set in .env.aa${NC}"
    exit 1
fi

echo -e "${GREEN}✓${NC} Environment loaded"

# Start Anvil with fork
echo -e "${YELLOW}Starting Anvil (mainnet fork)...${NC}"
anvil \
  --fork-url "$FORK_URL" \
  --chain-id 1 \
  --port 8545 \
  --accounts 10 \
  --balance 10000 \
  --gas-limit 30000000 \
  > /tmp/anvil-aa.log 2>&1 &
ANVIL_PID=$!

# Wait for Anvil to be ready
echo -e "${YELLOW}Waiting for Anvil to be ready...${NC}"
max_attempts=10
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if cast client --rpc-url http://localhost:8545 > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} Anvil is ready"
        break
    fi

    attempt=$((attempt + 1))
    if [ $attempt -eq $max_attempts ]; then
        echo -e "${RED}Error: Anvil failed to start${NC}"
        kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""

# Start Alto bundler (connects to local Anvil)
echo -e "${YELLOW}Starting Alto bundler...${NC}"
export ANVIL_RPC_URL=http://host.docker.internal:8545
docker compose -f docker/docker-compose-aa.yml --env-file .env.aa up -d

# Wait for Alto to be ready
echo -e "${YELLOW}Waiting for Alto to be ready...${NC}"
max_attempts=30
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if curl -sf http://localhost:4337/health > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} Alto is ready"
        break
    fi

    # Try JSON-RPC method if health check fails
    if curl -sf -X POST http://localhost:4337 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"eth_supportedEntryPoints","params":[]}' \
        > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} Alto is ready"
        break
    fi

    attempt=$((attempt + 1))
    if [ $attempt -eq $max_attempts ]; then
        echo -e "${RED}Error: Alto failed to start${NC}"
        echo ""
        echo "Check logs with:"
        echo "  docker compose -f docker/docker-compose-aa.yml logs alto"
        echo "  cat /tmp/anvil-aa.log"
        kill $ANVIL_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""

# Run POC (connects to local Anvil)
echo -e "${YELLOW}Running POC...${NC}"
echo ""

cd aomi
FORK_URL=http://localhost:8545 RUST_LOG=info cargo run --bin aa-poc

# Success
echo ""
echo -e "${GREEN}=====================================${NC}"
echo -e "${GREEN}  POC Completed Successfully!${NC}"
echo -e "${GREEN}=====================================${NC}"
echo ""
echo "To stop services:"
echo "  docker compose -f docker/docker-compose-aa.yml down"
echo "  kill $ANVIL_PID"
