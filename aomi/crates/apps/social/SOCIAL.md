# Social Jam - Cross-Platform Crypto Social Intelligence

This module provides an AI assistant specialized in aggregating social signals across multiple platforms for crypto/DeFi research.

## Overview

Social Jam combines data from:
- **X (Twitter)** - Via TwitterAPI.io (existing integration)
- **Farcaster** - Via Neynar API (Web3-native social)
- **LunarCrush** - Aggregated sentiment from X, Reddit, YouTube, TikTok, and news

## Available Tools

### X (Twitter) Tools
| Tool | Description |
|------|-------------|
| `get_x_user` | Get user profile by username |
| `get_x_user_posts` | Get recent posts from a user |
| `search_x` | Advanced search with operators |
| `get_x_trends` | Get trending topics |
| `get_x_post` | Get post details by ID |

### Farcaster Tools
| Tool | Description |
|------|-------------|
| `search_farcaster` | Search casts (posts) on Farcaster |
| `get_farcaster_user` | Get user profile by username or FID |
| `get_farcaster_channel` | Get channel info and recent casts |
| `get_farcaster_trending` | Get trending Farcaster channels |

### LunarCrush Sentiment Tools
| Tool | Description |
|------|-------------|
| `get_crypto_sentiment` | Get sentiment data for a crypto topic |
| `get_trending_topics` | Get trending social topics across platforms |
| `get_topic_summary` | AI-generated summary of what's happening |

## Configuration

### Required API Keys

```bash
# X (Twitter) - Required for X tools
X_API_KEY=your_twitterapi_io_key

# Farcaster (Neynar) - Required for Farcaster tools
NEYNAR_API_KEY=your_neynar_key

# LunarCrush - Required for sentiment tools
LUNARCRUSH_API_KEY=your_lunarcrush_key
```

## Use Cases

### Crypto Sentiment Analysis
```
"What's the sentiment around $ETH right now?"
→ Uses get_crypto_sentiment for LunarCrush data
→ Uses search_x for recent X posts
→ Uses search_farcaster for Farcaster discussions
```

### Influencer Tracking
```
"What's vitalik.eth been posting about?"
→ get_farcaster_user + get_x_user
→ Cross-references activity
```

### Community Signals
```
"What's trending in the /base channel?"
→ get_farcaster_channel for Farcaster
→ search_x for #Base discussions
```

### Alpha Discovery
```
"What crypto topics are gaining attention?"
→ get_trending_topics for LunarCrush trends
→ get_farcaster_trending for Farcaster channels
```

## Platform Notes

### Farcaster
- Web3-native social protocol (on-chain identities)
- Strong crypto/DeFi community presence
- Channels like /degen, /base, /crypto are highly active
- Users often have ENS names or verified wallet addresses

### LunarCrush
- Aggregates from X, Reddit, YouTube, TikTok, and news
- Provides Galaxy Score™ and AltRank™ metrics
- Sentiment classified as positive/neutral/negative
- Social dominance shows relative attention share

## Rate Limits

| Platform | Limit |
|----------|-------|
| TwitterAPI.io | 200 QPS (credit-based) |
| Neynar | Credit-based (varies by endpoint) |
| LunarCrush | Plan-based (free tier available) |

## Resources

- [TwitterAPI.io Docs](https://docs.twitterapi.io)
- [Neynar Docs](https://docs.neynar.com)
- [LunarCrush API](https://github.com/lunarcrush/api)
- [Farcaster Protocol](https://docs.farcaster.xyz)
