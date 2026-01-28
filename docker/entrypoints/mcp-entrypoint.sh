#!/usr/bin/env sh
set -e

if [ "$#" -gt 0 ]; then
  exec /usr/local/bin/aomi-mcp-server "$@"
fi

exec /usr/local/bin/aomi-mcp-server
