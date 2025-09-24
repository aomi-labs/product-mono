#!/usr/bin/env sh
set -e

if [ "$#" -gt 0 ]; then
  exec /usr/local/bin/aomi-mcp-server "$@"
fi

if [ -n "${MCP_NETWORK_URLS_JSON:-}" ]; then
  exec /usr/local/bin/aomi-mcp-server "${MCP_NETWORK_URLS_JSON}"
fi

exec /usr/local/bin/aomi-mcp-server
