#!/usr/bin/env bash
# Smoke-test the HTTPS proxy (nginx) endpoints served from AOMI_API_DOMAIN.
# Usage: ./scripts/test-proxy-curl.sh <domain> [override-ip]

set -uo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <domain> [ip]" >&2
  exit 1
fi

PROXY_DOMAIN="$1"
PROXY_IP="${2:-${PROXY_IP:-}}"
SCHEME="${PROXY_SCHEME:-https}"
TIMEOUT="${CURL_TIMEOUT:-10}"
INSECURE="${CURL_INSECURE:-0}"
TEST_ORIGIN="${PROXY_TEST_ORIGIN:-https://${PROXY_DOMAIN}}"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
GRAY='\033[0;90m'
NC='\033[0m'

pass_count=0
fail_count=0
reports=()

resolve_args=()
if [[ -n "$PROXY_IP" ]]; then
  resolve_args+=("--resolve" "${PROXY_DOMAIN}:443:${PROXY_IP}" "--resolve" "${PROXY_DOMAIN}:80:${PROXY_IP}")
fi

curl_common() {
  local body_file="$1"
  local header_file="$2"
  local url="$3"
  shift 3
  local args=("-sS" "-m" "$TIMEOUT" "-w" "%{http_code}" "-o" "$body_file" "-D" "$header_file")
  [[ "$INSECURE" == "1" ]] && args+=("-k")
  args+=("-H" "Origin: $TEST_ORIGIN")
  args+=("${resolve_args[@]}")
  args+=("$@")
  curl "${args[@]}" "$url" 2>/dev/null || echo "000"
}

section() {
  echo -e "\n${BLUE}$1${NC}\n${BLUE}$(printf '%0.s-' $(seq 1 ${#1}))${NC}"
}

record() {
  local name="$1"; shift
  local outcome="$1"; shift
  local detail="$1"
  if [[ "$outcome" == "pass" ]]; then
    ((pass_count++))
    reports+=("${GREEN}✔${NC} ${name}${detail}")
  else
    ((fail_count++))
    reports+=("${RED}✘${NC} ${name}${detail}")
  fi
}

trim() {
  echo "$1" | sed 's/^ *//;s/ *$//'
}

http_check() {
  local label="$1"
  local method="$2"
  local path="$3"
  local expected_status="${4:-200}"
  local expected_pattern="${5:-}"
  local expected_cors="${6:-}"
  local data="${7:-}"

  local url="${SCHEME}://${PROXY_DOMAIN}${path}"
  local body_file
  body_file=$(mktemp)
  local header_file
  header_file=$(mktemp)

  local status
  local extra_args=("-X" "$method")
  if [[ -n "$data" ]]; then
    extra_args+=("-H" "Content-Type: application/json" "-d" "$data")
  fi
  status=$(curl_common "$body_file" "$header_file" "$url" "${extra_args[@]}")

  local body
  body=$(<"$body_file")
  local cors_value
  cors_value=$(grep -i '^access-control-allow-origin:' "$header_file" | tail -1 | cut -d':' -f2- | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')

  local ok=true
  if [[ "$status" != "$expected_status" ]]; then
    ok=false
  fi
  if [[ "$ok" == true && -n "$expected_pattern" ]]; then
    if ! grep -q "$expected_pattern" <<<"$body"; then
      ok=false
    fi
  fi
  if [[ "$ok" == true && -n "$expected_cors" ]]; then
    if [[ "$expected_cors" == "*" ]]; then
      [[ -n "$cors_value" ]] || ok=false
    else
      [[ "$cors_value" == "$expected_cors" ]] || ok=false
    fi
  fi

  if [[ "$ok" == true ]]; then
    echo -e "${GREEN}✅${NC} ${label} (${status})"
    [[ -n "$cors_value" ]] && echo -e "   CORS: ${GRAY}${cors_value}${NC}"
    if [[ -n "$body" ]]; then
      echo -e "${GRAY}${body}${NC}" | sed 's/^/   /'
    fi
    record "$label" pass ""
  else
    echo -e "${RED}❌${NC} ${label} (${status})"
    [[ -n "$cors_value" ]] && echo -e "   CORS: ${GRAY}${cors_value}${NC}"
    if [[ -n "$body" ]]; then
      echo -e "${GRAY}${body}${NC}" | sed 's/^/   /'
    fi
    local detail=" -> expected ${expected_status}"
    [[ -n "$expected_pattern" ]] && detail+=" + pattern"
    [[ -n "$expected_cors" ]] && detail+=" + CORS"
    record "$label" fail "$detail"
  fi

  rm -f "$body_file" "$header_file"
}

options_check() {
  local label="$1"
  local path="$2"
  local url="${SCHEME}://${PROXY_DOMAIN}${path}"
  local header_file
  header_file=$(mktemp)
  local body_file
  body_file=$(mktemp)

  local status
  local args=("-X" "OPTIONS" "-H" "Origin: $TEST_ORIGIN" "-H" "Access-Control-Request-Method: POST")
  status=$(curl_common "$body_file" "$header_file" "$url" "${args[@]}")
  local cors_value
  cors_value=$(grep -i '^access-control-allow-origin:' "$header_file" | tail -1 | cut -d':' -f2- | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')

  if [[ "$status" == "204" && -n "$cors_value" ]]; then
    echo -e "${GREEN}✅${NC} ${label} (${status})"
    echo -e "   CORS: ${GRAY}${cors_value}${NC}"
    record "$label" pass ""
  else
    echo -e "${RED}❌${NC} ${label} (${status})"
    [[ -n "$cors_value" ]] && echo -e "   CORS: ${GRAY}${cors_value}${NC}"
    record "$label" fail " -> expected 204 + CORS"
  fi

  rm -f "$body_file" "$header_file"
}

json_rpc_payload() {
  local method="$1"
  cat <<JSON
{"jsonrpc":"2.0","id":1,"method":"${method}","params":[]}
JSON
}

section "Proxy"
echo "Domain     : ${PROXY_DOMAIN}"
[[ -n "$PROXY_IP" ]] && echo "Resolved IP: ${PROXY_IP}"
echo "Origin hdr : ${TEST_ORIGIN}"

options_check "Preflight /api/chat" "/api/chat"
http_check "GET /health" "GET" "/health" 200 "OK" "*"
http_check "GET /api/state" "GET" "/api/state?session_id=test-smoke" 200 "" "*"
http_check "GET /mcp/health" "GET" "/mcp/health" 200 "" "*"

json_payload=$(json_rpc_payload "eth_chainId")
http_check "POST /anvil (eth_chainId)" "POST" "/anvil/" 200 '"jsonrpc":"2.0"' "*" "$json_payload"
json_payload=$(json_rpc_payload "eth_blockNumber")
http_check "POST /anvil (eth_blockNumber)" "POST" "/anvil/" 200 '"jsonrpc":"2.0"' "*" "$json_payload"

section "Summary"
echo -e "${GREEN}${pass_count} passed${NC}, ${fail_count} failed"
for line in "${reports[@]}"; do
  echo -e "$line"
done

if (( fail_count > 0 )); then
  exit 1
fi
