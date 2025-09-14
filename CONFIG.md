# Configuration Guide

This document explains how to configure the forge-mcp project with proper port management and API keys.

## Quick Start

1. **Setup environment file:**

    If they're already avaliable in your local bash setup, you can skip this step.
   ```bash
   cp .env.example .env
   ```
    Edit your API keys:
   ```bash
   nano .env
   # Add your ANTHROPIC_API_KEY (required)
   # Add optional API keys for additional features
   ```

2. **Run the application:**
   ```bash
   ./script/run-all.sh
   ```

## Configuration System

The project uses a layered configuration approach:

1. **Environment variables** (highest priority)
2. **`.env` file** (loaded if env vars not set)
3. **`config.yaml`** (port and service configuration)
4. **Hardcoded defaults** (fallback)

## File Structure

```
forge-mcp/
├── config.yaml           # Service ports and configuration
├── .env.example          # Environment variables template  
├── .env                  # Your actual environment variables (create this)
├── scripts/
│   └── load-config.sh    # Configuration loader script
└── aomi-landing/
    └── .env.example      # Frontend environment variables template
```

## API Keys Configuration

### Required API Keys

| Variable | Required | Description | Get Key From |
|----------|----------|-------------|--------------|
| `ANTHROPIC_API_KEY` | ✅ Yes | Claude API access | [console.anthropic.com](https://console.anthropic.com/) |

### Optional API Keys

| Variable | Required | Description | Get Key From |
|----------|----------|-------------|--------------|
| `BRAVE_SEARCH_API_KEY` | ❌ No | Web search capabilities | [api.search.brave.com](https://api.search.brave.com/) |
| `ETHERSCAN_API_KEY` | ❌ No | Blockchain data & contract ABIs | [etherscan.io/apis](https://etherscan.io/apis) |
| `ZEROX_API_KEY` | ❌ No | Token swap functionality | [dashboard.0x.org](https://dashboard.0x.org/) |

### Example .env file:

```bash
# Required
ANTHROPIC_API_KEY=sk-ant-api03-your-api-key-here

# Optional
BRAVE_SEARCH_API_KEY=your_brave_api_key_here
ETHERSCAN_API_KEY=your_etherscan_api_key_here
ZEROX_API_KEY=your_0x_api_key_here
```

## Port Configuration

Default ports are defined in `config.yaml`:

| Service | Default Port | Environment Override |
|---------|-------------|---------------------|
| MCP Server | 5000 | `MCP_SERVER_PORT` |
| Backend API | 8080 | `BACKEND_PORT` |
| Frontend | 3000 | `FRONTEND_PORT` |
| Anvil (Ethereum) | 8545 | `ANVIL_PORT` |

### Custom Port Example:

```bash
# Override ports via environment variables
export MCP_SERVER_PORT=5001
export BACKEND_PORT=8081
export FRONTEND_PORT=3001
./test-chat-html2.sh
```

## Configuration Files

### config.yaml

Main configuration file for services, ports, and development settings:

```yaml
services:
  mcp_server:
    host: "127.0.0.1"
    port: 5000
  backend:
    host: "0.0.0.0" 
    port: 8080
  frontend:
    port: 3000
  anvil:
    host: "127.0.0.1"
    port: 8545

connection:
  max_reconnect_attempts: 5
  reconnect_delay_ms: 3000
  sse_interval_ms: 500
```

### Frontend Configuration

The frontend (`aomi-landing`) uses Vite environment variables:

```bash
# aomi-landing/.env
VITE_BACKEND_URL=http://localhost:8080
VITE_MAX_MESSAGE_LENGTH=2000
VITE_RECONNECT_ATTEMPTS=5
VITE_RECONNECT_DELAY=3000
```

## Environment Loading Priority

1. **System Environment Variables** - Highest priority
2. **`.env` file** - Loaded automatically if env vars not set
3. **Config defaults** - From `config.yaml`
4. **Hardcoded fallbacks** - Built into code

## Troubleshooting

### Missing API Key Error

```bash
❌ ERROR: ANTHROPIC_API_KEY environment variable is not set
```

**Solutions:**
1. Set environment variable: `export ANTHROPIC_API_KEY="your-key"`
2. Add to `.env` file: `ANTHROPIC_API_KEY=your-key`
3. Get key from: https://console.anthropic.com/

### Port Already in Use

```bash
❌ MCP server failed to start within 20 seconds
```

**Solutions:**
1. Kill existing processes: `lsof -ti:5000 | xargs kill -9`
2. Use different ports: `export MCP_SERVER_PORT=5001`
3. Check what's using the port: `lsof -i :5000`

### Configuration Not Loading

The `scripts/load-config.sh` script provides detailed output:

```bash
🔧 Loading forge-mcp configuration...
✅ Configuration loaded:
   MCP Server: http://127.0.0.1:5000
   Backend:    http://localhost:8080
   Frontend:   http://localhost:3000
   Anvil:      http://127.0.0.1:8545
🔍 Checking API keys...
   ✅ ANTHROPIC_API_KEY: Anthropic Claude API access
```

## Development

### Testing Configuration

Test your configuration without starting services:

```bash
source scripts/load-config.sh
echo "MCP Server: $MCP_SERVER_URL"
echo "Backend: $BACKEND_URL"
```

### Adding New Configuration

1. Add to `config.yaml` for static configuration
2. Add to `.env.example` for environment variables
3. Update `scripts/load-config.sh` to load the new values
4. Update this documentation

## Security Notes

- Never commit `.env` files to version control
- API keys should only be in `.env` files or environment variables
- Use `.env.example` as a template for sharing configuration structure
- The `.env` file is included in `.gitignore` by default