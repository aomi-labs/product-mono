# Foundry MCP Server

## Requirements

- [X] Uses `cast` to make RPC calls to 127.0.0.1:8545
- [X] Exposes RPC calls as MCP tools
- [X] Runs a streamable http server on 3000

## Abstractions

### CastTool

Gets ETH balances, executes contract calls

- [x] Contains a `Cast<P>`
- [x] Implements `rmcp::ServerHandler`
- [X] Defined tools corresponding to RPC functions
    - [x] balance
    - [x] send
    - [x] call (takes in calldata)
- [x] Builds & executes transactions (with Anvil, doesnt not support arbitrary keys or real network)

## Thinking

Would be nice to enable arbitrary contract calldata construction from downloaded contract bytecode

-> `code` tool call (CastTool)
-> some decode tool call to get ABI from bytecode
-> some tool that generates calldata given function signature and args
-> `call` or `send` tool call with calldata (CastTool)

(turns out this is pretty complicated)

## BONUS

- [X] Brave search
- [X] 0x
- [ ] Defi Llama
