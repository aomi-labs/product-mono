# X (Twitter) Integration

This module provides an AI assistant specialized in X (formerly Twitter) data analysis using TwitterAPI.io.

## Overview

The X app allows users to:
- Search posts by keywords, hashtags, or users
- Get user profiles and their recent posts
- Discover trending topics
- Analyze post engagement metrics

## Available Tools

| Tool | Description |
|------|-------------|
| `get_x_user` | Get user profile by username |
| `get_x_user_posts` | Get recent posts from a user |
| `search_x` | Advanced search with operators |
| `get_x_trends` | Get trending topics |
| `get_x_post` | Get post details by ID |

## Configuration

### API Key Required

This module uses [TwitterAPI.io](https://twitterapi.io) for data access. Set your API key via environment variable:

```bash
X_API_KEY=your_api_key_here
```

Or in `providers.toml`:
```toml
[x]
api_key = "your_api_key_here"
```

## Search Operators

The `search_x` tool supports X's advanced search operators:

| Operator | Example | Description |
|----------|---------|-------------|
| `from:` | `from:elonmusk` | Posts from specific user |
| `to:` | `to:elonmusk` | Replies to specific user |
| `#` | `#crypto` | Posts with hashtag |
| `@` | `@elonmusk` | Posts mentioning user |
| `lang:` | `lang:en` | Filter by language |
| `since:` | `since:2026-01-01` | Posts after date |
| `until:` | `until:2026-02-01` | Posts before date |
| `min_faves:` | `min_faves:100` | Minimum likes |
| `min_retweets:` | `min_retweets:50` | Minimum reposts |
| `-` | `-from:bot` | Exclude results |

## Example Queries

```
# Find crypto discussions from a user
from:elonmusk crypto

# Find trending AI posts with high engagement
AI min_faves:1000 lang:en

# Find recent posts about a topic
#DeFi since:2026-01-01
```

## Rate Limits

TwitterAPI.io supports up to 200 QPS. The free tier includes 10,000 credits.

**Credit costs:**
- Tweets: $0.15 per 1,000
- User profiles: $0.18 per 1,000
- Followers: $0.15 per 1,000

## Resources

- [TwitterAPI.io Documentation](https://docs.twitterapi.io)
- [X Advanced Search](https://twitter.com/search-advanced)
