#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/create-api-key.sh --chatbots <list> [--label <label>] [--key <key>]

Options:
  --chatbots   Comma-separated chatbots (e.g. "default,l2beat") [required]
  --label      Optional label for the key
  --key        Provide your own API key (otherwise a random key is generated)
  -h, --help   Show this help message

Environment:
  DATABASE_URL optional (defaults to postgres://$USER@localhost:5432/chatbot)
  PSQL_BIN     optional (defaults to psql)
  PGHOST/PGPORT/PGUSER/PGDATABASE override connection defaults
EOF
}

CHATBOTS=""
LABEL=""
API_KEY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --chatbots)
      CHATBOTS="${2:-}"
      shift 2
      ;;
    --label)
      LABEL="${2:-}"
      shift 2
      ;;
    --key)
      API_KEY="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$CHATBOTS" ]]; then
  echo "Error: --chatbots is required" >&2
  usage >&2
  exit 1
fi

generate_key() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
    return
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - <<'PY'
import secrets
print(secrets.token_hex(32))
PY
    return
  fi
  if command -v uuidgen >/dev/null 2>&1; then
    uuidgen | tr -d '-' | tr '[:upper:]' '[:lower:]'
    return
  fi
  echo "Error: cannot generate a key (need openssl, python3, or uuidgen)" >&2
  exit 1
}

if [[ -z "$API_KEY" ]]; then
  API_KEY="$(generate_key)"
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "Error: python3 is required to build JSON chatbots list" >&2
  exit 1
fi

CHATBOTS_JSON="$(
  python3 - <<'PY' "$CHATBOTS"
import json
import sys

raw = sys.argv[1]
values = [v.strip() for v in raw.split(",") if v.strip()]
if not values:
    raise SystemExit("No chatbots provided after parsing")
print(json.dumps(values))
PY
)"

sql_literal() {
  python3 - <<'PY' "$1"
import sys
value = sys.argv[1]
escaped = value.replace("'", "''")
print("'" + escaped + "'")
PY
}

PGUSER_DEFAULT="${USER:-postgres}"
PGHOST_DEFAULT="localhost"
PGPORT_DEFAULT="5432"
PGDATABASE_DEFAULT="chatbot"
DATABASE_URL="${DATABASE_URL:-postgres://${PGUSER_DEFAULT}@${PGHOST_DEFAULT}:${PGPORT_DEFAULT}/${PGDATABASE_DEFAULT}}"
PSQL_BIN="${PSQL_BIN:-psql}"

echo "Testing database connection..."
set +e
$PSQL_BIN --dbname="$DATABASE_URL" -c '\q' >/dev/null 2>&1
DB_CONN_EXIT=$?
set -e
if [[ $DB_CONN_EXIT -ne 0 ]]; then
  echo "Error: Cannot connect to database: $DATABASE_URL" >&2
  exit 1
fi

API_KEY_SQL="$(sql_literal "$API_KEY")"
if [[ -n "$LABEL" ]]; then
  LABEL_SQL="$(sql_literal "$LABEL")"
else
  LABEL_SQL="NULL"
fi
CHATBOTS_SQL="$(sql_literal "$CHATBOTS_JSON")"

SQL="INSERT INTO api_keys (api_key, label, allowed_chatbots)
VALUES (${API_KEY_SQL}, ${LABEL_SQL}, ${CHATBOTS_SQL}::jsonb)
RETURNING api_key, label, allowed_chatbots, is_active;"

echo "Inserting API key..."
$PSQL_BIN --dbname="$DATABASE_URL" -v ON_ERROR_STOP=1 -P pager=off -c "$SQL"

echo ""
echo "API key created:"
echo "  Key: $API_KEY"
echo "  Chatbots: $CHATBOTS_JSON"
