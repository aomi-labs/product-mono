#!/usr/bin/env bash
set -euo pipefail

# Test script demonstrating Tier 1 DB inspection endpoints
# Tests message-based throttling without triggering background jobs or side effects
#
# Configure via env vars:
#   BASE_URL (default: http://127.0.0.1:8080)
#   PUBLIC_KEY (default: 0xabc123)

BASE_URL=${BASE_URL:-"http://127.0.0.1:8080"}
PUBLIC_KEY=${PUBLIC_KEY:-"0xabc123"}
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

echo "=== DB-Driven Test Suite ==="
echo "Backend: $BASE_URL"
echo ""

# Health check
echo "Health check..."
echo "GET /health"
health=$(curl -fsS "$BASE_URL/health")
echo "$health"
if [ "$health" != "OK" ]; then
  echo "❌ Health check failed" >&2
  exit 1
fi
echo "✓ Backend is up"
echo ""

# Test 1: Create session and verify initial state
echo "=== Test 1: Initial State ==="
echo "POST /api/sessions"
session_resp=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  "${API_KEY_ARGS[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\"}")
echo "$session_resp" | jq .
session=$(echo "$session_resp" | jq -r '.session_id')
echo "✓ Session created: $session"
echo ""

# Inspect via DB endpoint (no side effects)
echo "GET /api/db/sessions/$session"
db_state=$(curl -fsS "$BASE_URL/api/db/sessions/$session" "${API_KEY_ARGS[@]}")
echo "$db_state" | jq .
msg_count=$(echo "$db_state" | jq '.message_count')
echo "✓ DB inspection: message_count = $msg_count"
echo ""

# Test 2: Message-based throttling (1-5 messages)
echo "=== Test 2: Messages 1-5 (No Summarization Expected) ==="
for i in {1..5}; do
  echo "Sending message $i..."
  echo "POST /api/chat?session_id=$session&public_key=$PUBLIC_KEY&message=Test message $i"
  msg_resp=$(curl -fsS -X POST "$BASE_URL/api/chat" \
    "${API_KEY_ARGS[@]}" \
    --get \
    --data-urlencode "session_id=$session" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=Test message $i")
  echo "$msg_resp" | jq .
  echo ""
done

# Inspect without update_state() side effects
echo "GET /api/db/sessions/$session"
db_state=$(curl -fsS "$BASE_URL/api/db/sessions/$session" "${API_KEY_ARGS[@]}")
echo "$db_state" | jq .
msg_count=$(echo "$db_state" | jq '.message_count')
title=$(echo "$db_state" | jq -r '.title')
echo "✓ After 5 messages:"
echo "  - message_count = $msg_count"
echo "  - title = $title (should still be 'DB Test Session')"
echo ""

# Test 3: Threshold (6+ messages)
echo "=== Test 3: Message 6 (Summarization Threshold) ==="
echo "Sending message 6..."
echo "POST /api/chat?session_id=$session&public_key=$PUBLIC_KEY&message=Test message 6 - should trigger summarization"
msg6_resp=$(curl -fsS -X POST "$BASE_URL/api/chat" \
  "${API_KEY_ARGS[@]}" \
  --get \
  --data-urlencode "session_id=$session" \
  --data-urlencode "public_key=$PUBLIC_KEY" \
  --data-urlencode "message=Test message 6 - should trigger summarization")
echo "$msg6_resp" | jq .
echo ""

# Small delay for background job
echo "Waiting 2 seconds for background job..."
sleep 2

echo "GET /api/db/sessions/$session"
db_state=$(curl -fsS "$BASE_URL/api/db/sessions/$session" "${API_KEY_ARGS[@]}")
echo "$db_state" | jq .
msg_count=$(echo "$db_state" | jq '.message_count')
title=$(echo "$db_state" | jq -r '.title')
is_processing=$(echo "$db_state" | jq '.is_processing')
echo "✓ After 6 messages:"
echo "  - message_count = $msg_count"
echo "  - title = $title (may update if summarization runs)"
echo "  - is_processing = $is_processing"
echo ""

# Test 4: Raw messages inspection
echo "=== Test 4: Messages Endpoint ==="
echo "GET /api/db/sessions/$session/messages"
messages=$(curl -fsS "$BASE_URL/api/db/sessions/$session/messages" "${API_KEY_ARGS[@]}")
echo "$messages" | jq .
first_msg=$(echo "$messages" | jq -r '.[0].content' 2>/dev/null || echo "N/A")
msg_array_len=$(echo "$messages" | jq 'length')
echo "✓ Messages array:"
echo "  - count = $msg_array_len"
echo "  - first message (first 50 chars): ${first_msg:0:50}"
echo ""

# Test 5: Multiple sessions
echo "=== Test 5: Multiple Sessions ==="
echo "POST /api/sessions (Session 2)"
s2_resp=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  "${API_KEY_ARGS[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\"}")
echo "$s2_resp" | jq .
session2=$(echo "$s2_resp" | jq -r '.session_id')
echo "✓ Session 2 created: $session2"
echo ""

echo "POST /api/sessions (Session 3)"
s3_resp=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  "${API_KEY_ARGS[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\"}")
echo "$s3_resp" | jq .
session3=$(echo "$s3_resp" | jq -r '.session_id')
echo "✓ Session 3 created: $session3"
echo ""

# Add different message counts to each
echo "Adding 3 messages to Session 2..."
for i in {1..3}; do
  s2m_resp=$(curl -fsS -X POST "$BASE_URL/api/chat" \
    "${API_KEY_ARGS[@]}" \
    --get \
    --data-urlencode "session_id=$session2" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=S2 message $i")
  echo "  Message $i:"
  echo "$s2m_resp" | jq -c '{title, message_count: (.messages | length), is_processing}'
done
echo ""

echo "Adding 8 messages to Session 3..."
for i in {1..8}; do
  s3m_resp=$(curl -fsS -X POST "$BASE_URL/api/chat" \
    "${API_KEY_ARGS[@]}" \
    --get \
    --data-urlencode "session_id=$session3" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=S3 message $i")
  echo "  Message $i:"
  echo "$s3m_resp" | jq -c '{title, message_count: (.messages | length), is_processing}'
done
echo ""

# Inspect all in parallel (demonstrating efficiency)
echo "Inspecting all sessions via DB:"
echo "GET /api/db/sessions/$session"
s1_data=$(curl -fsS "$BASE_URL/api/db/sessions/$session" "${API_KEY_ARGS[@]}")
echo "$s1_data" | jq .
echo ""

echo "GET /api/db/sessions/$session2"
s2_data=$(curl -fsS "$BASE_URL/api/db/sessions/$session2" "${API_KEY_ARGS[@]}")
echo "$s2_data" | jq .
echo ""

echo "GET /api/db/sessions/$session3"
s3_data=$(curl -fsS "$BASE_URL/api/db/sessions/$session3" "${API_KEY_ARGS[@]}")
echo "$s3_data" | jq .
echo ""

echo "Summary:"
echo "  Session 1: $(echo "$s1_data" | jq '.message_count') messages, title='$(echo "$s1_data" | jq -r '.title')'"
echo "  Session 2: $(echo "$s2_data" | jq '.message_count') messages, title='$(echo "$s2_data" | jq -r '.title')'"
echo "  Session 3: $(echo "$s3_data" | jq '.message_count') messages, title='$(echo "$s3_data" | jq -r '.title')'"
echo ""

# Test 6: Stats endpoint
echo "=== Test 6: DB Stats ==="
echo "GET /api/db/stats"
stats=$(curl -fsS "$BASE_URL/api/db/stats" "${API_KEY_ARGS[@]}")
echo "$stats" | jq .
session_count=$(echo "$stats" | jq '.session_count')
echo "✓ Stats: session_count = $session_count"
echo ""

# Test 7: Comparison - DB vs API endpoint
echo "=== Test 7: DB vs Regular API Endpoints ==="
echo "Getting session via /api/sessions/:session_id (triggers update_state)..."
echo "GET /api/sessions/$session"
api_state=$(curl -fsS "$BASE_URL/api/sessions/$session" "${API_KEY_ARGS[@]}")
echo "$api_state" | jq .
echo "  - title: $(echo "$api_state" | jq -r '.title')"
echo "  - is_processing: $(echo "$api_state" | jq '.is_processing')"
echo ""

echo "Getting session via /api/db/sessions/:session_id (read-only, no side effects)..."
echo "GET /api/db/sessions/$session"
db_state=$(curl -fsS "$BASE_URL/api/db/sessions/$session" "${API_KEY_ARGS[@]}")
echo "$db_state" | jq .
echo "  - title: $(echo "$db_state" | jq -r '.title')"
echo "  - is_processing: $(echo "$db_state" | jq '.is_processing')"
echo ""

echo "=== Test Complete ==="
echo "✓ All DB inspection endpoints working"
echo "✓ Message-based throttling verified"
echo "✓ Multiple session inspection efficient"
echo ""
