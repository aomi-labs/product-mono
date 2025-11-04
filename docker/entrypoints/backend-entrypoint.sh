#!/usr/bin/env sh
set -e

skip_docs="${BACKEND_SKIP_DOCS:-true}"
case "$skip_docs" in
  1|true|TRUE|yes|on)
    set -- --no-docs "$@"
    ;;
esac

skip_mcp="${BACKEND_SKIP_MCP:-false}"
case "$skip_mcp" in
  1|true|TRUE|yes|on)
    set -- --skip-mcp "$@"
    ;;
esac

CONFIG_HELPER=${MCP_CONFIG_HELPER:-/app/scripts/configure.py}
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

export MCP_NETWORK_URLS_JSON
echo "🚀 Backend starting with MCP_NETWORK_URLS_JSON: $MCP_NETWORK_URLS_JSON"

exec /usr/local/bin/backend "$@"
