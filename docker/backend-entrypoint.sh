#!/usr/bin/env sh
set -e

skip_docs="${BACKEND_SKIP_DOCS:-false}"
case "$skip_docs" in
  1|true|TRUE|yes|on)
    set -- --no-docs "$@"
    ;;
esac

exec /usr/local/bin/backend "$@"
