-- Normalize api_keys table: one row per namespace instead of JSONB array
-- This migration converts allowed_namespaces JSONB array to a single namespace TEXT column

-- Step 1: Create a new table with the normalized structure
CREATE TABLE IF NOT EXISTS api_keys_new (
    id BIGSERIAL PRIMARY KEY,
    api_key TEXT NOT NULL,
    label TEXT,
    namespace TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    UNIQUE (api_key, namespace)
);

-- Step 2: Migrate data from old table to new table (one row per namespace)
INSERT INTO api_keys_new (api_key, label, namespace, is_active, created_at)
SELECT 
    api_key,
    label,
    namespace::TEXT,
    is_active,
    created_at
FROM api_keys, jsonb_array_elements_text(allowed_namespaces) AS namespace
ON CONFLICT (api_key, namespace) DO NOTHING;

-- Step 3: Drop the old table
DROP TABLE IF EXISTS api_keys;

-- Step 4: Rename new table to api_keys
ALTER TABLE api_keys_new RENAME TO api_keys;

-- Step 5: Create indexes
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys(is_active);
CREATE INDEX IF NOT EXISTS idx_api_keys_api_key ON api_keys(api_key);
CREATE INDEX IF NOT EXISTS idx_api_keys_namespace ON api_keys(namespace);
