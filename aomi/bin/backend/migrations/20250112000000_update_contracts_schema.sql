-- Add new columns to contracts table for enhanced metadata and tracking
-- Add new columns
ALTER TABLE contracts
    ADD COLUMN IF NOT EXISTS name TEXT NOT NULL DEFAULT 'Unknown',
    ADD COLUMN IF NOT EXISTS symbol TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS is_proxy BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS implementation_address TEXT,
    ADD COLUMN IF NOT EXISTS tvl DECIMAL,
    ADD COLUMN IF NOT EXISTS transaction_count BIGINT,
    ADD COLUMN IF NOT EXISTS last_activity_at BIGINT,
    ADD COLUMN IF NOT EXISTS data_source TEXT NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    ADD COLUMN IF NOT EXISTS updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_contracts_tvl ON contracts(tvl DESC) WHERE tvl IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_tx_count ON contracts(transaction_count DESC) WHERE transaction_count IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_updated ON contracts(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_contracts_symbol ON contracts(symbol) WHERE symbol IS NOT NULL;
