# Agent Crate

Core agent logic and tool integration

## Project Structure

```
src/
├── lib.rs              # Public API and re-exports
├── agent.rs            # Core agent logic and message handling
├── abi_encoder.rs      # Ethereum ABI encoding utilities
├── accounts.rs         # Account context generation
├── docs.rs             # Uniswap documentation RAG system
├── helpers.rs          # Streaming utilities and multi-turn prompts
└── time.rs             # Current timestamp tool
```

## Usage

### Prerequisites
1. Set environment variable: `export ANTHROPIC_API_KEY="your-key"`
2. Start MCP server: `cargo run -p mcp-server`
3. Optional: Start Anvil for local testing: `anvil`

Start

```bash
# Without ingesting uniswap docs (since it's slow)
cargo run -- --no-docs

# With documents
cargo run
```
