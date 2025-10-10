#!/usr/bin/env bash
# Smoke-test the raw backend/MCP/Anvil stack over HTTP.
# Usage: ./scripts/test-backend-curl.sh [host]

set -uo pipefail

TARGET_HOST="${1:-127.0.0.1}"
BACKEND_PORT="${BACKEND_PORT:-8081}"
MCP_PORT="${MCP_PORT:-5001}"
ANVIL_PORT="${ANVIL_PORT:-8545}"
TIMEOUT="${CURL_TIMEOUT:-10}"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
GRAY='\033[0;90m'
NC='\033[0m'

pass_count=0
fail_count=0
reports=()

section() {
  echo -e "\n${BLUE}$1${NC}\n${BLUE}$(printf '%0.s-' $(seq 1 ${#1}))${NC}"
}

record() {
  local name="$1"
  local outcome="$2"
  local detail="$3"
  if [[ "$outcome" == "pass" ]]; then
    ((pass_count++))
    reports+=("${GREEN}✔${NC} ${name}${detail}")
  else
    ((fail_count++))
    reports+=("${RED}✘${NC} ${name}${detail}")
  fi
}

http_check() {
  local label="$1"
  local method="$2"
  local url="$3"
  local expected_status="${4:-200}"
  local expected_pattern="${5:-}"
  local data="${6:-}"

  local body_file
  body_file=$(mktemp)
  local status
  local curl_args=("-sS" "-m" "$TIMEOUT" "-w" "%{http_code}" "-o" "$body_file" "-X" "$method")
  if [[ -n "$data" ]]; then
    curl_args+=("-H" "Content-Type: application/json" "-d" "$data")
  fi

  status=$(curl "${curl_args[@]}" "$url" 2>/dev/null || echo "000")
  local body
  body=$(<"$body_file")

  if [[ "$status" == "$expected_status" ]] && { [[ -z "$expected_pattern" ]] || grep -q "$expected_pattern" <<<"$body"; }; then
    echo -e "${GREEN}✅${NC} ${label} (${status})"
    [[ -n "$body" ]] && echo -e "${GRAY}${body}${NC}" | sed 's/^/   /'
    record "$label" pass ""
  else
    echo -e "${RED}❌${NC} ${label} (${status})"
    [[ -n "$body" ]] && echo -e "${GRAY}${body}${NC}" | sed 's/^/   /'
    record "$label" fail " -> expected ${expected_status}${expected_pattern:+ and "$expected_pattern"}" 
  fi
  rm -f "$body_file"
}

port_check() {
  local label="$1"
  local port="$2"
  if command -v nc >/dev/null 2>&1; then
    if nc -z "$TARGET_HOST" "$port" >/dev/null 2>&1; then
      echo -e "${GREEN}✅${NC} ${label} port ${port} open"
      record "$label port ${port}" pass ""
    else
      echo -e "${RED}❌${NC} ${label} port ${port} closed"
      record "$label port ${port}" fail ""
    fi
  else
    echo -e "${YELLOW}⚠${NC} nc not available; skipped ${label} port check"
  fi
}

json_rpc() {
  local label="$1"
  local method="$2"
  local payload
  payload=$(cat <<JSON
{"jsonrpc":"2.0","id":1,"method":"${method}","params":[]}
JSON
  )
  http_check "$label" "POST" "http://${TARGET_HOST}:${ANVIL_PORT}" 200 '"jsonrpc":"2.0"' "$payload"
}

section "TCP Ports"
port_check "Backend" "$BACKEND_PORT"
port_check "MCP" "$MCP_PORT"
port_check "Anvil" "$ANVIL_PORT"

section "Backend"
http_check "Backend /health" "GET" "http://${TARGET_HOST}:${BACKEND_PORT}/health" 200 "OK"
http_check "Backend /api/state" "GET" "http://${TARGET_HOST}:${BACKEND_PORT}/api/state?session_id=test-smoke" 200

section "Anvil JSON-RPC"
json_rpc "eth_chainId" "eth_chainId"
json_rpc "eth_blockNumber" "eth_blockNumber"
json_rpc "eth_accounts" "eth_accounts"

section "Summary"
echo -e "${GREEN}${pass_count} passed${NC}, ${fail_count} failed"
for line in "${reports[@]}"; do
  echo -e "$line"
done

if (( fail_count > 0 )); then
  exit 1
fi
