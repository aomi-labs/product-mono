-- Deduplicate messages by enforcing a unique key on core identity fields
CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_dedupe
    ON messages (session_id, sender, timestamp, content);
