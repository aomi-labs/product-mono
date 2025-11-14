# Required Contracts for Intent Testing

This document lists all contracts needed to support the example intents in `CONTRACT_SEARCH_INTENTS.md`.

## Ethereum Mainnet (chain_id: 1)

### Tokens
- **USDC (Proxy)**: `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`
  - Type: ERC20, Proxy
  - Protocol: Circle
  - Tags: token,stablecoin,erc20
  - Used for: Intents 1, 11, 12

- **DAI**: `0x6B175474E89094C44Da98b954EedeAC495271d0F`
  - Type: ERC20
  - Protocol: MakerDAO
  - Tags: token,stablecoin,erc20
  - Used for: Intent 2

### Uniswap
- **Uniswap V2 Router**: `0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D`
  - Type: UniswapV2Router
  - Protocol: Uniswap
  - Version: v2
  - Tags: dex,router,swap
  - Used for: Intent 17

- **Uniswap V2 USDC/WETH Pair**: `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc`
  - Type: UniswapV2Pair
  - Protocol: Uniswap
  - Version: v2
  - Tags: dex,amm,pair
  - Used for: Intent 6

- **Uniswap V3 Router**: `0xE592427A0AEce92De3Edee1F18E0157C05861564`
  - Type: SwapRouter
  - Protocol: Uniswap
  - Version: v3
  - Tags: dex,router,swap
  - Used for: Intent 1

### Aave
- **Aave V3 Pool**: `0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2`
  - Type: LendingPool
  - Protocol: Aave
  - Version: v3
  - Tags: lending,defi
  - Used for: Intent 2, 13

### Compound
- **Compound cUSDC (cToken)**: `0x39AA39c021dfbaE8faC545936693aC917d5E7563`
  - Type: CErc20
  - Protocol: Compound
  - Version: v2
  - Tags: lending,ctoken,defi
  - Used for: Intent 4, 13

### Curve
- **Curve 3pool**: `0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7`
  - Type: StableSwap
  - Protocol: Curve
  - Version: v1
  - Tags: dex,stable,swap
  - Used for: Intent 5, 15

### NFT Examples
- **Bored Ape Yacht Club (BAYC)**: `0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D`
  - Type: ERC721
  - Protocol: Yuga Labs
  - Tags: nft,erc721,collectible
  - Used for: Intent 14

## Arbitrum (chain_id: 42161)

### Tokens
- **USDC (Bridged)**: `0xaf88d065e77c8cC2239327C5EDb3A432268e5831`
  - Type: ERC20
  - Protocol: Circle
  - Tags: token,stablecoin,erc20,bridged
  - Used for: Intent 3

### Uniswap
- **Uniswap V3 Router**: `0xE592427A0AEce92De3Edee1F18E0157C05861564`
  - Type: SwapRouter
  - Protocol: Uniswap
  - Version: v3
  - Tags: dex,router,swap
  - Used for: Intent 16

## Polygon (chain_id: 137)

### Chainlink
- **Chainlink USDT/USD Price Feed**: `0x0A6513e40db6EB1b165753AD52E80663aeA50545`
  - Type: AggregatorV3
  - Protocol: Chainlink
  - Tags: oracle,pricefeed
  - Used for: Intent 7

## Optimism (chain_id: 10)

### Aave
- **Aave V3 Pool**: `0x794a61358D6845594F94dc1DB02A252b5b4814aD`
  - Type: LendingPool
  - Protocol: Aave
  - Version: v3
  - Tags: lending,defi
  - Used for: Intent 8

## Base (chain_id: 8453)

### Uniswap
- **Uniswap V3 Router**: `0x2626664c2603336E57B271c5C0b26F421741e481`
  - Type: SwapRouter
  - Protocol: Uniswap
  - Version: v3
  - Tags: dex,router,swap
  - Used for: Intent 9

## Summary by Intent

| Intent # | Contract(s) Needed | Chain |
|----------|-------------------|-------|
| 1 | Uniswap V3 Router | Ethereum |
| 2 | DAI, Aave V3 Pool | Ethereum |
| 3 | USDC | Arbitrum |
| 4 | Compound cUSDC | Ethereum |
| 5 | Curve 3pool | Ethereum |
| 6 | Uniswap V2 Pair | Ethereum |
| 7 | Chainlink USDT/USD Feed | Polygon |
| 8 | Aave V3 Pool | Optimism |
| 9 | Uniswap V3 Router | Base |
| 10 | Any ERC20 (USDC/DAI) | Ethereum |
| 11 | USDC | Ethereum |
| 12 | USDC | Ethereum |
| 13 | Aave/Compound | Ethereum |
| 14 | BAYC (ERC721) | Ethereum |
| 15 | Curve 3pool | Ethereum |
| 16 | Uniswap V3 Router | Arbitrum |
| 17 | Uniswap V2 Router | Ethereum |

## Total Contracts Required

- **Ethereum**: 10 contracts
- **Arbitrum**: 2 contracts
- **Polygon**: 1 contract
- **Optimism**: 1 contract
- **Base**: 1 contract

**Total: 15 unique contracts across 5 chains**

## Minimal Test Set

For quick testing, this minimal set covers most use cases:

1. **USDC (Ethereum)** - `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`
2. **DAI (Ethereum)** - `0x6B175474E89094C44Da98b954EedeAC495271d0F`
3. **Uniswap V3 Router (Ethereum)** - `0xE592427A0AEce92De3Edee1F18E0157C05861564`
4. **Aave V3 Pool (Ethereum)** - `0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2`
5. **USDC (Arbitrum)** - `0xaf88d065e77c8cC2239327C5EDb3A432268e5831`

This minimal set supports:
- Token operations (ERC20)
- DEX interactions
- Lending protocol
- Multi-chain queries
- Proxy contracts

## Metadata to Populate

For each contract, populate these fields:
- `name` - Contract name
- `symbol` - Token symbol (if applicable)
- `protocol` - Protocol name
- `contract_type` - Interface/type
- `version` - Protocol version
- `tags` - CSV tags for categorization
- `is_proxy` - Boolean for proxy detection
- `implementation_address` - For proxies

## Fetching Script

To populate the database with these contracts, use:

```bash
# Ethereum contracts
cargo run --bin contract-scraper -- \
  --chain-id 1 \
  --addresses 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48,0x6B175474E89094C44Da98b954EedeAC495271d0F,...

# Arbitrum contracts
cargo run --bin contract-scraper -- \
  --chain-id 42161 \
  --addresses 0xaf88d065e77c8cC2239327C5EDb3A432268e5831,...
```

Or manually add via database:
```sql
INSERT INTO contracts (address, chain, chain_id, name, symbol, protocol, contract_type, version, tags, ...)
VALUES ('0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48', 'ethereum', 1, 'USD Coin', 'USDC', 'Circle', 'ERC20', NULL, 'token,stablecoin,erc20', ...);
```
