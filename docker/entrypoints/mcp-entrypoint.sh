#!/usr/bin/env sh
set -e

if [ "$#" -gt 0 ]; then
  exec /usr/local/bin/aomi-mcp-server "$@"
fi

CONFIG_HELPER=${MCP_CONFIG_HELPER:-/app/scripts2/configure.py}
MCP_ENVIRONMENT=${MCP_ENV:-prod}

if [ -z "${MCP_NETWORK_URLS_JSON:-}" ]; then
  if [ -x "$CONFIG_HELPER" ]; then
    python3 "$CONFIG_HELPER" "$MCP_ENVIRONMENT" --check-keys
    MCP_NETWORK_URLS_JSON=$(python3 "$CONFIG_HELPER" "$MCP_ENVIRONMENT" --chain-json)
  else
    echo "⚠️  MCP config helper not found at $CONFIG_HELPER; falling back to default testnet"
    MCP_NETWORK_URLS_JSON='{"testnet":"http://anvil:8545"}'
  fi
fi

echo "Running mcp server with MCP_NETWORK_URLS_JSON:\n🔥 $MCP_NETWORK_URLS_JSON"
exec /usr/local/bin/aomi-mcp-server "${MCP_NETWORK_URLS_JSON}"
