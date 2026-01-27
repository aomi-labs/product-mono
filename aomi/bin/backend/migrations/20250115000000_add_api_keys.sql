-- Add API key access control table
CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    api_key TEXT NOT NULL UNIQUE,
    label TEXT,
    allowed_chatbots JSONB NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    CONSTRAINT api_keys_allowed_chatbots_is_array CHECK (jsonb_typeof(allowed_chatbots) = 'array'),
    CONSTRAINT api_keys_allowed_chatbots_nonempty CHECK (jsonb_array_length(allowed_chatbots) > 0)
);

CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys(is_active);
