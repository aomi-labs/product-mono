#!/usr/bin/env bash
set -euo pipefail

# SSE test with 3 rounds of different content
# Tests title summarization with 1-message threshold and SSE updates
#
# Configure via env vars:
#   BASE_URL (default: http://127.0.0.1:8080)
#   PUBLIC_KEY (default: 0xabc123)

BASE_URL=${BASE_URL:-"http://127.0.0.1:8080"}
PUBLIC_KEY=${PUBLIC_KEY:-"0xabc123"}

# Temp file for SSE events
SSE_EVENTS=$(mktemp)
trap "rm -f $SSE_EVENTS" EXIT

echo "=== SSE Test: 3 Rounds of Different Content ==="
echo "Backend: $BASE_URL"
echo "Threshold: 1 message (title updates immediately)"
echo ""

# Health check
echo "Health check..."
echo "GET /health"
health=$(curl -fsS "$BASE_URL/health")
echo "$health"
echo ""

# Create two sessions
echo "Creating session 1..."
echo "POST /api/sessions (Session 1)"
s1_resp=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\"}")
echo "$s1_resp" | jq .
session1=$(echo "$s1_resp" | jq -r '.session_id')
echo "✓ Session 1: $session1"
echo ""

echo "Creating session 2..."
echo "POST /api/sessions (Session 2)"
s2_resp=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\"}")
echo "$s2_resp" | jq .
session2=$(echo "$s2_resp" | jq -r '.session_id')
echo "✓ Session 2: $session2"
echo ""

# Start SSE listener in background
echo "Starting SSE listener (background, 30 sec timeout)..."
timeout 30 curl -sS "$BASE_URL/api/updates" 2>/dev/null >"$SSE_EVENTS" &
sse_pid=$!
sleep 1  # Give listener time to connect
echo "✓ SSE listener started (PID $sse_pid)"
echo ""

# Define 3 rounds with different content
declare -a ROUND_NAMES=("Python Basics" "Web Development" "Data Science Tools")
declare -a SESSION1_MSGS=(
  "Can you explain Python decorators and how they work with functions?"
  "What's the difference between REST and GraphQL APIs for web applications?"
  "How do I use pandas for data cleaning and preprocessing?"
)
declare -a SESSION2_MSGS=(
  "Tell me about distributed systems and eventual consistency"
  "What are microservices and when should I use them?"
  "Explain blockchain and how consensus mechanisms work"
)

# Send messages in 3 rounds with different content
echo "=== Sending 3 Rounds of Different Content ==="
for i in {1..3}; do
  echo ""
  echo "=== Round $i: ${ROUND_NAMES[$((i-1))]} ==="

  # Send to session 1
  msg1="${SESSION1_MSGS[$((i-1))]}"
  echo "POST /api/chat (Session 1)"
  echo "Message: $msg1"
  s1_msg=$(curl -fsS -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$session1" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=$msg1")
  echo "$s1_msg" | jq .
  echo ""

  # Send to session 2
  msg2="${SESSION2_MSGS[$((i-1))]}"
  echo "POST /api/chat (Session 2)"
  echo "Message: $msg2"
  s2_msg=$(curl -fsS -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$session2" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=$msg2")
  echo "$s2_msg" | jq .
  echo ""

  # Wait 5 seconds for background summarization job
  if [ $i -le 3 ]; then
    echo "Waiting 5 seconds for title summarization (threshold: 1 message)..."
    sleep 5

    # Check current DB state during wait
    echo ""
    echo "DB state check (mid-wait):"
    echo "GET /api/db/sessions/$session1"
    s1_db=$(curl -fsS "$BASE_URL/api/db/sessions/$session1")
    echo "$s1_db" | jq '{title, message_count}'
    echo ""
  fi
done

# Give final moment for SSE events
echo ""
echo "Waiting 3 seconds for final SSE events..."
sleep 3
kill $sse_pid 2>/dev/null || true
echo "✓ SSE listener stopped"
echo ""

# Show SSE events
echo "=== SSE Events Received ==="
if [ -s "$SSE_EVENTS" ]; then
  echo "Raw SSE stream:"
  cat "$SSE_EVENTS"
  echo ""
  echo "Extracted TitleChanged events:"
  grep -o '"type":"TitleChanged"[^}]*}' "$SSE_EVENTS" || echo "⚠ No TitleChanged events found"
  echo ""
  event_count=$(grep -c "TitleChanged" "$SSE_EVENTS" || true)
  echo "✓ Total TitleChanged events: $event_count"
else
  echo "⚠ No SSE events captured"
fi

echo ""
echo "=== Final Session States ==="
echo "Session 1 (via GET /api/sessions/:session_id):"
echo "GET /api/sessions/$session1"
s1_final=$(curl -fsS "$BASE_URL/api/sessions/$session1")
echo "$s1_final" | jq .
echo ""

echo "Session 2 (via GET /api/sessions/:session_id):"
echo "GET /api/sessions/$session2"
s2_final=$(curl -fsS "$BASE_URL/api/sessions/$session2")
echo "$s2_final" | jq .
echo ""

echo "=== Test Complete ==="
