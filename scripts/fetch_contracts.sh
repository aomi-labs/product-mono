#!/bin/bash

# Script to fetch top verified contracts from Etherscan and store in database
# Usage: export ETHERSCAN_API_KEY=your_key && ./fetch_contracts.sh

# Example queries:
# View all contracts with clean output:
#   $PSQL_BIN $DATABASE_URL -c "SELECT address, chain, chain_id FROM contracts ORDER BY address;"
#
# Count total contracts:
#   $PSQL_BIN $DATABASE_URL -c "SELECT COUNT(*) FROM contracts;"
#
# View a specific contract:
#   $PSQL_BIN $DATABASE_URL -c "SELECT address, chain, chain_id, LENGTH(source_code) as src_len, LENGTH(abi) as abi_len FROM contracts WHERE address = '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48';"

set -e

ETHERSCAN_API_KEY="${ETHERSCAN_API_KEY}"
# Defaults for explicit connection flags
PGUSER_DEFAULT="${USER:-postgres}"
PGHOST_DEFAULT="localhost"
PGPORT_DEFAULT="5432"
PGDATABASE_DEFAULT="chatbot"
DATABASE_URL="${DATABASE_URL:-postgres://${PGUSER_DEFAULT}@${PGHOST_DEFAULT}:${PGPORT_DEFAULT}/${PGDATABASE_DEFAULT}}"
PSQL_BIN="${PSQL_BIN:-psql}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_CSV="${SCRIPT_DIR}/top_contracts.csv"

if [ -z "$ETHERSCAN_API_KEY" ]; then
    echo "Error: ETHERSCAN_API_KEY environment variable not set"
    echo "Get your API key at: https://etherscan.io/myapikey"
    echo "Usage: export ETHERSCAN_API_KEY=your_key && ./fetch_contracts.sh"
    exit 1
fi

if [ ! -f "$CONTRACTS_CSV" ]; then
    echo "Error: Contract list file not found: $CONTRACTS_CSV"
    exit 1
fi

# Derive connection args (favor explicit flags over URI)
PGUSER_CONN="${PGUSER:-$PGUSER_DEFAULT}"
PGHOST_CONN="${PGHOST:-$PGHOST_DEFAULT}"
PGPORT_CONN="${PGPORT:-$PGPORT_DEFAULT}"
PGDATABASE_CONN="${PGDATABASE:-$PGDATABASE_DEFAULT}"

# Test database connection before starting
echo "Testing database connection..."
set +e
$PSQL_BIN -h "$PGHOST_CONN" -p "$PGPORT_CONN" -U "$PGUSER_CONN" -d "$PGDATABASE_CONN" -c '\q' >/dev/null 2>&1
DB_CONN_EXIT=$?
set -e
if [ $DB_CONN_EXIT -ne 0 ]; then
    echo "Error: Cannot connect to database: $DATABASE_URL"
    echo "Make sure PostgreSQL is running and the user exists."
    echo "For local Postgres, try: USER=\$(whoami) ./scripts/fetch_contracts.sh"
    exit 1
fi

echo "Fetching top verified contracts from Etherscan..."
echo "API Key: ${ETHERSCAN_API_KEY:0:10}..."
echo "Database: $DATABASE_URL"
echo "Contract list: $CONTRACTS_CSV"
echo ""

TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

# Count total contracts (excluding header)
TOTAL=$(tail -n +2 "$CONTRACTS_CSV" | wc -l | tr -d ' ')
COUNT=0
SKIPPED=0
FETCHED=0
FAILED=0

# Read CSV file (skip header) using process substitution to avoid subshell
while IFS=',' read -r ADDRESS NAME CATEGORY; do
    COUNT=$((COUNT + 1))

    # Strip any whitespace/control characters from fields
    ADDRESS=$(echo "$ADDRESS" | tr -d '\r\n' | xargs)
    NAME=$(echo "$NAME" | tr -d '\r\n' | xargs)
    CATEGORY=$(echo "$CATEGORY" | tr -d '\r\n' | xargs)

    printf "[%d/%d] Checking %s (%s) [%s]...\n" "$COUNT" "$TOTAL" "$NAME" "$ADDRESS" "$CATEGORY"

    # Convert address to lowercase for DB query
    ADDRESS_LOWER=$(echo "$ADDRESS" | tr '[:upper:]' '[:lower:]')

    # Check if contract already exists in database
    EXISTS=$($PSQL_BIN "$DATABASE_URL" -t -c "SELECT EXISTS(SELECT 1 FROM contracts WHERE chain_id = 1 AND address = '$ADDRESS_LOWER');" 2>/dev/null | tr -d ' ')

    if [ "$EXISTS" = "t" ]; then
        echo "  ⊙ Already in database, skipping"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    # Fetch contract source code from Etherscan V2 API
    RESPONSE=$(curl -s "https://api.etherscan.io/v2/api?chainid=1&module=contract&action=getsourcecode&address=$ADDRESS&apikey=$ETHERSCAN_API_KEY")

    # Check if request was successful
    STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)
    if [ "$STATUS" != "1" ]; then
        echo "  ✗ Failed to fetch contract (status: $STATUS)"
        FAILED=$((FAILED + 1))
        continue
    fi

    # Extract source code and ABI
    SOURCE_CODE=$(echo "$RESPONSE" | jq -r '.result[0].SourceCode' 2>/dev/null)
    ABI=$(echo "$RESPONSE" | jq -r '.result[0].ABI' 2>/dev/null)
    CONTRACT_NAME=$(echo "$RESPONSE" | jq -r '.result[0].ContractName' 2>/dev/null)

    # Check if contract is verified
    if [ "$SOURCE_CODE" = "Contract source code not verified" ] || [ -z "$SOURCE_CODE" ] || [ "$SOURCE_CODE" = "null" ]; then
        echo "  ✗ Contract not verified"
        FAILED=$((FAILED + 1))
        continue
    fi

    # Save to temp files
    echo "$SOURCE_CODE" > "$TEMP_DIR/source_$ADDRESS.sol"
    echo "$ABI" > "$TEMP_DIR/abi_$ADDRESS.json"

    # Escape single quotes for SQL
    SOURCE_CODE_ESCAPED=$(echo "$SOURCE_CODE" | sed "s/'/''/g")
    ABI_ESCAPED=$(echo "$ABI" | sed "s/'/''/g")

    # Insert into database (chain_id 1 = Ethereum mainnet)
    # Use set +e temporarily to handle database errors gracefully
    set +e
    DB_RESULT=$($PSQL_BIN -h "$PGHOST_CONN" -p "$PGPORT_CONN" -U "$PGUSER_CONN" -d "$PGDATABASE_CONN" -c "
        INSERT INTO contracts (address, chain, chain_id, source_code, abi)
        VALUES ('$ADDRESS_LOWER', 'ethereum', 1, '$SOURCE_CODE_ESCAPED', '$ABI_ESCAPED')
        ON CONFLICT (chain_id, address)
        DO UPDATE SET
            chain = EXCLUDED.chain,
            source_code = EXCLUDED.source_code,
            abi = EXCLUDED.abi;
    " 2>&1)
    DB_EXIT_CODE=$?
    set -e
    
    if [ $DB_EXIT_CODE -eq 0 ] && echo "$DB_RESULT" | grep -q "INSERT\|UPDATE"; then
        echo "  ✓ Stored $CONTRACT_NAME"
        FETCHED=$((FETCHED + 1))
    else
        echo "  ✗ Failed to store in database: $DB_RESULT"
        echo "  ⚠️  Continuing with next contract..."
    fi

    # Rate limiting (5 requests per second for free tier)
    if [ $((COUNT % 5)) -eq 0 ]; then
        sleep 1
    fi
done < <(tail -n +2 "$CONTRACTS_CSV")

echo ""
echo "✓ Finished!"
echo "Summary:"
echo "  Total contracts: $TOTAL"
echo "  Already in DB (skipped): $SKIPPED"
echo "  Successfully fetched: $FETCHED"
echo "  Failed: $FAILED"
