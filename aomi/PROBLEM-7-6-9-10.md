# Remaining Issues (7, 8, 9, 10)

## Issue #2: ≤6 Character Title Detection is Fragile

**Location:** `crates/backend/src/manager.rs:391-396`

**Problem:**
```rust
if let Some(ref title) = session_data.title {
    if title.len() > 6 {
        return None; // Has user-provided or already-summarized title
    }
}
```

The background job uses `title.len() <= 6` to detect "fallback" placeholder titles (truncated session IDs). This is fragile because:
- User could legitimately set a short title like "Help" (4 chars), "ETH" (3 chars), or "Chat" (4 chars)
- These would be overwritten by auto-generation

**Suggested Fix:**
Use a marker field or prefix instead of length heuristic:
```rust
// Option A: Add a field to track if title is auto-generated
is_auto_generated_title: bool,

// Option B: Use a prefix for fallback titles
let fallback = format!("~{}", &session_id[..6]); // "~abc123"
```

---

## Issue #7: No Deduplication of Title Updates

**Location:** `crates/backend/src/manager.rs:453-464`

**Problem:**
```rust
session_data.title = Some(result.title.clone());
session_data.last_summarized_msg = msg_count;
// ... broadcasts TitleChanged
```

The background job updates and broadcasts title changes even if the new auto-generated title is identical to the current one. This causes:
- Unnecessary DB writes
- Unnecessary SSE broadcasts to clients
- Wasted resources

**Suggested Fix:**
```rust
if session_data.title.as_ref() != Some(&result.title) {
    session_data.title = Some(result.title.clone());
    // ... broadcast only if changed
}
session_data.last_summarized_msg = msg_count; // Always update this
```

---

## Issue #8: SSE Updates Don't Include Title Changes (By Design)

**Location:**
- `bin/backend/src/endpoint/mod.rs` - `chat_stream` endpoint
- `bin/backend/src/endpoint/system.rs` - `updates_endpoint`

**Problem:**
Title changes from the background job are broadcast via a separate `/api/updates` SSE endpoint using `SystemUpdate::TitleChanged`. The main `chat_stream` SSE includes title in each response, but:
- Clients might only listen to `chat_stream` and miss title updates
- Title updates happen between chat stream intervals (100ms)
- No guarantee client sees the update

**This is a design decision, not necessarily a bug.** Options:
1. Document that clients must listen to both endpoints
2. Include a `title_version` or `title_updated_at` in chat_stream response
3. Accept eventual consistency (title will be correct on next chat_stream tick)

---

## Issue #9: Potential Memory Leak in System Update Channel

**Location:** `crates/backend/src/manager.rs:79`

**Problem:**
```rust
let (system_update_tx, _) = broadcast::channel(64);
```

The broadcast receiver is immediately dropped. While this is valid Tokio code (broadcast channels work with only senders), there are edge cases:
- If buffer fills (64 messages) with no subscribers, oldest messages are dropped (expected)
- If `send()` is called with no subscribers, it returns `Err` (currently ignored with `let _ = ...`)

**Current behavior is acceptable** but could log when sends fail:
```rust
if manager.system_update_tx.send(update).is_err() {
    tracing::debug!("No subscribers for system update");
}
```

---

## Issue #10: Title from Frontend vs Auto-Generated Conflict

**Location:** `bin/backend/src/endpoint/sessions.rs:44-48`

**Problem:**
```rust
let title = payload.get("title").cloned().or_else(|| {
    let mut placeholder = session_id.clone();
    placeholder.truncate(6);
    Some(placeholder)
});
```

If frontend sends `{"title": ""}` (empty string), it won't fall back to placeholder because `"".cloned()` returns `Some("")`. An empty string title would:
- Pass the `need_summarize()` check (length 0 ≤ 6)
- Cause display issues on frontend (empty title)
- Be treated as needing auto-generation (probably fine, but inconsistent)

**Suggested Fix:**
```rust
let title = payload.get("title")
    .filter(|t| !t.is_empty()) // Filter out empty strings
    .cloned()
    .or_else(|| {
        let mut placeholder = session_id.clone();
        placeholder.truncate(6);
        Some(placeholder)
    });
```

---

## Summary

| Issue | Severity | Effort | Recommendation |
|-------|----------|--------|----------------|
| #2 ≤6 char detection | Medium | Medium | Fix with marker field |
| #7 No deduplication | Low | Low | Quick fix, add check |
| #8 SSE design | Low | N/A | Document behavior |
| #9 Broadcast channel | Very Low | Low | Add debug logging |
| #10 Empty string | Low | Low | Quick fix, filter empty |
