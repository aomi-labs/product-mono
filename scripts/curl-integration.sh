#!/usr/bin/env bash
set -euo pipefail

# Simple curl-based integration check against the running backend.
# Configure via env vars:
#   BASE_URL (default: http://127.0.0.1:8080)
#   PUBLIC_KEY (default: 0xabc123)
#   MESSAGE (default: "Hello from curl script")
#   NEW_TITLE (default: "Curl Session Title")
#   WEATHER_MESSAGE (default: "What's the weather in San Francisco?")
#   WEATHER_SESSION_ID (optional: reuse an existing session to mirror frontend logs)

BASE_URL=${BASE_URL:-"http://127.0.0.1:8080"}
PUBLIC_KEY=${PUBLIC_KEY:-"0xabc123"}
MESSAGE=${MESSAGE:-"Hello from curl script"}
NEW_TITLE=${NEW_TITLE:-"Curl Session Title"}
WEATHER_MESSAGE=${WEATHER_MESSAGE:-"What's the weather in San Francisco?"}
WEATHER_SESSION_ID=${WEATHER_SESSION_ID:-""}
API_KEY_HEADER="X-API-Key"
API_KEY_VALUE="${BACKEND_API_KEY:-}"
if [[ -z "$API_KEY_VALUE" && -n "${BACKEND_API_KEYS:-}" ]]; then
  API_KEY_VALUE="${BACKEND_API_KEYS%%,*}"
  API_KEY_VALUE="${API_KEY_VALUE%%:*}"
fi
API_KEY_ARGS=()
if [[ -n "${API_KEY_VALUE:-}" ]]; then
  API_KEY_ARGS=(-H "${API_KEY_HEADER}: ${API_KEY_VALUE}")
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required for parsing responses. Install jq and rerun." >&2
  exit 1
fi

echo "Health check at $BASE_URL/health"
curl -fsS "$BASE_URL/health" || {
  echo "Health check failed; ensure backend is running at $BASE_URL" >&2
  exit 1
}

echo "Creating session..."
create_resp="$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  "${API_KEY_ARGS[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\"}")"
session_id="$(echo "$create_resp" | jq -r '.session_id')"
main_topic="$(echo "$create_resp" | jq -r '.main_topic')"
echo "Session created: id=$session_id topic=$main_topic"

echo "Sending chat message..."
curl -fsS -X POST "$BASE_URL/api/chat" \
  "${API_KEY_ARGS[@]}" \
  --get \
  --data-urlencode "session_id=$session_id" \
  --data-urlencode "public_key=$PUBLIC_KEY" \
  --data-urlencode "message=$MESSAGE" | jq '.'

echo "Polling state..."
curl -fsS "$BASE_URL/api/state" \
  "${API_KEY_ARGS[@]}" \
  --get \
  --data-urlencode "session_id=$session_id" | jq '{messages, is_processing, pending_wallet_tx}'

echo "Archiving session..."
curl -fsS -o /dev/null -w "Archive status: %{http_code}\n" \
  "${API_KEY_ARGS[@]}" \
  -X POST "$BASE_URL/api/sessions/$session_id/archive"

echo "Unarchiving session..."
curl -fsS -o /dev/null -w "Unarchive status: %{http_code}\n" \
  "${API_KEY_ARGS[@]}" \
  -X POST "$BASE_URL/api/sessions/$session_id/unarchive"

echo "Renaming session title..."
curl -fsS -o /dev/null -w "Rename status: %{http_code}\n" \
  "${API_KEY_ARGS[@]}" \
  -X PATCH "$BASE_URL/api/sessions/$session_id" \
  -H "Content-Type: application/json" \
  -d "{\"title\":\"$NEW_TITLE\"}"

echo "Getting session by ID..."
curl -fsS "$BASE_URL/api/sessions/$session_id" "${API_KEY_ARGS[@]}" | jq '{title, messages: .messages | length, is_processing, pending_wallet_tx}'

echo "Listing sessions for public key..."
curl -fsS "$BASE_URL/api/sessions" \
  "${API_KEY_ARGS[@]}" \
  --get \
  --data-urlencode "public_key=$PUBLIC_KEY" \
  --data-urlencode "limit=5" | jq '.'

echo "Deleting session..."
curl -fsS -o /dev/null -w "Delete status: %{http_code}\n" \
  "${API_KEY_ARGS[@]}" \
  -X DELETE "$BASE_URL/api/sessions/$session_id"

# Optional: mirror frontend postChatMessage payload for weather check
if [[ -n "$WEATHER_SESSION_ID" ]]; then
  echo "Posting weather message to existing session $WEATHER_SESSION_ID ..."
  curl -fsS -X POST "$BASE_URL/api/chat" \
    "${API_KEY_ARGS[@]}" \
    --get \
    --data-urlencode "session_id=$WEATHER_SESSION_ID" \
    --data-urlencode "message=$WEATHER_MESSAGE" | jq '.'

  echo "Polling state for weather session..."
  curl -fsS "$BASE_URL/api/state" \
    "${API_KEY_ARGS[@]}" \
    --get \
    --data-urlencode "session_id=$WEATHER_SESSION_ID" | jq '{messages, is_processing, pending_wallet_tx}'
fi


echo "Testing SSE updates endpoint..."
# Just check if endpoint responds (don't stream, SSE streams are infinite)
# Use max-time to limit connection attempt to 3 seconds
http_code=$(curl -sS -w "%{http_code}" -o /dev/null --max-time 3 "$BASE_URL/api/updates" "${API_KEY_ARGS[@]}")
if [ "$http_code" = "200" ]; then
  echo "SSE test status: $http_code"
else
  echo "SSE test failed: HTTP $http_code" >&2
  exit 1
fi

echo "Done."
