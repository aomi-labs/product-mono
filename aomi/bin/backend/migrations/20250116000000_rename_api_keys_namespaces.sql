-- Rename allowed_chatbots to allowed_namespaces for api_keys
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'api_keys'
          AND column_name = 'allowed_chatbots'
    ) THEN
        ALTER TABLE api_keys RENAME COLUMN allowed_chatbots TO allowed_namespaces;
    END IF;
END $$;

ALTER TABLE IF EXISTS api_keys DROP CONSTRAINT IF EXISTS api_keys_allowed_chatbots_is_array;
ALTER TABLE IF EXISTS api_keys DROP CONSTRAINT IF EXISTS api_keys_allowed_chatbots_nonempty;
ALTER TABLE IF EXISTS api_keys DROP CONSTRAINT IF EXISTS api_keys_allowed_namespaces_is_array;
ALTER TABLE IF EXISTS api_keys DROP CONSTRAINT IF EXISTS api_keys_allowed_namespaces_nonempty;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'api_keys'
          AND column_name = 'allowed_namespaces'
    ) THEN
        EXECUTE 'ALTER TABLE api_keys ADD CONSTRAINT api_keys_allowed_namespaces_is_array CHECK (jsonb_typeof(allowed_namespaces) = ''array'')';
        EXECUTE 'ALTER TABLE api_keys ADD CONSTRAINT api_keys_allowed_namespaces_nonempty CHECK (jsonb_array_length(allowed_namespaces) > 0)';
    END IF;
END $$;
