# Contract Search Intent Examples

This document contains example user intents that trigger contract ABI/source code lookups, demonstrating both exact address lookups and fuzzy search capabilities.

## Direct Protocol Interaction

### 1. Swap Tokens
**Intent:** "I want to swap 100 USDC for ETH on Uniswap V3"
- **Needs:** Uniswap V3 Router ABI
- **Search by:** `protocol="Uniswap"`, `version="v3"`, `contract_type="Router"`

### 2. Token Approval
**Intent:** "Approve DAI to be spent by the Aave lending pool"
- **Needs:** DAI token ABI + Aave LendingPool ABI
- **Search by:** `symbol="DAI"` + `protocol="Aave"`, `contract_type="LendingPool"`

### 3. Balance Check
**Intent:** "Check my USDC balance on Arbitrum"
- **Needs:** USDC contract ABI on Arbitrum
- **Search by:** `symbol="USDC"`, `chain_id=42161`

## Protocol Analysis

### 4. Function Discovery
**Intent:** "What functions are available on the Compound cUSDC contract?"
- **Needs:** cUSDC ABI
- **Search by:** `protocol="Compound"`, `name` contains "USDC"

### 5. Event Analysis
**Intent:** "Show me all events that Curve's 3pool emits"
- **Needs:** Curve 3pool ABI
- **Search by:** `protocol="Curve"`, `name="3pool"`

### 6. Implementation Details
**Intent:** "How does Uniswap V2 calculate reserves?"
- **Needs:** Uniswap V2 Pair source code
- **Search by:** `protocol="Uniswap"`, `version="v2"`, `contract_type="Pair"`

## Multi-Chain Discovery

### 7. Cross-Chain Price Feeds
**Intent:** "Find the Chainlink USDT/USD price feed on Polygon"
- **Needs:** Chainlink aggregator ABI
- **Search by:** `protocol="Chainlink"`, `chain_id=137`, `name` contains "USDT/USD"

### 8. Protocol Availability
**Intent:** "Is Aave deployed on Optimism?"
- **Needs:** Aave contracts on Optimism
- **Search by:** `protocol="Aave"`, `chain_id=10`

## Integration/Development

### 9. DEX Integration
**Intent:** "I need to integrate with a DEX on Base"
- **Needs:** Any DEX contract
- **Search by:** `tags="dex,amm"`, `chain_id=8453`

### 10. Standard Interface Learning
**Intent:** "Show me how to interact with ERC20 tokens"
- **Needs:** Any ERC20 ABI
- **Search by:** `contract_type="ERC20"`

## Debugging/Verification

### 11. Contract Verification
**Intent:** "Is this address the real USDC contract? 0xA0b8..."
- **Needs:** Exact contract lookup for verification
- **Search by:** `address="0xA0b8..."`, `chain_id=1`

### 12. Proxy Implementation Check
**Intent:** "What's the current implementation of the USDC proxy?"
- **Needs:** USDC proxy contract details
- **Search by:** `symbol="USDC"`, `is_proxy=true`

## Learning/Research

### 13. Pattern Analysis
**Intent:** "How do lending protocols implement interest calculations?"
- **Needs:** Source code from Aave/Compound
- **Search by:** `tags="lending"`, `protocol` in ["Aave", "Compound"]

### 14. Example Contracts
**Intent:** "Show me an example ERC721 contract"
- **Needs:** Any ERC721 source
- **Search by:** `contract_type="ERC721"`

## Vague/Fuzzy Queries

These queries demonstrate where fuzzy search becomes essential:

### 15. General Category Search
**Intent:** "I need a stablecoin swap"
- **Search by:** `tags="stable,swap,dex"`

### 16. Type-Based Discovery
**Intent:** "Find me a router on Arbitrum"
- **Search by:** `contract_type="Router"`, `chain_id=42161`

### 17. Natural Language Query
**Intent:** "What's that Uniswap thing on Ethereum?"
- **Search by:** `protocol` contains "uniswap", `chain_id=1`

## When Fuzzy Search Shines

Fuzzy search becomes essential when users:

- **Don't know exact addresses** - "I need the Uniswap router" vs "I need 0x..."
- **Want to compare across chains** - "Show me all USDC contracts"
- **Need similar contracts** - "Find all DEXes on Arbitrum"
- **Have vague descriptions** - "that token thing", "the swap contract"
- **Want to discover what's available** - "What lending protocols exist?"
- **Search by characteristics** - "Find all ERC20 tokens", "Show me proxy contracts"

## Search Priority Strategy

The fuzzy search follows this priority:

1. **Exact symbol match** - Fast, indexed lookup (e.g., "USDC")
2. **Combined filters** - contract_type (exact) + protocol (fuzzy) + version (exact)
3. **Tag matching** - CSV fuzzy contains (e.g., "dex,amm,defi")
4. **Name fuzzy search** - Fallback LIKE search

## Use Cases Summary

| Use Case | Example | Search Type |
|----------|---------|-------------|
| Token operations | Check USDC balance | Symbol exact |
| Protocol interaction | Swap on Uniswap V3 | Protocol + version |
| Multi-chain | Find contract on different chain | Symbol + chain_id |
| Discovery | Find all DEXes | Tags or contract_type |
| Learning | Example ERC20 | contract_type |
| Verification | Is this the real contract? | Address exact |
| Development | Integrate with lending | Tags + protocol |
