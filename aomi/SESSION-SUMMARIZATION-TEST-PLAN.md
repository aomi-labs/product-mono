# Session Summarization Test Plan

## Overview
Comprehensive testing strategy for the session title auto-generation and SSE update system. Tests cover message-based throttling, state validation, and real-time event delivery.

---

## 1. Message-Based Throttling Tests

### 1.1 First Message (Should NOT Summarize)
**Setup:** Empty session
**Action:** Send 1 message
**Assertion:**
- `messages.len() = 1`
- `last_summarized_msg = 0`
- `1 > 0 + 5 = false` → No title generated
- **Expected:** No SSE TitleChanged event

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

curl -s -X POST "$BASE_URL/api/chat" \
  --get \
  --data-urlencode "session_id=$SESSION_ID" \
  --data-urlencode "public_key=0xtest" \
  --data-urlencode "message=First message"

# Wait 7 seconds (background job + buffer)
sleep 7

# Check: title should still be None or placeholder
curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq '.title'
```

### 1.2 Five Messages (Should NOT Summarize)
**Setup:** Session from 1.1
**Action:** Add 4 more messages (total 5)
**Assertion:**
- `messages.len() = 5`
- `last_summarized_msg = 0`
- `5 > 0 + 5 = false` → Still no summary
- **Expected:** No SSE TitleChanged event

**Command:**
```bash
for i in {2..5}; do
  curl -s -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i"
  sleep 0.5
done

sleep 7
curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq '.title'
```

### 1.3 Sixth Message (SHOULD Summarize)
**Setup:** Session from 1.2
**Action:** Add 1 more message (total 6)
**Assertion:**
- `messages.len() = 6`
- `last_summarized_msg = 0`
- `6 > 0 + 5 = true` → Summarize!
- After summarization: `last_summarized_msg = 6`
- **Expected:** SSE TitleChanged event with session_id

**Command:**
```bash
curl -s -X POST "$BASE_URL/api/chat" \
  --get \
  --data-urlencode "session_id=$SESSION_ID" \
  --data-urlencode "public_key=0xtest" \
  --data-urlencode "message=Message 6"

# Capture SSE in background
timeout 10 curl -s "$BASE_URL/api/updates" | grep -q "TitleChanged" && echo "✓ SSE Event received"

sleep 7
TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')
echo "Generated title: $TITLE"
```

### 1.4 Second Summarization Threshold (11+ Messages)
**Setup:** Session from 1.3 with title
**Action:** Add 5 more messages (total 11)
**Assertion:**
- `messages.len() = 11`
- `last_summarized_msg = 6`
- `11 > 6 + 5 = true` → Summarize again!
- After: `last_summarized_msg = 11`
- **Expected:** New SSE TitleChanged event (possibly different title)

**Command:**
```bash
for i in {7..11}; do
  curl -s -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i"
  sleep 0.5
done

sleep 7
NEW_TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')
echo "Updated title: $NEW_TITLE"
```

### 1.5 No Summarization Between Thresholds
**Setup:** Session from 1.4 with 11 messages and `last_summarized_msg = 11`
**Action:** Add 4 messages (total 15)
**Assertion:**
- `messages.len() = 15`
- `last_summarized_msg = 11`
- `15 > 11 + 5 = false` → No summarization
- **Expected:** No new SSE event, title unchanged

**Command:**
```bash
for i in {12..15}; do
  curl -s -X POST "$BASE_URL/api/chat" \
    --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i"
  sleep 0.5
done

sleep 7
STABLE_TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')
echo "Title should be unchanged: $STABLE_TITLE"
# Compare with NEW_TITLE from 1.4
```

---

## 2. State Validation Tests

### 2.1 Archived Sessions (Should NOT Summarize)
**Setup:** Session with 6+ messages (would normally summarize)
**Action:** Archive session, add more messages
**Assertion:**
- `is_archived = true`
- `need_summarize()` returns false
- **Expected:** No new title generation regardless of message count

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

# Add 6 messages to trigger summarization
for i in {1..6}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i" >/dev/null
done

sleep 7
INITIAL_TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')

# Archive the session
curl -s -X POST "$BASE_URL/api/sessions/$SESSION_ID/archive" >/dev/null

# Add 5+ more messages
for i in {7..12}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i" >/dev/null
done

sleep 7
FINAL_TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')

# Title should be identical
if [ "$INITIAL_TITLE" = "$FINAL_TITLE" ]; then
  echo "✓ Archived sessions prevent summarization"
else
  echo "✗ Title changed despite archive flag"
fi
```

### 2.2 Processing Sessions (Should NOT Summarize)
**Setup:** Session during active LLM processing
**Action:** While `is_processing = true`, check `need_summarize()`
**Assertion:**
- `need_summarize()` returns false when `is_processing = true`
- **Expected:** No title generation while chat is being processed

**Note:** This is harder to test directly since processing is quick. Strategy:
- Send rapid messages in succession
- Monitor SSE for TitleChanged events
- Verify no title changes occur during the "processing" window

### 2.3 Custom User Titles (Should NOT Overwrite)
**Setup:** Session with custom user-provided title
**Action:** Add 10+ messages
**Assertion:**
- Title is ≤6 chars (placeholder): `need_summarize()` allows override
- Title is >6 chars (user-provided): `need_summarize()` returns false
- **Expected:** User-provided titles are never overwritten

**Command:**
```bash
# Session with user-provided title
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest","title":"My Custom Title"}' | jq -r '.session_id')

INITIAL_TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')
echo "Initial title: $INITIAL_TITLE"

# Add 10+ messages
for i in {1..10}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i" >/dev/null
done

sleep 7
FINAL_TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')

if [ "$INITIAL_TITLE" = "$FINAL_TITLE" ]; then
  echo "✓ User-provided title preserved"
else
  echo "✗ Title was overwritten: $FINAL_TITLE"
fi
```

### 2.4 Sessions Without Public Key (Should NOT Summarize)
**Setup:** Session created without public_key
**Action:** Add 6+ messages
**Assertion:**
- `session_public_keys.contains_key(&session_id)` is false
- Background job skips this session
- **Expected:** No SSE event, no title generation

**Command:**
```bash
# Create session without public_key
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{}' | jq -r '.session_id')

# Add 6+ messages (still without public_key)
for i in {1..6}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "message=Message $i" >/dev/null
done

sleep 7
TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')

if [ -z "$TITLE" ] || [ "$TITLE" = "null" ]; then
  echo "✓ Session without public_key was not summarized"
else
  echo "✗ Session was summarized despite missing public_key"
fi
```

---

## 3. SSE Update Tests

### 3.1 TitleChanged Event Format
**Setup:** Session reaches summarization threshold
**Action:** Capture SSE event
**Assertion:**
- Event has `type: "TitleChanged"`
- Includes `session_id: "..."`
- Includes `new_title: "..."`
- Valid JSON parseable

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

# Capture SSE in background
(timeout 15 curl -s "$BASE_URL/api/updates") > /tmp/sse_events.txt &
SSE_PID=$!
sleep 1

# Trigger summarization
for i in {1..6}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i" >/dev/null
  sleep 0.5
done

sleep 10
kill $SSE_PID 2>/dev/null

# Parse SSE event
EVENT=$(grep "TitleChanged" /tmp/sse_events.txt | head -1 | sed 's/^data: //')
echo "SSE Event: $EVENT" | jq '.'

# Validate structure
echo "$EVENT" | jq -e '.type == "TitleChanged" and .data.session_id == "'$SESSION_ID'" and .data.new_title != null' && echo "✓ Event format valid"
```

### 3.2 Multiple Sessions Independent Updates
**Setup:** Two sessions, each reaching summarization threshold
**Action:** Subscribe to SSE once, send messages to both sessions
**Assertion:**
- Both TitleChanged events received in single SSE stream
- Each event has correct session_id
- Frontend can differentiate which session was updated

**Command:** (See `/scripts/test-sse.sh` - this is the comprehensive 2-session test)

### 3.3 SSE Stream Persistence
**Setup:** Long-running SSE connection
**Action:** Listen for 60+ seconds while sessions are summarized
**Assertion:**
- Connection stays open
- KeepAlive frames prevent timeout
- Events delivered in real-time
- **Expected:** No disconnections, clean event delivery

**Command:**
```bash
# Listen for 60 seconds
timeout 65 curl -s "$BASE_URL/api/updates" | tee /tmp/sse_full.log &
SSE_PID=$!

for j in {1..4}; do
  SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
    -H "Content-Type: application/json" \
    -d '{"public_key":"0xtest'$j'"}' | jq -r '.session_id')

  echo "Session $j: $SESSION_ID"

  for i in {1..6}; do
    curl -s -X POST "$BASE_URL/api/chat" --get \
      --data-urlencode "session_id=$SESSION_ID" \
      --data-urlencode "public_key=0xtest$j" \
      --data-urlencode "message=Msg $i" >/dev/null
  done

  sleep 15
done

wait $SSE_PID
EVENT_COUNT=$(grep -c "TitleChanged" /tmp/sse_full.log || echo 0)
echo "Received $EVENT_COUNT TitleChanged events over 60 seconds"
```

---

## 4. Backend Job Behavior Tests

### 4.1 Background Job Execution (5s Interval)
**Setup:** Session ready for summarization
**Action:** Monitor title generation timing
**Assertion:**
- Title appears ~5-7 seconds after 6th message (5s job + small variance)
- Consistent timing across multiple sessions
- **Expected:** Regular, predictable intervals

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

# Record time before sending 6th message
START=$(date +%s)

for i in {1..6}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i" >/dev/null
done

# Poll for title
TITLE=""
while [ -z "$TITLE" ] || [ "$TITLE" = "null" ]; do
  TITLE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID" | jq -r '.title')
  NOW=$(date +%s)
  ELAPSED=$((NOW - START))

  if [ $ELAPSED -gt 15 ]; then
    echo "✗ Timeout waiting for title after 15 seconds"
    exit 1
  fi

  if [ -z "$TITLE" ] || [ "$TITLE" = "null" ]; then
    echo "  Waiting... ($ELAPSED seconds)"
    sleep 1
  fi
done

END=$(date +%s)
DURATION=$((END - START))
echo "✓ Title generated in $DURATION seconds"
```

### 4.2 mark_summarized() Updates State
**Setup:** Session with title generated
**Action:** Check `last_summarized_msg` field
**Assertion:**
- `last_summarized_msg = messages.len()`
- When new messages added, it doesn't immediately re-summarize
- **Expected:** State correctly tracks summarization history

**Note:** `last_summarized_msg` is internal state, not exposed via API. Validate indirectly by checking that second summarization doesn't happen until message threshold is crossed.

---

## 5. Integration/E2E Tests

### 5.1 Full Workflow (Create → Chat → SSE → Update)
**Setup:** Clean environment
**Action:** Complete workflow from creation to title update
**Assertion:**
- Session created successfully
- Messages accepted
- SSE event received
- Final state reflects updated title

**Command:** (See `/scripts/test-sse.sh` - already comprehensive)

### 5.2 Concurrent Sessions (Stress Test)
**Setup:** Create 5+ sessions
**Action:** Send messages to all concurrently
**Assertion:**
- All sessions process independently
- All titles generated correctly
- SSE delivers all events without loss
- **Expected:** No race conditions, all events delivered

**Command:**
```bash
NUM_SESSIONS=5

# Create sessions
SESSION_IDS=()
for j in $(seq 1 $NUM_SESSIONS); do
  SID=$(curl -s -X POST "$BASE_URL/api/sessions" \
    -H "Content-Type: application/json" \
    -d '{"public_key":"0xtest"}' | jq -r '.session_id')
  SESSION_IDS+=($SID)
  echo "Created session $j: $SID"
done

# Listen to SSE
(timeout 45 curl -s "$BASE_URL/api/updates") > /tmp/sse_concurrent.txt &
SSE_PID=$!
sleep 1

# Send messages to all sessions in parallel
for j in $(seq 0 $((NUM_SESSIONS-1))); do
  SID=${SESSION_IDS[$j]}
  (
    for i in {1..6}; do
      curl -s -X POST "$BASE_URL/api/chat" --get \
        --data-urlencode "session_id=$SID" \
        --data-urlencode "public_key=0xtest" \
        --data-urlencode "message=Session $j Message $i" >/dev/null
      sleep 0.3
    done
  ) &
done

# Wait for all background jobs
wait

sleep 15
kill $SSE_PID 2>/dev/null

# Verify all sessions have titles
for j in $(seq 0 $((NUM_SESSIONS-1))); do
  SID=${SESSION_IDS[$j]}
  TITLE=$(curl -s "$BASE_URL/api/sessions/$SID" | jq -r '.title')
  if [ -n "$TITLE" ] && [ "$TITLE" != "null" ]; then
    echo "✓ Session $j has title: $TITLE"
  else
    echo "✗ Session $j missing title"
  fi
done

# Count SSE events
EVENT_COUNT=$(grep -c "TitleChanged" /tmp/sse_concurrent.txt || echo 0)
echo "Received $EVENT_COUNT TitleChanged events (expected ~$NUM_SESSIONS)"
```

### 5.3 Title Update Visibility (Frontend Perspective)
**Setup:** Frontend subscribing to SSE
**Action:** Simulate frontend receiving SSE updates
**Assertion:**
- Frontend receives TitleChanged event
- Can extract session_id and new_title
- Can update UI for correct session
- **Expected:** No confusion between sessions

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

echo "Session ID: $SESSION_ID"

# Simulate frontend SSE listener (jq to pretty-print events)
(timeout 20 curl -s "$BASE_URL/api/updates" | while read line; do
  if [[ $line == data:* ]]; then
    EVENT=${line#"data: "}
    TYPE=$(echo "$EVENT" | jq -r '.type' 2>/dev/null)
    if [ "$TYPE" = "TitleChanged" ]; then
      RECEIVED_SID=$(echo "$EVENT" | jq -r '.data.session_id')
      NEW_TITLE=$(echo "$EVENT" | jq -r '.data.new_title')
      if [ "$RECEIVED_SID" = "$SESSION_ID" ]; then
        echo "✓ Frontend received update for correct session: $NEW_TITLE"
      fi
    fi
  fi
done) &
LISTENER_PID=$!
sleep 1

# Trigger summarization
for i in {1..6}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Message $i" >/dev/null
  sleep 0.3
done

wait $LISTENER_PID
```

---

## 6. Edge Cases

### 6.1 Empty Session (No Messages)
**Setup:** Create session, don't send messages
**Action:** Wait 20+ seconds
**Assertion:**
- `need_summarize()` returns false (empty messages)
- No title generated
- No error logs

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

sleep 20

STATE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID")
MESSAGES=$(echo "$STATE" | jq '.messages | length')
TITLE=$(echo "$STATE" | jq -r '.title')

echo "Messages: $MESSAGES, Title: $TITLE"
if [ "$MESSAGES" = "0" ] && ([ -z "$TITLE" ] || [ "$TITLE" = "null" ]); then
  echo "✓ Empty session handled correctly"
fi
```

### 6.2 Only System Messages
**Setup:** Session with only system messages (no user messages)
**Action:** Wait for summarization
**Assertion:**
- No title generated
- `need_summarize()` returns false (no user content)

### 6.3 Rapid Message Spam
**Setup:** Send many messages in quick succession
**Action:** Send 50+ messages with no delay
**Assertion:**
- All messages processed
- Correct summarization at thresholds (6, 12, 18, 24...)
- No race conditions or lost events

**Command:**
```bash
SESSION_ID=$(curl -s -X POST "$BASE_URL/api/sessions" \
  -H "Content-Type: application/json" \
  -d '{"public_key":"0xtest"}' | jq -r '.session_id')

# Rapid fire 50 messages
for i in {1..50}; do
  curl -s -X POST "$BASE_URL/api/chat" --get \
    --data-urlencode "session_id=$SESSION_ID" \
    --data-urlencode "public_key=0xtest" \
    --data-urlencode "message=Rapid message $i" >/dev/null &

  # Release 10 at a time
  if [ $((i % 10)) -eq 0 ]; then
    wait
  fi
done

wait

sleep 30
STATE=$(curl -s "$BASE_URL/api/sessions/$SESSION_ID")
TITLE=$(echo "$STATE" | jq -r '.title')
MESSAGES=$(echo "$STATE" | jq '.messages | length')

echo "Final: $MESSAGES messages, title: $TITLE"
if [ "$MESSAGES" -eq 50 ]; then
  echo "✓ All messages persisted"
fi
```

### 6.4 Very Long Title
**Setup:** Send messages that generate extremely long title
**Action:** Verify title is stored and transmitted
**Assertion:**
- Long title persisted correctly
- SSE event includes full title
- No truncation

---

## 7. Automated Test Suite Structure

```bash
#!/usr/bin/env bash
# tests/session-summarization-tests.sh

set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
TESTS_PASSED=0
TESTS_FAILED=0

test_case() {
  local name="$1"
  local result="$2"

  if [ "$result" -eq 0 ]; then
    echo "✓ $name"
    ((TESTS_PASSED++))
  else
    echo "✗ $name"
    ((TESTS_FAILED++))
  fi
}

# Health check
curl -fs "$BASE_URL/health" >/dev/null || {
  echo "Backend not running at $BASE_URL"
  exit 1
}

# Run all test functions below...
test_no_summarize_first_message
test_no_summarize_five_messages
test_summarize_sixth_message
# ... etc

echo ""
echo "================"
echo "Tests Passed: $TESTS_PASSED"
echo "Tests Failed: $TESTS_FAILED"
echo "================"

[ $TESTS_FAILED -eq 0 ] && exit 0 || exit 1
```

---

## 8. Test Execution Order (Recommended)

1. **Message-Based Throttling** (1.1 → 1.5): Verify core logic
2. **State Validation** (2.1 → 2.4): Verify skip conditions
3. **Backend Job Behavior** (4.1 → 4.2): Verify timing
4. **SSE Updates** (3.1 → 3.3): Verify event delivery
5. **Integration/E2E** (5.1 → 5.3): Full workflows
6. **Edge Cases** (6.1 → 6.4): Boundary conditions
7. **Stress Tests** (5.2): Concurrent load

---

## 9. Success Criteria

- ✅ All message-based throttling thresholds correct
- ✅ All state checks (archived, processing, custom title, public_key) respected
- ✅ SSE events delivered with correct format and content
- ✅ Multiple sessions update independently without interference
- ✅ Background job runs at consistent 5-second interval
- ✅ No race conditions or lost events under concurrent load
- ✅ Edge cases handled gracefully
- ✅ All tests repeatable and deterministic

