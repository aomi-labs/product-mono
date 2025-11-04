#!/bin/bash

# Debug script for contract insertion
ADDRESS="0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
ETHERSCAN_API_KEY="BYY29WWH6IHAB2KS8DXFG2S7YP9C5GQXT5"
DATABASE_URL="postgres://aomi@localhost:5432/chatbot"

echo "Fetching contract data..."
RESPONSE=$(curl -s "https://api.etherscan.io/v2/api?chainid=1&module=contract&action=getsourcecode&address=$ADDRESS&apikey=$ETHERSCAN_API_KEY")

echo "Extracting data..."
SOURCE_CODE=$(echo "$RESPONSE" | jq -r '.result[0].SourceCode' 2>/dev/null)
ABI=$(echo "$RESPONSE" | jq -r '.result[0].ABI' 2>/dev/null)
CONTRACT_NAME=$(echo "$RESPONSE" | jq -r '.result[0].ContractName' 2>/dev/null)

echo "Contract: $CONTRACT_NAME"
echo "Source code length: ${#SOURCE_CODE}"
echo "ABI length: ${#ABI}"

# Test escaping
echo "Testing SQL insertion..."
SOURCE_CODE_ESCAPED=$(echo "$SOURCE_CODE" | sed "s/'/''/g")
ABI_ESCAPED=$(echo "$ABI" | sed "s/'/''/g")
ADDRESS_LOWER=$(echo "$ADDRESS" | tr '[:upper:]' '[:lower:]')

# Try insertion with error output
/opt/homebrew/opt/postgresql@17/bin/psql "$DATABASE_URL" -c "
    INSERT INTO contracts (address, chain, chain_id, source_code, abi)
    VALUES ('$ADDRESS_LOWER', 'ethereum', 1, '$SOURCE_CODE_ESCAPED', '$ABI_ESCAPED')
    ON CONFLICT (chain_id, address)
    DO UPDATE SET
        chain = EXCLUDED.chain,
        source_code = EXCLUDED.source_code,
        abi = EXCLUDED.abi;
"

echo "Insertion result: $?"