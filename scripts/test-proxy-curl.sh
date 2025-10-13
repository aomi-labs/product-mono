#!/usr/bin/env bash
# Smoke-test the public nginx proxy endpoints.
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
BLUE='\033[0;34m'
GRAY='\033[0;90m'
NC='\033[0m'

pass_count=0
fail_count=0
reports=()

resolve_args=()
if [[ -n "$PROXY_IP" ]]; then
  resolve_args+=( "--resolve" "${PROXY_DOMAIN}:443:${PROXY_IP}" )
  resolve_args+=( "--resolve" "${PROXY_DOMAIN}:80:${PROXY_IP}" )
fi

curl_common() {
  local body_file="$1" header_file="$2" url="$3"
  shift 3
  local args=( -sS -m "$TIMEOUT" -w '%{http_code}' -o "$body_file" -D "$header_file" )
  [[ "$INSECURE" == "1" ]] && args+=( -k )
  args+=( -H "Origin: $TEST_ORIGIN" "${resolve_args[@]}" "$@" )
  curl "${args[@]}" "$url" 2>/dev/null || echo "000"
}

section() {
  echo -e "\n${BLUE}$1${NC}\n${BLUE}$(printf '%0.s-' $(seq 1 ${#1}))${NC}"
}

record() {
  local name="$1" outcome="$2" detail="$3"
  if [[ "$outcome" == "pass" ]]; then
    ((pass_count++))
    reports+=( "${GREEN}✔${NC} ${name}${detail}" )
  else
    ((fail_count++))
    reports+=( "${RED}✘${NC} ${name}${detail}" )
  fi
}

http_check() {
  local label="$1" method="$2" path="$3" expected_status="$4" expected_pattern="$5" expected_cors="$6"
  shift 6
  local headers=( "$@" )
  local url="${SCHEME}://${PROXY_DOMAIN}${path}"
  local body_file; body_file=$(mktemp)
  local header_file; header_file=$(mktemp)

  local extra_args=( -X "$method" )
  [[ ${#headers[@]} -gt 0 ]] && extra_args+=( "${headers[@]}" )

  local status body cors_value ok=true detail=""
  status=$(curl_common "$body_file" "$header_file" "$url" "${extra_args[@]}")
  body=$(<"$body_file")
  cors_value=$(grep -i '^access-control-allow-origin:' "$header_file" | tail -1 | cut -d':' -f2- |sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')

  [[ "$status" == "$expected_status" ]] || { ok=false; detail+=" -> expected ${expected_status}"; }
  if [[ "$ok" == true && -n "$expected_pattern" ]]; then
    grep -q "$expected_pattern" <<<"$body" || { ok=false; detail+=" + body"; }
  fi
  if [[ "$ok" == true && -n "$expected_cors" ]]; then
    if [[ "$expected_cors" == "*" ]]; then
      [[ -n "$cors_value" ]] || { ok=false; detail+=" + CORS"; }
    else
      [[ "$cors_value" == "$expected_cors" ]] || { ok=false; detail+=" + CORS"; }
    fi
  fi

  if [[ "$ok" == true ]]; then
    echo -e "${GREEN}✅${NC} ${label} (${status})"
  else
    echo -e "${RED}❌${NC} ${label} (${status})"
  fi
  [[ -n "$cors_value" ]] && echo -e "   CORS: ${GRAY}${cors_value}${NC}"
  [[ -n "$body" ]] && echo -e "${GRAY}${body}${NC}" | sed 's/^/   /'

  record "$label" "$([[ "$ok" == true ]] && echo pass || echo fail)" "$detail"

  rm -f "$body_file" "$header_file"
}

options_check() {
  local label="$1" path="$2"
  local url="${SCHEME}://${PROXY_DOMAIN}${path}"
  local header_file; header_file=$(mktemp)
  local body_file; body_file=$(mktemp)

  local status cors_value
  status=$(curl_common "$body_file" "$header_file" "$url" -X OPTIONS -H "Access-Control-Request-Method: POST")
  cors_value=$(grep -i '^access-control-allow-origin:' "$header_file" | tail -1 | cut -d':' -f2- |
sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')

  if [[ "$status" == "204" ]]; then
    echo -e "${GREEN}✅${NC} ${label} (${status})"
    [[ -n "$cors_value" ]] && echo -e "   CORS: ${GRAY}${cors_value}${NC}"
    record "$label" pass ""
  else
    echo -e "${RED}❌${NC} ${label} (${status})"
    [[ -n "$cors_value" ]] && echo -e "   CORS: ${GRAY}${cors_value}${NC}"
    record "$label" fail " -> expected 204"
  fi

  rm -f "$body_file" "$header_file"
}

json_rpc_payload() {
  local method="$1"
  printf '{"jsonrpc":"2.0","id":1,"method":"%s","params":[]}' "$method"
}

section "Proxy"
echo "Domain     : ${PROXY_DOMAIN}"
[[ -n "$PROXY_IP" ]] && echo "Resolved IP: ${PROXY_IP}"
echo "Origin hdr : ${TEST_ORIGIN}"

options_check "Preflight /api/chat" "/api/chat"
http_check "GET /health" "GET" "/health" 200 "OK" "$TEST_ORIGIN"
http_check "GET /api/state" "GET" "/api/state?session_id=test-smoke" 200 "" "*"
http_check "GET /mcp/health" "GET" "/mcp/health" 401 "Session ID is required" "" -H "Accept: text/event-stream"

payload=$(json_rpc_payload "eth_chainId")
http_check "POST /anvil (eth_chainId)" "POST" "/anvil/" 200 '"jsonrpc":"2.0"' "*" -H "Content-Type: application/json" -d "$payload"
payload=$(json_rpc_payload "eth_blockNumber")
http_check "POST /anvil (eth_blockNumber)" "POST" "/anvil/" 200 '"jsonrpc":"2.0"' "*" -H "Content-Type: application/json" -d "$payload"

section "Summary"
echo -e "${GREEN}${pass_count} passed${NC}, ${fail_count} failed"
for line in "${reports[@]}"; do
  echo -e "$line"
done

(( fail_count == 0 )) || exit 1
