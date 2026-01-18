# Polymarket Integration

This module provides an AI assistant specialized in Polymarket prediction markets analysis and trading.

## Overview

The Polymarket app allows users to:
- Query and discover prediction markets
- Analyze market trends, volumes, and liquidity
- Retrieve trade history
- Place orders on the Polymarket CLOB (Central Limit Order Book)

## Available Tools

| Tool | Description |
|------|-------------|
| `get_polymarket_markets` | Query markets with filtering by status, category, tags |
| `get_polymarket_market_details` | Get detailed info about a specific market |
| `get_polymarket_trades` | Retrieve historical trade data |
| `place_polymarket_order` | Submit a signed order to the Polymarket CLOB |

## Configuration

### No API Key Required for Reading

The Polymarket Gamma API (for market discovery) and Data API (for trades) are **public** and don't require authentication.

### API Key Required for Trading

To place orders, you need:

1. **A wallet with USDC on Polygon** - Polymarket operates on Polygon mainnet
2. **API Credentials** - Derived from your wallet's private key using the Polymarket CLOB client

The trading flow requires wallet signature for each order (EIP-712), which is handled through the `SendTransactionToWallet` tool.

## Placing a Bet

### Step 1: Discover Markets

Ask the bot to find markets:
```
Show me active crypto markets on Polymarket
```

### Step 2: Get Market Details

Get the token ID and pricing info:
```
Get details for the market "will-bitcoin-reach-100k"
```

### Step 3: Place Order

The bot will:
1. Construct the order payload with your parameters
2. Request your wallet signature via `SendTransactionToWallet`
3. Submit the signed order to Polymarket's CLOB

Example:
```
Buy 10 shares of YES at $0.45 for the Bitcoin 100k market
```

## Order Structure

Orders require:
- `tokenID` - The outcome token to trade (from market details)
- `price` - Price per share (0.00 to 1.00 USDC)
- `size` - Number of shares
- `side` - BUY or SELL
- `signature` - EIP-712 signature from your wallet

## Market Resolution & Settlement

- Markets resolve to $1.00 for the winning outcome
- Resolution is handled by Polymarket via UMA oracle
- After resolution, winning tokens can be redeemed for USDC

## API Endpoints

| API | Base URL | Purpose |
|-----|----------|---------|
| Gamma API | `https://gamma-api.polymarket.com` | Market discovery & metadata |
| Data API | `https://data-api.polymarket.com` | Trades, positions, history |
| CLOB API | `https://clob.polymarket.com` | Order placement & management |

## Popular Market Tags

- **Politics**: `election 2024`, `donald trump`, `kamala harris`
- **Crypto**: `bitcoin`, `ethereum`, `stablecoins`
- **Sports**: `EPL`, `NCAA`, `Wimbledon`
- **Economics**: `stock market`, `recession`, `gdp`

## Geographic Restrictions

Polymarket has geographic restrictions. Users from certain regions cannot place trades. See [Polymarket Docs](https://docs.polymarket.com/developers/CLOB/geoblock) for details.

## Resources

- [Polymarket Documentation](https://docs.polymarket.com)
- [CLOB Client (TypeScript)](https://github.com/Polymarket/clob-client)
- [CLOB Client (Python)](https://github.com/Polymarket/py-clob-client)
