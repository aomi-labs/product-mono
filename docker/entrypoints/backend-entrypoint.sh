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

exec /usr/local/bin/backend "$@"
