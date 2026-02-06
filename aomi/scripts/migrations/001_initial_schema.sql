-- Aomi Database Schema v1.0
-- Clean consolidated schema for DO Managed PostgreSQL
-- Generated: 2026-02-04

-- ============================================================================
-- CONTRACTS & TRANSACTIONS (Blockchain data)
-- ============================================================================

-- Smart contracts with metadata
CREATE TABLE IF NOT EXISTS contracts (
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    chain_id INTEGER NOT NULL,
    source_code TEXT NOT NULL,
    abi TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT 'Unknown',
    symbol TEXT,
    description TEXT,
    is_proxy BOOLEAN NOT NULL DEFAULT false,
    implementation_address TEXT,
    last_activity_at BIGINT,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    protocol TEXT,
    contract_type TEXT,
    version TEXT,
    PRIMARY KEY (chain_id, address)
);

CREATE INDEX IF NOT EXISTS idx_contracts_chain_id ON contracts(chain_id);
CREATE INDEX IF NOT EXISTS idx_contracts_address ON contracts(address);
CREATE INDEX IF NOT EXISTS idx_contracts_chain ON contracts(chain);
CREATE INDEX IF NOT EXISTS idx_contracts_last_activity ON contracts(last_activity_at DESC) WHERE last_activity_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_updated ON contracts(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_contracts_symbol ON contracts(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_protocol ON contracts(protocol) WHERE protocol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_type ON contracts(contract_type) WHERE contract_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_contracts_version ON contracts(version) WHERE version IS NOT NULL;

-- Transaction fetch tracking per address
CREATE TABLE IF NOT EXISTS transaction_records (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    nonce BIGINT,
    last_fetched_at BIGINT,
    last_block_number BIGINT,
    total_transactions INTEGER,
    PRIMARY KEY (chain_id, address)
);

-- Individual transactions
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

CREATE INDEX IF NOT EXISTS idx_tx_chain_address_block ON transactions(chain_id, address, block_number DESC);
CREATE INDEX IF NOT EXISTS idx_tx_hash ON transactions(hash);
CREATE INDEX IF NOT EXISTS idx_tx_timestamp ON transactions(chain_id, address, timestamp DESC);

-- ============================================================================
-- USERS & SESSIONS (Application state)
-- ============================================================================

-- Users identified by wallet public key
CREATE TABLE IF NOT EXISTS users (
    public_key TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    namespaces TEXT[] NOT NULL DEFAULT ARRAY['default', 'polymarket'],
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_users_namespaces ON users USING GIN (namespaces);

-- Chat sessions with optional wallet binding
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
    started_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    last_active_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    title TEXT,
    pending_transaction JSONB,
    messages_persisted BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_sessions_public_key ON sessions(public_key);
CREATE INDEX IF NOT EXISTS idx_sessions_last_active ON sessions(last_active_at DESC);

-- Chat and agent messages
CREATE TABLE IF NOT EXISTS messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL DEFAULT 'chat',
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_messages_session_type ON messages(session_id, message_type, timestamp ASC);

-- ============================================================================
-- AUTH & ACCESS CONTROL
-- ============================================================================

-- API keys with namespace access (one row per api_key+namespace)
CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    api_key TEXT NOT NULL,
    label TEXT,
    namespace TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    UNIQUE (api_key, namespace)
);

CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys(is_active);
CREATE INDEX IF NOT EXISTS idx_api_keys_api_key ON api_keys(api_key);
CREATE INDEX IF NOT EXISTS idx_api_keys_namespace ON api_keys(namespace);

-- Wallet signature challenges for session binding
CREATE TABLE IF NOT EXISTS signup_challenges (
    session_id VARCHAR(255) PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    nonce VARCHAR(64) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- ============================================================================
-- MIGRATION TRACKING (for sqlx compatibility)
-- ============================================================================

CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL
);

-- Record this migration
INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
VALUES (1, 'initial_schema', true, E'\\x00', 0)
ON CONFLICT (version) DO NOTHING;
