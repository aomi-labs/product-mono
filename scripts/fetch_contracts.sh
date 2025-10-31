#!/bin/bash

# Script to fetch top verified contracts from Etherscan and store in database
# Usage: export ETHERSCAN_API_KEY=your_key && ./fetch_contracts.sh


    # View all contracts with clean output:
    # psql postgres://kevin@localhost:5432/chatbot -c "SELECT address, chain, chain_id FROM contracts ORDER BY address;"

    # Count total contracts:
    # psql postgres://kevin@localhost:5432/chatbot -c "SELECT COUNT(*) FROM contracts;"

    # View a specific contract:
    # psql postgres://kevin@localhost:5432/chatbot -c "SELECT address, chain, chain_id, LENGTH(source_code) as src_len, LENGTH(abi) as
    # abi_len FROM contracts WHERE address = '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48';"

set -e

ETHERSCAN_API_KEY="${ETHERSCAN_API_KEY}"
DATABASE_URL="${DATABASE_URL:-postgres://kevin@localhost:5432/chatbot}"
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

# Read CSV file (skip header)
tail -n +2 "$CONTRACTS_CSV" | while IFS=',' read -r ADDRESS NAME CATEGORY; do
    COUNT=$((COUNT + 1))

    echo "[$COUNT/$TOTAL] Fetching $NAME ($ADDRESS) [$CATEGORY]..."

    # Fetch contract source code from Etherscan V2 API
    RESPONSE=$(curl -s "https://api.etherscan.io/v2/api?chainid=1&module=contract&action=getsourcecode&address=$ADDRESS&apikey=$ETHERSCAN_API_KEY")

    # Check if request was successful
    STATUS=$(echo "$RESPONSE" | jq -r '.status')
    if [ "$STATUS" != "1" ]; then
        echo "  ✗ Failed to fetch contract"
        continue
    fi

    # Extract source code and ABI
    SOURCE_CODE=$(echo "$RESPONSE" | jq -r '.result[0].SourceCode')
    ABI=$(echo "$RESPONSE" | jq -r '.result[0].ABI')
    CONTRACT_NAME=$(echo "$RESPONSE" | jq -r '.result[0].ContractName')

    # Check if contract is verified
    if [ "$SOURCE_CODE" = "Contract source code not verified" ] || [ -z "$SOURCE_CODE" ] || [ "$SOURCE_CODE" = "null" ]; then
        echo "  ✗ Contract not verified"
        continue
    fi

    # Save to temp files
    echo "$SOURCE_CODE" > "$TEMP_DIR/source_$ADDRESS.sol"
    echo "$ABI" > "$TEMP_DIR/abi_$ADDRESS.json"

    # Escape single quotes for SQL
    SOURCE_CODE_ESCAPED=$(echo "$SOURCE_CODE" | sed "s/'/''/g")
    ABI_ESCAPED=$(echo "$ABI" | sed "s/'/''/g")
    ADDRESS_LOWER=$(echo "$ADDRESS" | tr '[:upper:]' '[:lower:]')

    # Insert into database (chain_id 1 = Ethereum mainnet)
    psql "$DATABASE_URL" -c "
        INSERT INTO contracts (address, chain, chain_id, source_code, abi)
        VALUES ('$ADDRESS_LOWER', 'ethereum', 1, '$SOURCE_CODE_ESCAPED', '$ABI_ESCAPED')
        ON CONFLICT (chain_id, address)
        DO UPDATE SET
            chain = EXCLUDED.chain,
            source_code = EXCLUDED.source_code,
            abi = EXCLUDED.abi;
    " > /dev/null 2>&1

    if [ $? -eq 0 ]; then
        echo "  ✓ Stored $CONTRACT_NAME"
    else
        echo "  ✗ Failed to store in database"
    fi

    # Rate limiting (5 requests per second for free tier)
    if [ $((COUNT % 5)) -eq 0 ]; then
        sleep 1
    fi
done

echo ""
echo "✓ Finished! Check your database for stored contracts."
