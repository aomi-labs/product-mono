# forge-mcp

A Model Context Protocol (MCP) implementation for Foundry's Cast tool, enabling seamless integration of Cast functionality into AI-powered development environments.

## Overview

This project extends Foundry's Cast tool with MCP support, allowing AI assistants to interact with Ethereum networks through a standardized protocol. It provides a bridge between AI development environments and Ethereum tooling, making it easier to perform blockchain operations programmatically.

## Features
Currently we starts with Cast as an entrypoint to break down foundry.

Based on the complete list of Cast commands, I'll categorize and select the most useful ones for AI agents to read onchain data, organized by functionality:

1. **Block and Transaction Data**:
   - `block` - Get block information
   - `block-number` - Get latest block number
   - `tx` - Get transaction information
   - `receipt` - Get transaction receipt
   - `age` - Get block timestamp
   - `base-fee` - Get block base fee
   - `gas-price` - Get current gas price

2. **Contract State Reading**:
   - `call` - Read contract state without publishing transaction
   - `storage` - Get raw value of contract's storage slot
   - `code` - Get contract's runtime bytecode
   - `codesize` - Get contract's bytecode size
   - `codehash` - Get contract's codehash
   - `implementation` - Get EIP-1967 implementation address

3. **Account Information**:
   - `balance` - Get account balance
   - `nonce` - Get account nonce
   - `storage-root` - Get account's storage root
   - `proof` - Generate storage proof for a slot

4. **Event and Log Reading**:
   - `logs` - Get logs by signature or topic
   - `decode-event` - Decode event data
   - `4byte-event` - Get event signature from topic

5. **Data Decoding and Formatting**:
   - `decode-abi` - Decode ABI-encoded data
   - `decode-calldata` - Decode input data
   - `decode-string` - Decode ABI-encoded string
   - `format-units` - Format numbers with decimals
   - `parse-units` - Convert decimal to smallest unit

6. **ENS and Address Resolution**:
   - `resolve-name` - ENS lookup
   - `lookup-address` - ENS reverse lookup
   - `namehash` - Calculate ENS namehash

7. **Chain Information**:
   - `chain` - Get current chain name
   - `chain-id` - Get Ethereum chain ID
   - `client` - Get client version

8. **Utility Functions**:
   - `from-wei` - Convert wei to ETH
   - `to-wei` - Convert ETH to wei
   - `to-check-sum-address` - Convert to checksummed address
   - `to-utf8` - Convert hex to UTF-8
   - `to-ascii` - Convert hex to ASCII

These commands provide a comprehensive toolkit for AI agents to:
1. Read blockchain state
2. Monitor transactions
3. Track events
4. Decode contract data
5. Handle different data formats
6. Resolve addresses and names
7. Get chain information
8. Perform unit conversions


## Setup

1. Ensure you have Rust and Cargo installed
2. Clone this repository
3. Install dependencies:
   ```bash
   cargo build
   ```

## Usage

The service can be started using:
```bash
cargo run --release --bin cast-server
```

## MCP Configuration

To use this service with Claude Desktop, add the following JSON configuration:

```json
{
    "mcpServers": {
      "cast-server": {
        "command": "/path/to/forge-mcp/target/release/cast-server",
        "args": [
          "--verbose"
        ]
      }
    }
  }
```

## Dependencies

- Foundry v1.1.0
- Alloy v1.1.0
- RMCP v0.1.5

## License

MIT