#!/bin/bash
# Seed database with contract data (idempotent)
# Usage: DATABASE_URL=... ./scripts/seed.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR"

if [ -z "${DATABASE_URL:-}" ]; then
    echo "❌ ERROR: DATABASE_URL is required"
    exit 1
fi

echo "🌱 Seeding database..."
echo "   Target: ${DATABASE_URL%%@*}@****"

# Check if contracts.csv exists
CONTRACTS_CSV="$ROOT_DIR/contracts.csv"
if [ ! -f "$CONTRACTS_CSV" ]; then
    echo "⚠️  No contracts.csv found, skipping contract import"
    exit 0
fi

# Count existing contracts
EXISTING=$(psql "$DATABASE_URL" -t -c "SELECT COUNT(*) FROM contracts;" 2>/dev/null | tr -d ' ')
echo "   📊 Existing contracts: $EXISTING"

# Import contracts from CSV using COPY (fast bulk import)
# CSV format: address,chain,chain_id,source_code,abi,name,symbol,description,is_proxy,implementation_address,last_activity_at,created_at,updated_at,protocol,contract_type,version
echo "   📥 Importing from contracts.csv..."

# Create temp table, load CSV, then upsert
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<EOF
-- Create temp table for import
CREATE TEMP TABLE contracts_import (LIKE contracts INCLUDING ALL);

-- Import CSV (skip header)
\COPY contracts_import(address, chain, chain_id, source_code, abi) FROM '$CONTRACTS_CSV' WITH (FORMAT csv, HEADER true);

-- Upsert into main table (idempotent)
INSERT INTO contracts (address, chain, chain_id, source_code, abi)
SELECT address, chain, chain_id, source_code, abi
FROM contracts_import
ON CONFLICT (chain_id, address) DO UPDATE SET
    source_code = EXCLUDED.source_code,
    abi = EXCLUDED.abi,
    updated_at = EXTRACT(EPOCH FROM NOW())::BIGINT;

-- Report results
SELECT 'Imported/Updated ' || COUNT(*) || ' contracts' FROM contracts_import;

DROP TABLE contracts_import;
EOF

# Count after import
AFTER=$(psql "$DATABASE_URL" -t -c "SELECT COUNT(*) FROM contracts;" 2>/dev/null | tr -d ' ')
echo "   📊 Contracts after seed: $AFTER"

echo "✅ Seeding complete!"
