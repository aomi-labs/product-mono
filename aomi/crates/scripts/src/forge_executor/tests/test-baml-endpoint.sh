#!/usr/bin/env bash
set -euo pipefail

# Test script to manually verify BAML server endpoints
# Usage: ./test-baml-endpoint.sh [ping|extract|generate]

# Bypass proxy for localhost to avoid 502 errors
export no_proxy="localhost,127.0.0.1"
export NO_PROXY="localhost,127.0.0.1"

BAML_URL="${BAML_SERVER_URL:-http://127.0.0.1:2024}"

test_ping() {
  echo "Testing BAML ping endpoint..."
  curl -v --max-time 5 --noproxy localhost,127.0.0.1 "$BAML_URL/_debug/ping"
  local status=$?
  echo ""
  if [ $status -ne 0 ]; then
    echo "❌ Ping failed with status $status"
    echo "Possible causes:"
    echo "  - BAML server not running or suspended"
    echo "  - Run: pkill -9 -f 'baml-cli serve' to clean up"
    return 1
  fi
  echo "✅ Ping successful"
}

test_extract_contract_info() {
  echo "Testing ExtractContractInfo endpoint..."

  # Minimal test payload
  local payload='{
    "contracts": [
      {
        "address": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "abi": "[{\"inputs\":[],\"name\":\"deposit\",\"outputs\":[],\"stateMutability\":\"payable\",\"type\":\"function\"}]",
        "description": "WETH token",
        "source_code": "contract WETH { function deposit() public payable {} }"
      }
    ],
    "group_operations": [
      "Wrap 1 ETH to WETH using function deposit() of IWETH interface [value: 1 ether]"
    ]
  }'

  echo "Payload:"
  echo "$payload" | jq .
  echo ""

  echo "Sending request to $BAML_URL/call/ExtractContractInfo..."
  curl -v -X POST "$BAML_URL/call/ExtractContractInfo" \
    -H "Content-Type: application/json" \
    -d "$payload" | jq .
  echo ""
}

test_generate_script() {
  echo "Testing GenerateScript endpoint..."

  # Minimal test payload
  local payload='{
    "extracted_contracts": [
      {
        "address": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "description": "WETH",
        "functions": [
          {
            "name": "deposit",
            "signature": "deposit()",
            "abi": "{\"inputs\":[],\"name\":\"deposit\",\"outputs\":[],\"stateMutability\":\"payable\",\"type\":\"function\"}",
            "body": "function deposit() public payable {}"
          }
        ],
        "storage": [],
        "events": []
      }
    ],
    "group_operations": [
      "Wrap 1 ETH to WETH using function deposit() of IWETH interface [value: 1 ether]"
    ]
  }'

  echo "Payload:"
  echo "$payload" | jq .
  echo ""

  echo "Sending request to $BAML_URL/call/GenerateScript..."
  curl -v -X POST "$BAML_URL/call/GenerateScript" \
    -H "Content-Type: application/json" \
    -d "$payload" | jq .
  echo ""
}

case "${1:-ping}" in
  ping)
    test_ping
    ;;
  extract)
    test_extract_contract_info
    ;;
  generate)
    test_generate_script
    ;;
  all)
    test_ping
    echo "---"
    test_extract_contract_info
    echo "---"
    test_generate_script
    ;;
  *)
    echo "Usage: $0 [ping|extract|generate|all]"
    exit 1
    ;;
esac
