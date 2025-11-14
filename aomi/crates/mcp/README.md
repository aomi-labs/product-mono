# MCP Server

Model Context Protocol (MCP) server providing blockchain tools and external API integrations for the AI agent.

## Overview

The MCP server acts as a bridge between the AI agent and blockchain networks, exposing various tools through a standardized protocol. It integrates Foundry Cast commands, external APIs, and custom blockchain utilities.

## Usage

```bash
# Development
cargo run -p aomi-mcp

# Production (with network configuration)
cargo run -p aomi-mcp --release -- '{"testnet":"http://localhost:8545","ethereum":"https://eth-mainnet.g.alchemy.com/v2/KEY"}'
```

Default port: `127.0.0.1:5000` (development) or `0.0.0.0:5001` (production)

## Environment Variables

```bash
# Required for enhanced features
BRAVE_SEARCH_API_KEY=your_brave_api_key_here     # Web search capabilities
ETHERSCAN_API_KEY=your_etherscan_api_key_here    # Contract ABI retrieval
ALCHEMY_API_KEY=your_alchemy_api_key_here        # Premium RPC endpoints
ZEROX_API_KEY=your_0x_api_key_here               # Token swap functionality
```

## Exposed Tools

### Blockchain Operations (via Cast)
- **Balance Queries**: Get ETH/token balances for any address
- **Contract Interactions**: Call read/write functions on smart contracts
- **Transaction Management**: Send transactions, check status, get receipts
- **Block Information**: Current block height, timestamp, gas prices
- **ENS Resolution**: Resolve ENS names to addresses and vice versa
- **Contract Code**: Retrieve bytecode and storage values

### External API Integrations
- **Brave Search**: Web search for real-time blockchain information
- **Etherscan**: Verified contract ABIs and source code
- **0x Protocol**: Token swap quotes and optimal routing
- **ZeroxDocumentParser**: Parse and extract data from documents

### Supported Networks
- Ethereum
- Local Testnet (Anvil)
- Polygon
- Arbitrum
- Base
- Custom RPC endpoints (configurable)

## Architecture

```
MCP Server
├── brave_search.rs    # Web search integration
├── cast.rs           # Foundry Cast tool wrapper
├── combined_tool.rs  # Tool orchestration and routing
├── etherscan.rs      # Contract ABI retrieval
├── zerox.rs          # 0x Protocol integration
└── main.rs           # Server initialization and RPC handling
```

## Tool Response Format

All tools return structured JSON responses compatible with the MCP protocol:

```json
{
  "tool": "cast_call",
  "result": {
    "output": "0x...",
    "decoded": "1000000000000000000"
  }
}
