-- Add namespaces column to users table
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS namespaces TEXT[] NOT NULL DEFAULT ARRAY['default', 'polymarket'];

-- Create index for namespace lookups
CREATE INDEX IF NOT EXISTS idx_users_namespaces ON users USING GIN (namespaces);
