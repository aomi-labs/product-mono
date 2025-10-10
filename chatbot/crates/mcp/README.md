# MCP Server

Model Context Protocol server exposing Foundry Cast tools and web search.

## Usage

```bash
cargo run -p aomi-mcp-server
```

Server runs on `127.0.0.1:3000`.

## Environment Variables

```bash
# Optional: Enable Brave Search
BRAVE_SEARCH_API_KEY=your_brave_api_key_here

# Optional: Enable Etherscan
ETHERSCAN_API_KEY=your_etherscan_api_key_here

# Optional: Enable 0x API
ZEROX_API_KEY=your_0x_api_key_here
```

## Exposed Tools

### Ethereum Tools
- Balance queries
- Contract calls
- Transaction sending
- Contract code retrieval
- Transaction details lookup
- Block information (including current block height)

### Web Search
- Brave Search API (requires API key)

### Contract Tools
- Get verified contract ABIs from Etherscan (requires API key)
- Supports multiple networks: mainnet, goerli, sepolia, polygon, arbitrum, optimism, base
