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

## Dependencies

- Foundry v1.1.0
- Alloy v1.1.0
- RMCP v0.1.5

## License

MIT