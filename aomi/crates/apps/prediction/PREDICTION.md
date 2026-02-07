# Prediction Wizard 🔮

Aggregated prediction market intelligence across multiple platforms.

## Overview

The Prediction Wizard provides unified access to the world's leading prediction markets:

| Platform | Type | Real Money | Best For |
|----------|------|------------|----------|
| **Polymarket** | Crypto (Polygon) | Yes (USDC) | Politics, Crypto, Events |
| **Kalshi** | CFTC-regulated | Yes (US only) | Economics, Weather, US Events |
| **Manifold** | Community | Play money* | Anything, Community Questions |
| **Metaculus** | Forecasting | No | Long-term, AI, Science |

*Manifold has some prize-eligible markets

## Available Tools

| Tool | Description |
|------|-------------|
| `search_prediction_markets` | Search across all platforms with unified results |
| `get_prediction_market_details` | Get full details for a specific market |
| `get_aggregated_odds` | Compare probabilities across platforms |
| `get_trending_predictions` | Discover hot/trending markets |

## Tool Schemas

### search_prediction_markets

```json
{
  "query": "Trump 2028",           // Required: search query
  "category": "politics",          // Optional: filter category
  "platforms": ["polymarket"],     // Optional: limit to specific platforms
  "limit": 5                       // Optional: max results per platform
}
```

### get_prediction_market_details

```json
{
  "market_id": "0x1234...",        // Required: market identifier
  "platform": "polymarket"         // Required: platform name
}
```

### get_aggregated_odds

```json
{
  "query": "Bitcoin 100k"          // Required: question to search
}
```

### get_trending_predictions

```json
{
  "category": "crypto",            // Optional: filter category
  "sort_by": "volume_24h",         // Optional: sort order
  "limit": 10                      // Optional: max results
}
```

## API Endpoints Used

### Polymarket
- `GET https://gamma-api.polymarket.com/events` - Market discovery
- `GET https://gamma-api.polymarket.com/markets/{id}` - Market details
- No authentication required for read operations

### Kalshi
- `GET https://api.elections.kalshi.com/trade-api/v2/markets` - List markets
- `GET https://api.elections.kalshi.com/trade-api/v2/markets/{ticker}` - Market details
- No authentication required for read operations

### Manifold
- `GET https://api.manifold.markets/v0/search-markets` - Search markets
- `GET https://api.manifold.markets/v0/market/{id}` - Market details
- No authentication required for read operations

### Metaculus
- `GET https://www.metaculus.com/api/questions/` - Search questions
- No authentication required for read operations

## Example Usage

### Finding odds for an event
```
User: What are the odds Trump wins in 2028?

Wizard: Let me check across prediction markets...

📊 **Aggregated Odds: Trump 2028 Presidential**

| Platform | Probability |
|----------|-------------|
| Polymarket | 42.3% |
| Kalshi | 39.5% |
| Manifold | 44.1% |

**Consensus:** 41.9%
**Spread:** 4.6% (normal range)

The markets suggest roughly 2-in-5 odds. Polymarket has the highest 
volume ($2.1M), making it the most reliable indicator.
```

### Comparing platforms
```
User: Is there arbitrage on the Bitcoin 100k market?

Wizard: Checking for price discrepancies...

🔍 **Bitcoin $100k by Dec 2026**

| Platform | YES Price | Volume |
|----------|-----------|--------|
| Polymarket | 67.2% | $4.5M |
| Kalshi | 64.8% | $890K |

**Spread:** 2.4%

No significant arbitrage opportunity. The 2.4% spread is within 
normal variance given transaction costs (~1% each way).
```

## Rate Limits

All APIs are free for read operations with reasonable rate limits:
- Polymarket: No strict limit, be reasonable
- Kalshi: ~100 requests/minute
- Manifold: ~500 requests/minute  
- Metaculus: ~60 requests/minute

## Future Enhancements

- [ ] Historical price data
- [ ] WebSocket real-time updates
- [ ] Arbitrage alert system
- [ ] Portfolio tracking across platforms
- [ ] Trading execution (with auth)
