# foameow ≽^•⩊•^≼

agentic evm oPURRator (sorry)

## Notes

- There are a couple of regular Rig tools for encoding function calls and getting the current unix time.
- Provided foameow generalized contract tools so that, in conjunction with Brave Search and Etherscan, it can look up ABIs and generate calldata for any contract it can find an ABI for.
- 0x: I disabled the quote tool because I don't think they allow Anvil addresses to act as taker. Price works, but still sometimes errors over the taker thing. Quote tool is way cooler though since it gives you the exact calldata to execute.
- DefiLlama: Opted to not implement for now
- Anvil: Wasted a lot of time not noticing that ETH transfers failed because the addresses are honeypots in mainnet that drain ETH into another address. Solved this by providing an explicit height in the anvil rpc url.
- TUI: Hope you don't mind it's not strictly a REPL. I preferred this ux.
- Uniswap Documents:
    - At first I was going to use a Qdrant store but opted to keep it simple since this isn't for production. It loads in-memory at startup. This can take some time, so use `--no-docs` if you want to start it without chunking/indexing the docs. Also I selected just a few docs for an example because the loading time gets ridiculous with the current approach as you add docs, and I didn't want it to slow down startup more.
    - I wanted to actually allow the agent to go and fetch documents from the internet to store and RAG but decided to keep it simple because... time.
- Future features:
    - Healthchecks and more robust retries on tui -> mcp connection and mcp -> anvil connection
    - Metrics
    - Actual config
    - More visual information like MCP connection status
    - Conversation history
    - More tools ofc

## Project Structure

```
foameow/
├── bin/
│   └── tui/        # Chat-style terminal UI (main binary)
├── crates/
│   ├── agent/      # Core rig agent
│   ├── mcp-server/ # MCP server with Cast tools & external APIs
│   └── rag/        # Vector embeddings and storage for Uniswap docs
├── documents/      # Uniswap V2/V3 documentation & contracts
```

## Quick Start

**Prerequisites:** Rust, Node.js, Foundry

**Simple Setup:**
```bash
# Development
cp .env.template .env.dev
# Edit .env.dev with your API keys
./scripts/dev.sh

# Production  
cp .env.template .env.prod
# Edit .env.prod with your API keys
./scripts/prod.sh
```

**Manual Setup (if preferred):**

1. **Setup environment:**
   ```bash
   cp .env.template .env.dev
   # Edit .env.dev with your API keys
   ```

2. **Start Anvil:**
   ```bash
   anvil --fork-url https://eth-mainnet.public.blastapi.io@22419684
   ```

3. **Start MCP Server:**
   ```bash
   cargo run -p mcp-server
   ```

4. **Start Backend:**
   ```bash
   cargo run -p backend -- --no-docs
   ```

5. **Start Frontend:**
   ```bash
   cd ../aomi-landing && npm run dev
   ```

## Quick Test

Verify the implementation with these assessment examples:

```
> send 1 ETH from Alice to Bob
> How much USDC does Alice have?
> Is Uniswap V2 Router (0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D) deployed?
> How do I calculate slippage for Uniswap V3?
> What's the difference between exactInput and exactOutput?
> Show me the SwapRouter contract interface
```
