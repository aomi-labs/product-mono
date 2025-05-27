# forge-mcp

A Model Context Protocol (MCP) implementation for Foundry's Cast tool, enabling seamless integration of Cast functionality into AI-powered development environments.

## Overview

This project extends Foundry's Cast tool with MCP support, allowing AI assistants to interact with Ethereum networks through a standardized protocol. It provides a bridge between AI development environments and Ethereum tooling, making it easier to perform blockchain operations programmatically.

## Features

- MCP-compliant Cast service implementation
- Async provider initialization for Ethereum network interaction
- Standard I/O transport layer for seamless integration
- Comprehensive error handling and logging

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
cargo run --bin cast-server
```

## MCP Configuration

To use this service with Claude Desktop, add the following JSON configuration:

```json
{
  "name": "cast-server",
  "description": "A Model Context Protocol (MCP) implementation for Foundry's Cast tool, enabling AI assistants to interact with Ethereum networks",
  "version": "1.0.0",
  "type": "mcp",
  "transport": {
    "type": "stdio",
    "protocol": "json"
  },
  "commands": {
    "cast": {
      "description": "Execute Cast commands for Ethereum network interaction",
      "usage": "cast [command] [args...]",
      "examples": [
        "cast balance <address>",
        "cast send <to> <value>",
        "cast call <address> <function> [args...]"
      ]
    }
  },
  "dependencies": {
    "foundry": "1.1.0",
    "alloy": "1.1.0",
    "rmcp": "0.1.5"
  },
  "installation": {
    "type": "cargo",
    "command": "cargo build"
  },
  "startup": {
    "command": "cargo run --bin cast-server"
  }
}
```

## Dependencies

- Foundry v1.1.0
- Alloy v1.1.0
- RMCP v0.1.5

## License

MIT