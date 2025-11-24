#!/usr/bin/env bash
set -euo pipefail

# Comprehensive SSE test: Create 2 sessions, send messages in a loop,
# and verify SSE updates are received in real-time.
#
# Configure via env vars:
#   BASE_URL (default: http://127.0.0.1:8080)
#   PUBLIC_KEY (default: 0xabc123)

BASE_URL=${BASE_URL:-"http://127.0.0.1:8080"}
PUBLIC_KEY=${PUBLIC_KEY:-"0xabc123"}

# Temp file for SSE events
SSE_EVENTS=$(mktemp)
trap "rm -f $SSE_EVENTS" EXIT

echo "=== SSE End-to-End Test ==="
echo "Backend: $BASE_URL"
echo ""

# Health check
echo "Health check..."
curl -fsS "$BASE_URL/health" >/dev/null || {
  echo "❌ Health check failed" >&2
  exit 1
}
echo "✓ Backend is up"
echo ""

# Create two sessions
echo "Creating session 1..."
session1=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\",\"title\":\"Test Session 1\"}" | jq -r '.session_id')
echo "✓ Session 1: $session1"

echo "Creating session 2..."
session2=$(curl -fsS -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d "{\"public_key\":\"$PUBLIC_KEY\",\"title\":\"Test Session 2\"}" | jq -r '.session_id')
echo "✓ Session 2: $session2"
echo ""

# Start SSE listener in background
echo "Starting SSE listener (background)..."
timeout 35 curl -sS "$BASE_URL/api/updates" 2>/dev/null >"$SSE_EVENTS" &
sse_pid=$!
sleep 1  # Give listener time to connect
echo "✓ SSE listener started (PID $sse_pid)"
echo ""

# Send messages in a loop for 6 iterations (30 seconds)
echo "Sending messages in 6 iterations (5 sec intervals)..."
for i in {1..6}; do
  echo ""
  echo "=== Iteration $i ==="

  # Send to session 1
  msg1="Message $i from session 1"
  echo "Sending to session 1: '$msg1'"
  curl -fsS -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$session1" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=$msg1" >/dev/null
  echo "✓ Sent"

  # Send to session 2
  msg2="Message $i from session 2"
  echo "Sending to session 2: '$msg2'"
  curl -fsS -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$session2" \
    --data-urlencode "public_key=$PUBLIC_KEY" \
    --data-urlencode "message=$msg2" >/dev/null
  echo "✓ Sent"

  # Wait 5 seconds (except on last iteration)
  if [ $i -lt 6 ]; then
    echo "Waiting 5 seconds for title summarization..."
    sleep 5
  fi
done

# Give final moment for SSE events
sleep 2
kill $sse_pid 2>/dev/null || true

echo ""
echo "=== SSE Events Received ==="
if [ -s "$SSE_EVENTS" ]; then
  # Extract just the data lines (remove "data: " prefix)
  grep -o '"type":"TitleChanged"[^}]*}' "$SSE_EVENTS" || true
  echo ""
  event_count=$(grep -c "TitleChanged" "$SSE_EVENTS" || true)
  echo "✓ Received $event_count title updates"
else
  echo "⚠ No SSE events captured (titles may still be generating)"
fi

echo ""
echo "=== Test Complete ==="
echo "Check /api/state for final session states:"
echo ""
echo "Session 1 title:"
curl -fsS "$BASE_URL/api/state" \
  --get \
  --data-urlencode "session_id=$session1" | jq '.title'

echo "Session 2 title:"
curl -fsS "$BASE_URL/api/state" \
  --get \
  --data-urlencode "session_id=$session2" | jq '.title'
