-- Add new columns to contracts table for enhanced metadata and tracking
-- Add new columns
ALTER TABLE contracts
    ADD COLUMN IF NOT EXISTS name TEXT NOT NULL DEFAULT 'Unknown',
    ADD COLUMN IF NOT EXISTS symbol TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS is_proxy BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS implementation_address TEXT,
    ADD COLUMN IF NOT EXISTS last_activity_at BIGINT,
    ADD COLUMN IF NOT EXISTS created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    ADD COLUMN IF NOT EXISTS updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    ADD COLUMN IF NOT EXISTS protocol TEXT,
    ADD COLUMN IF NOT EXISTS contract_type TEXT,
    ADD COLUMN IF NOT EXISTS version TEXT;

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_contracts_last_activity ON contracts(last_activity_at DESC) WHERE last_activity_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_updated ON contracts(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_contracts_symbol ON contracts(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_protocol ON contracts(protocol) WHERE protocol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_type ON contracts(contract_type) WHERE contract_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_version ON contracts(version) WHERE version IS NOT NULL;
