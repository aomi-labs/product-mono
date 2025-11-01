# Agent Crate

Core AI agent implementation with Claude integration for natural language blockchain interactions.

## Project Structure

```
src/
├── lib.rs              # Public API and re-exports
├── agent.rs            # Core Claude agent and conversation management
├── abi_encoder.rs      # Ethereum ABI encoding/decoding utilities
├── accounts.rs         # Wallet account context and address management
├── docs.rs             # Uniswap documentation RAG system
├── helpers.rs          # SSE streaming and multi-turn conversation helpers
├── wallet.rs           # Wallet transaction handling and state management
└── time.rs             # Current timestamp tool for agent context
```

## Features

- **Claude 3 Integration**: Powered by Anthropic's Claude API for natural language understanding
- **Multi-turn Conversations**: Maintains conversation history and context
- **Tool Orchestration**: Coordinates blockchain operations through MCP server
- **Document RAG**: Semantic search over Uniswap protocol documentation
- **Wallet Management**: Handles wallet connections and transaction flows
- **Streaming Responses**: Server-Sent Events for real-time updates

## Usage

### Prerequisites
1. Set environment variable: `export ANTHROPIC_API_KEY="sk-ant-api03-..."`
2. Start MCP server: `cargo run -p aomi-mcp`
3. Optional: Start Anvil for local testing: `anvil --fork-url $ETH_RPC_URL`

### Running the Agent

```bash
# Development mode (skip docs for faster startup)
cargo run -p backend -- --no-docs

# Production mode (with full documentation)
cargo run -p backend --release
```

## Configuration

The agent uses environment variables and `config.yaml` for configuration:

- `ANTHROPIC_API_KEY`: Required for Claude API access
- `MCP_SERVER_HOST`: MCP server hostname (default: 127.0.0.1)
- `MCP_SERVER_PORT`: MCP server port (default: 5000)
