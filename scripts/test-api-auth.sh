#!/usr/bin/env bash
# Admin CLI + curl auth checks for backend endpoints.
# Usage: ./scripts/test-api-auth.sh [server-url]

set -uo pipefail

SERVER_URL="${SERVER_URL:-}"
TIMEOUT="${CURL_TIMEOUT:-10}"
ADMIN_CLI_BIN="${ADMIN_CLI_BIN:-}"
ADMIN_DATABASE_URL="${ADMIN_DATABASE_URL:-}"
VERBOSE=0
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT_DIR=$(cd "${SCRIPT_DIR}/.." && pwd)

usage() {
  cat <<'EOF'
Usage: ./scripts/test-api-auth.sh [options] [server-url]

Options:
  -v, --verbose   Enable step-by-step logs
  -h, --help      Show this help message

Environment:
  SERVER_URL          Server base URL (default http://localhost:8080)
  ADMIN_CLI_BIN       Path to admin-cli binary (default: cargo run -p admin-cli --quiet --)
  ADMIN_DATABASE_URL  DB connection string to pass to admin-cli
  CURL_TIMEOUT        Curl timeout in seconds (default 10)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -v|--verbose)
      VERBOSE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      if [[ -z "$SERVER_URL" ]]; then
        SERVER_URL="$1"
      else
        echo "Unexpected argument: $1" >&2
        usage >&2
        exit 1
      fi
      shift
      ;;
  esac
done

if [[ -z "$SERVER_URL" ]]; then
  SERVER_URL="http://localhost:8080"
fi

SERVER_URL="${SERVER_URL%/}"

ADMIN_CLI=()
if [[ -n "$ADMIN_CLI_BIN" ]]; then
  ADMIN_CLI=("$ADMIN_CLI_BIN")
else
  ADMIN_CLI=(cargo run --manifest-path "${ROOT_DIR}/aomi/Cargo.toml" -p admin-cli --quiet --)
fi

DB_ARGS=()
if [[ -n "$ADMIN_DATABASE_URL" ]]; then
  DB_ARGS+=(--database-url "$ADMIN_DATABASE_URL")
fi

API_KEY_HEADER="X-API-Key"
SESSION_ID_HEADER="X-Session-Id"

GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
GRAY='\033[0;90m'
NC='\033[0m'

pass_count=0
fail_count=0
reports=()

log() {
  if [[ "$VERBOSE" == "1" ]]; then
    echo "[""$(date +%H:%M:%S)""] $*"
  fi
}

section() {
  echo -e "\n${BLUE}$1${NC}\n${BLUE}$(printf '%0.s-' $(seq 1 ${#1}))${NC}"
}

record() {
  local name="$1" outcome="$2" detail="$3"
  if [[ "$outcome" == "pass" ]]; then
    ((pass_count++))
    reports+=("${GREEN}PASS${NC} ${name}${detail}")
  else
    ((fail_count++))
    reports+=("${RED}FAIL${NC} ${name}${detail}")
  fi
}

create_key() {
  local label="$1" namespaces="$2" key_value="$3"
  local db_args=()

  log "Creating API key: ${label} (namespaces=${namespaces}, key=${key_value})"
  if [[ ${DB_ARGS+x} && ${#DB_ARGS[@]} -gt 0 ]]; then
    db_args=("${DB_ARGS[@]}")
  fi
  if [[ ${#db_args[@]} -gt 0 ]]; then
    "${ADMIN_CLI[@]}" "${db_args[@]}" api-keys create -n "$namespaces" -l "$label" -k "$key_value" >/dev/null 2>&1
  else
    "${ADMIN_CLI[@]}" api-keys create -n "$namespaces" -l "$label" -k "$key_value" >/dev/null 2>&1
  fi
  log "Created key: ${label} -> ${key_value}"
  echo "$key_value"
}

http_check() {
  local label="$1" method="$2" url="$3" expected="$4"
  shift 4

  local body_file header_file status ok=false detail=""
  body_file=$(mktemp)
  header_file=$(mktemp)

  log "Request: ${method} ${url} (expected ${expected})"
  status=$(curl -sS -m "$TIMEOUT" -o "$body_file" -D "$header_file" -w '%{http_code}' -X "$method" "$@" "$url" 2>/dev/null || echo "000")

  IFS=',' read -r -a expected_list <<<"$expected"
  for code in "${expected_list[@]}"; do
    if [[ "$status" == "$code" ]]; then
      ok=true
      break
    fi
  done

  if [[ "$ok" == true ]]; then
    echo -e "${GREEN}OK${NC}  ${label} (${status})"
  else
    echo -e "${RED}ERR${NC} ${label} (${status})"
    if [[ -s "$body_file" ]]; then
      echo -e "${GRAY}$(<"$body_file")${NC}" | sed 's/^/   /'
    fi
    detail+=" -> expected ${expected}"
  fi

  record "$label" "$([[ "$ok" == true ]] && echo pass || echo fail)" "$detail"
  rm -f "$body_file" "$header_file"
}

stream_check() {
  local label="$1" url="$2" expected="$3"
  shift 3
  local header_file status ok=false detail=""

  header_file=$(mktemp)
  log "Request (stream): GET ${url} (expected ${expected})"
  curl -sS -m "$TIMEOUT" -o /dev/null -D "$header_file" "$@" "$url" 2>/dev/null || true

  status=$(awk 'NR==1 {print $2}' "$header_file")
  status=${status:-000}

  IFS=',' read -r -a expected_list <<<"$expected"
  for code in "${expected_list[@]}"; do
    if [[ "$status" == "$code" ]]; then
      ok=true
      break
    fi
  done

  if [[ "$ok" == true ]]; then
    echo -e "${GREEN}OK${NC}  ${label} (${status})"
  else
    echo -e "${RED}ERR${NC} ${label} (${status})"
    detail+=" -> expected ${expected}"
  fi

  record "$label" "$([[ "$ok" == true ]] && echo pass || echo fail)" "$detail"
  rm -f "$header_file"
}

section "Setup"
log "Server URL : ${SERVER_URL}"

RUN_ID=$(date +%s%N)
LABEL_PREFIX="admin-cli-test-${RUN_ID}"
SESSION_ID="admin-cli-${RUN_ID}"
PUBLIC_KEY="0xadmin${RUN_ID}"
MESSAGE="ping"

KEY_DEFAULT_VALUE="test-default-${RUN_ID}"
KEY_L2BEAT_VALUE="test-l2beat-${RUN_ID}"
KEY_BOTH_VALUE="test-both-${RUN_ID}"

KEY_DEFAULT=$(create_key "${LABEL_PREFIX}-default" "default" "$KEY_DEFAULT_VALUE")
KEY_L2BEAT=$(create_key "${LABEL_PREFIX}-l2beat" "l2beat" "$KEY_L2BEAT_VALUE")
KEY_BOTH=$(create_key "${LABEL_PREFIX}-both" "default,l2beat" "$KEY_BOTH_VALUE")
INVALID_KEY="invalid-${RUN_ID}"

section "Auth coverage"
log "Public endpoints: /health"
log "Session header endpoints: /api/chat, /api/state, /api/interrupt, /api/updates, /api/system, /api/events, /api/memory-mode, /api/sessions/*, /api/db/sessions/*"
log "API key enforced: /api/chat (non-default namespaces only)"

section "Public checks"
http_check "GET /health no key" "GET" "${SERVER_URL}/health" 200

section "Chat auth"
http_check "POST /api/chat default no session" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=default&public_key=${PUBLIC_KEY}" 400
http_check "POST /api/chat default no key" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=default&public_key=${PUBLIC_KEY}" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"
http_check "POST /api/chat default invalid key" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=default" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}" -H "${API_KEY_HEADER}: ${INVALID_KEY}"
http_check "POST /api/chat l2beat no key" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=l2beat" 401 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"
http_check "POST /api/chat l2beat default key" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=l2beat" 403 -H "${SESSION_ID_HEADER}: ${SESSION_ID}" -H "${API_KEY_HEADER}: ${KEY_DEFAULT}"
http_check "POST /api/chat l2beat l2beat key" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=l2beat" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}" -H "${API_KEY_HEADER}: ${KEY_L2BEAT}"
http_check "POST /api/chat l2beat multi key" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&namespace=l2beat" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}" -H "${API_KEY_HEADER}: ${KEY_BOTH}"
http_check "POST /api/chat l2beat chatbot param" "POST" "${SERVER_URL}/api/chat?message=${MESSAGE}&chatbot=l2beat" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}" -H "${API_KEY_HEADER}: ${KEY_L2BEAT}"

section "Session header checks"
stream_check "GET /api/updates no session" "${SERVER_URL}/api/updates" 400
stream_check "GET /api/updates with session" "${SERVER_URL}/api/updates" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"
http_check "GET /api/state no session" "GET" "${SERVER_URL}/api/state" 400
http_check "GET /api/state with session" "GET" "${SERVER_URL}/api/state" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

http_check "POST /api/interrupt no session" "POST" "${SERVER_URL}/api/interrupt" 400
http_check "POST /api/interrupt with session" "POST" "${SERVER_URL}/api/interrupt" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

http_check "POST /api/system no session" "POST" "${SERVER_URL}/api/system?message=system" 400
http_check "POST /api/system with session" "POST" "${SERVER_URL}/api/system?message=system" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

http_check "POST /api/memory-mode no session" "POST" "${SERVER_URL}/api/memory-mode?memory_mode=true" 400
http_check "POST /api/memory-mode with session" "POST" "${SERVER_URL}/api/memory-mode?memory_mode=true" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

http_check "GET /api/events no session" "GET" "${SERVER_URL}/api/events" 400
http_check "GET /api/events with session" "GET" "${SERVER_URL}/api/events" 200 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

http_check "GET /api/sessions list no key" "GET" "${SERVER_URL}/api/sessions?public_key=${PUBLIC_KEY}" 200
http_check "POST /api/sessions create no key" "POST" "${SERVER_URL}/api/sessions" 200 -H "Content-Type: application/json" -d '{"public_key":"'"${PUBLIC_KEY}"'","title":"Test"}'

http_check "GET /api/db/stats no key" "GET" "${SERVER_URL}/api/db/stats" 200

http_check "GET /api/db/session no session" "GET" "${SERVER_URL}/api/db/sessions/${SESSION_ID}" 400
http_check "GET /api/db/session with session" "GET" "${SERVER_URL}/api/db/sessions/${SESSION_ID}" 200,404 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

http_check "GET /api/db/messages no session" "GET" "${SERVER_URL}/api/db/sessions/${SESSION_ID}/messages" 400
http_check "GET /api/db/messages with session" "GET" "${SERVER_URL}/api/db/sessions/${SESSION_ID}/messages" 200,404 -H "${SESSION_ID_HEADER}: ${SESSION_ID}"

section "Summary"
log "${pass_count} passed, ${fail_count} failed"
for line in "${reports[@]}"; do
  echo -e "$line"
done

(( fail_count == 0 )) || exit 1
