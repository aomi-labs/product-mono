-- Database initialization script for Docker PostgreSQL
-- This script creates the required tables for the chatbot application

-- Create contracts table matching the Contract struct
CREATE TABLE IF NOT EXISTS contracts (
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    chain_id INTEGER NOT NULL,
    source_code TEXT NOT NULL,
    abi TEXT NOT NULL,
    PRIMARY KEY (chain_id, address)
);

-- Create indexes on contracts table for faster queries
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

-- Create session persistence tables
-- Create users table (public_key as primary identifier)
CREATE TABLE IF NOT EXISTS users (
    public_key TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Create sessions table with pending transaction support
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
    started_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    last_active_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    title TEXT,
    pending_transaction JSONB
);

-- Create unified messages table (both chat and agent history)
CREATE TABLE IF NOT EXISTS messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL DEFAULT 'chat',
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Create indexes for session persistence
CREATE INDEX IF NOT EXISTS idx_sessions_public_key ON sessions(public_key);
CREATE INDEX IF NOT EXISTS idx_sessions_last_active ON sessions(last_active_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_session_type ON messages(session_id, message_type, timestamp ASC);