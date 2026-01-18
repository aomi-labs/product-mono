-- Track whether session messages have been persisted to avoid duplicate flushes
ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS messages_persisted BOOLEAN NOT NULL DEFAULT FALSE;

-- Remove the previous dedupe index if it exists
DROP INDEX IF EXISTS idx_messages_dedupe;
