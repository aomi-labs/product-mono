-- Initial schema migration (replaces init_db.sql)
-- Create contracts table
CREATE TABLE IF NOT EXISTS contracts (
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    chain_id INTEGER NOT NULL,
    source_code TEXT NOT NULL,
    abi TEXT NOT NULL,
    PRIMARY KEY (chain_id, address)
);

-- Create indexes on contracts table
CREATE INDEX IF NOT EXISTS idx_contracts_chain_id ON contracts(chain_id);
CREATE INDEX IF NOT EXISTS idx_contracts_address ON contracts(address);
CREATE INDEX IF NOT EXISTS idx_contracts_chain ON contracts(chain);

-- Create transaction_records table
CREATE TABLE IF NOT EXISTS transaction_records (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    nonce BIGINT,
    last_fetched_at BIGINT,
    last_block_number BIGINT,
    total_transactions INTEGER,
    PRIMARY KEY (chain_id, address)
);

-- Create transactions table
CREATE TABLE IF NOT EXISTS transactions (
    id BIGSERIAL PRIMARY KEY,
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    hash TEXT NOT NULL,
    block_number BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    value TEXT NOT NULL,
    gas TEXT NOT NULL,
    gas_price TEXT NOT NULL,
    gas_used TEXT NOT NULL,
    is_error TEXT NOT NULL,
    input TEXT NOT NULL,
    contract_address TEXT,
    FOREIGN KEY (chain_id, address) REFERENCES transaction_records(chain_id, address),
    UNIQUE (chain_id, address, hash)
);

-- Create indexes for transactions
CREATE INDEX IF NOT EXISTS idx_tx_chain_address_block ON transactions(chain_id, address, block_number DESC);
CREATE INDEX IF NOT EXISTS idx_tx_hash ON transactions(hash);
CREATE INDEX IF NOT EXISTS idx_tx_timestamp ON transactions(chain_id, address, timestamp DESC);
