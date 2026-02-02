#!/usr/bin/env bash
set -euo pipefail

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_cmd psql
require_cmd jq

if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required for this smoke test." >&2
  exit 1
fi

if [[ "${ADMIN_CLI_SMOKE_OK:-}" != "1" ]]; then
  echo "Refusing to run without ADMIN_CLI_SMOKE_OK=1 (prevents accidental use on prod)." >&2
  exit 1
fi

cleanup() {
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<SQL >/dev/null
DELETE FROM contracts WHERE address = '${contract_address}' AND chain_id = ${contract_chain_id};
DELETE FROM sessions WHERE id = '${session_id}';
DELETE FROM users WHERE public_key = '${user_public_key}';
DELETE FROM api_keys WHERE api_key = '${api_key}';
SQL
}
trap cleanup EXIT

api_key="cli_smoke_key_$(date +%s)"
user_public_key="cli_smoke_user_$(date +%s)"
session_id="cli_smoke_session_$(date +%s)"
contract_address="0xabc0000000000000000000000000000000000000"
contract_chain_id=1

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<SQL >/dev/null
INSERT INTO users (public_key, username) VALUES ('${user_public_key}', 'alice');
INSERT INTO sessions (id, public_key, started_at, last_active_at, title)
VALUES ('${session_id}', '${user_public_key}', 1700000000, 1700000000, 'hello');
INSERT INTO contracts (address, chain, chain_id, source_code, abi, name, symbol, protocol, contract_type, version, is_proxy)
VALUES ('${contract_address}', 'ethereum', ${contract_chain_id}, 'contract X {}', '[{"type":"function","name":"foo"}]', 'ContractX', 'CX', 'proto', 'typeA', 'v1', false);
SQL

run_cli() {
  ${ADMIN_CLI_BIN:-admin-cli} -d "$DATABASE_URL" "$@"
}

assert_sql() {
  local expected="$1"
  local query="$2"
  local actual
  actual=$(psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -t -A -c "$query")
  if [[ "$actual" != "$expected" ]]; then
    echo "Assertion failed: expected '${expected}', got '${actual}'" >&2
    echo "Query: ${query}" >&2
    exit 1
  fi
}

echo "API keys"
run_cli api-keys create -n default -l test -k "${api_key}" | tee /dev/stderr | jq -e ".api_key == \"${api_key}\"" >/dev/null
assert_sql "1" "SELECT COUNT(*) FROM api_keys WHERE api_key = '${api_key}';"
run_cli api-keys list | tee /dev/stderr | jq -e 'length >= 1' >/dev/null
run_cli api-keys update -k "${api_key}" -a | tee /dev/stderr | jq -e '.is_active == true' >/dev/null
assert_sql "t" "SELECT is_active FROM api_keys WHERE api_key = '${api_key}';"

echo "Users"
run_cli users list | tee /dev/stderr | jq -e "map(select(.public_key == \"${user_public_key}\")) | length == 1" >/dev/null
run_cli users update -p "${user_public_key}" -u bob | tee /dev/stderr | jq -e '.username == "bob"' >/dev/null
assert_sql "bob" "SELECT username FROM users WHERE public_key = '${user_public_key}';"
run_cli users delete -p "${user_public_key}" | tee /dev/stderr | jq -e '.deleted == 1' >/dev/null
assert_sql "0" "SELECT COUNT(*) FROM users WHERE public_key = '${user_public_key}';"

echo "Sessions"
run_cli sessions list | tee /dev/stderr | jq -e "map(select(.id == \"${session_id}\")) | length == 1" >/dev/null
run_cli sessions update -i "${session_id}" -t "new title" | tee /dev/stderr | jq -e '.title == "new title"' >/dev/null
assert_sql "new title" "SELECT title FROM sessions WHERE id = '${session_id}';"
run_cli sessions delete -i "${session_id}" | tee /dev/stderr | jq -e '.deleted == 1' >/dev/null
assert_sql "0" "SELECT COUNT(*) FROM sessions WHERE id = '${session_id}';"

echo "Contracts"
run_cli contracts list | tee /dev/stderr | jq -e "map(select(.address == \"${contract_address}\")) | length == 1" >/dev/null
run_cli contracts update -c ${contract_chain_id} -a ${contract_address} -n "ContractY" | tee /dev/stderr | jq -e '.name == "ContractY"' >/dev/null
assert_sql "ContractY" "SELECT name FROM contracts WHERE chain_id = ${contract_chain_id} AND address = '${contract_address}';"
run_cli contracts delete -c ${contract_chain_id} -a ${contract_address} | tee /dev/stderr | jq -e '.deleted == 1' >/dev/null
assert_sql "0" "SELECT COUNT(*) FROM contracts WHERE chain_id = ${contract_chain_id} AND address = '${contract_address}';"

echo "admin-cli smoke test OK"
