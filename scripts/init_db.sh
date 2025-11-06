#!/usr/bin/env bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Database configuration
DB_USER="${DB_USER:-$USER}"
DB_NAME="${DB_NAME:-chatbot}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"

# PostgreSQL binary path
PSQL="${PSQL:-$(which psql)}"

echo -e "${YELLOW}Initializing database for chatbot...${NC}"

# Check if PostgreSQL is running
if ! $PSQL -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c '\q' 2>/dev/null; then
    echo -e "${RED}Error: PostgreSQL is not running or not accessible${NC}"
    echo -e "${YELLOW}Try running: brew services start postgresql@17${NC}"
    exit 1
fi

# Create database if it doesn't exist
echo -e "${YELLOW}Creating database '$DB_NAME' if it doesn't exist...${NC}"
$PSQL -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q 1 || \
    $PSQL -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "CREATE DATABASE $DB_NAME"

echo -e "${GREEN}✓ Database '$DB_NAME' ready${NC}"

# Create contracts table
echo -e "${YELLOW}Creating contracts table...${NC}"
$PSQL -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" <<EOF
-- Drop table if exists (for development)
DROP TABLE IF EXISTS contracts CASCADE;

-- Create contracts table matching the Contract struct
CREATE TABLE contracts (
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    chain_id INTEGER NOT NULL,
    source_code TEXT NOT NULL,
    abi TEXT NOT NULL,
    PRIMARY KEY (chain_id, address)
);

-- Create index on chain_id for faster queries
CREATE INDEX idx_contracts_chain_id ON contracts(chain_id);

-- Create index on address for faster lookups
CREATE INDEX idx_contracts_address ON contracts(address);

-- Create index on chain name for lookups by name
CREATE INDEX idx_contracts_chain ON contracts(chain);

-- Display table structure
\d contracts
EOF

echo -e "${GREEN}✓ Contracts table created successfully!${NC}"

# Create transaction history tables
echo -e "${YELLOW}Creating transaction history tables...${NC}"
$PSQL -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" <<EOF
-- Drop tables if they exist (for development)
-- Must drop transactions first due to foreign key
DROP TABLE IF EXISTS transactions CASCADE;
DROP TABLE IF EXISTS transaction_records CASCADE;

-- Create transaction_records table
CREATE TABLE transaction_records (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    nonce BIGINT,
    last_fetched_at BIGINT,
    last_block_number BIGINT,
    total_transactions INTEGER,
    PRIMARY KEY (chain_id, address)
);

-- Create transactions table
CREATE TABLE transactions (
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
CREATE INDEX idx_tx_chain_address_block ON transactions(chain_id, address, block_number DESC);
CREATE INDEX idx_tx_hash ON transactions(hash);
CREATE INDEX idx_tx_timestamp ON transactions(chain_id, address, timestamp DESC);

-- Display table structures
\d transaction_records
\d transactions
EOF

echo -e "${GREEN}✓ Transaction history tables created successfully!${NC}"

# Create session persistence tables
echo -e "${YELLOW}Creating session persistence tables...${NC}"
$PSQL -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" <<EOF
-- Drop tables if they exist (for development)
-- Must drop in order due to foreign keys
DROP TABLE IF EXISTS messages CASCADE;
DROP TABLE IF EXISTS sessions CASCADE;
DROP TABLE IF EXISTS users CASCADE;

-- Create users table (public_key as primary identifier)
CREATE TABLE users (
    public_key TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Create sessions table with pending transaction support
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
    started_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    last_active_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    title TEXT,
    pending_transaction JSONB
);

-- Create unified messages table (both chat and agent history)
CREATE TABLE messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL DEFAULT 'chat',
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

-- Create indexes for performance
CREATE INDEX idx_sessions_public_key ON sessions(public_key);
CREATE INDEX idx_sessions_last_active ON sessions(last_active_at DESC);
CREATE INDEX idx_messages_session_type ON messages(session_id, message_type, timestamp ASC);

-- Display table structures
\d users
\d sessions
\d messages
EOF

echo -e "${GREEN}✓ Session persistence tables created successfully!${NC}"

# Display connection string
echo ""
echo -e "${YELLOW}Database connection string:${NC}"
echo -e "${GREEN}postgres://$DB_USER@$DB_HOST:$DB_PORT/$DB_NAME${NC}"
echo ""
echo -e "${YELLOW}To connect manually:${NC}"
echo -e "${GREEN}$PSQL -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME${NC}"
