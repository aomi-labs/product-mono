# DeFi Master Integration

This module provides an AI assistant specialized in decentralized finance (DeFi) data analysis and real-time market information.

## Overview

The DeFi Master app allows users to:
- Check token prices across multiple chains
- Discover yield farming opportunities
- Monitor gas prices on multiple networks
- Get swap quotes from DEX aggregators
- Analyze protocol TVL rankings
- Compare cross-chain bridges

## Available Tools

| Tool | Description |
|------|-------------|
| `get_token_price` | Get current price for any cryptocurrency |
| `get_yield_opportunities` | Find DeFi yield pools sorted by APY |
| `get_gas_prices` | Get gas prices for supported chains |
| `get_swap_quote` | Get DEX aggregator quotes (requires API key) |
| `get_defi_protocols` | Get top DeFi protocols by TVL |
| `get_chain_tvl` | Get TVL rankings by blockchain |
| `get_bridges` | Get top cross-chain bridge volumes |

## Data Sources

### DeFiLlama (Free, No API Key Required)
- **Token prices**: `coins.llama.fi` - Real-time prices with high confidence
- **Yield data**: `yields.llama.fi` - APY for 10,000+ pools
- **TVL data**: `api.llama.fi` - Protocol and chain TVL
- **Bridges**: `bridges.llama.fi` - Cross-chain bridge volumes

### Owlracle (Free Tier, No API Key Required)
- **Gas prices**: Multi-chain gas oracle
- **Supported chains**: Ethereum, Arbitrum, Optimism, Polygon, Base, BSC, Avalanche, Fantom

### 0x API (Requires API Key)
- **Swap quotes**: DEX aggregation across 100+ liquidity sources
- **Get API key**: https://0x.org/products/swap

## Configuration

### Environment Variables

```bash
# Optional: Required only for swap quotes
ZEROX_API_KEY=your_api_key_here
```

## Usage Examples

### Get Token Prices
```
"What's the price of ETH?"
"Check USDC and USDT prices"
"Get price for ethereum:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
```

### Find Yield Opportunities
```
"Find stablecoin yields above 5% APY"
"Show me Aave pools on Arbitrum"
"What are the best yield opportunities with over $10M TVL?"
```

### Check Gas Prices
```
"What's the current gas on Ethereum?"
"Compare gas prices across all L2s"
"Is it cheaper to transact on Base or Arbitrum?"
```

### Get Swap Quotes
```
"Get a quote to swap 1000 USDC for ETH"
"What's the best rate for swapping WBTC to DAI?"
```

### Explore Protocols
```
"What are the top DEXs by TVL?"
"Show me lending protocols on Ethereum"
"List the biggest DeFi protocols"
```

### Chain & Bridge Analysis
```
"Which chains have the most TVL?"
"What are the most used bridges?"
"Compare Arbitrum and Optimism TVL"
```

## Supported Chains for Gas

| Chain | Code | Chain ID |
|-------|------|----------|
| Ethereum | eth | 1 |
| Arbitrum | arb | 42161 |
| Optimism | opt | 10 |
| Polygon | poly | 137 |
| Base | base | 8453 |
| BSC | bsc | 56 |
| Avalanche | avax | 43114 |
| Fantom | ftm | 250 |

## Common Token Symbols

### Major Tokens
- ETH, WETH - Ethereum
- BTC, WBTC - Bitcoin (wrapped)
- BNB - Binance Coin
- SOL - Solana
- AVAX - Avalanche

### Stablecoins
- USDC - USD Coin
- USDT - Tether
- DAI - MakerDAO DAI

### DeFi Tokens
- UNI - Uniswap
- AAVE - Aave
- LINK - Chainlink
- MKR - Maker
- CRV - Curve
- LDO - Lido

### L2 Tokens
- ARB - Arbitrum
- OP - Optimism
- MATIC/POL - Polygon

## Protocol Categories

Filter protocols by these categories:
- **DEXes** - Decentralized exchanges (Uniswap, Curve, etc.)
- **Lending** - Lending protocols (Aave, Compound, etc.)
- **Liquid Staking** - Staking derivatives (Lido, Rocket Pool, etc.)
- **Bridge** - Cross-chain bridges
- **Yield** - Yield aggregators (Yearn, Convex, etc.)
- **CDP** - Collateralized Debt Positions (MakerDAO, etc.)
- **Derivatives** - Options, perps, synthetics
- **Yield Aggregator** - Auto-compounding vaults

## Rate Limits

### DeFiLlama
- No official rate limits for public endpoints
- Recommended: <100 requests/minute for good citizenship

### Owlracle
- Free tier: Reasonable usage without key
- For high-volume: Register for API key at owlracle.info

### 0x API
- Requires API key for all requests
- Free tier available at https://0x.org

## Risk Considerations

When providing DeFi advice, the assistant considers:
- **Smart contract risk** - Unaudited or new protocols
- **Impermanent loss** - Risk in AMM liquidity pools
- **Stablecoin risk** - Depegging potential
- **Bridge risk** - Historical bridge exploits
- **Gas costs** - L1 vs L2 transaction economics

## Resources

- [DeFiLlama](https://defillama.com) - DeFi TVL aggregator
- [Owlracle](https://owlracle.info) - Gas oracle
- [0x](https://0x.org) - DEX aggregator API
