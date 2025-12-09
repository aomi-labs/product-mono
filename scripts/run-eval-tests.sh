#!/usr/bin/env bash
# Bootstraps a local Anvil testnet and runs the eval test suite with Anthropic credentials

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT_DIR}/.env.dev"
ANVIL_BIN="${ANVIL_BIN:-anvil}"
ANVIL_HOST="${ANVIL_HOST:-127.0.0.1}"
ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_CHAIN_ID="${ANVIL_CHAIN_ID:-31337}"
CHAIN_CONFIG=${CHAIN_NETWORK_URLS_JSON:-"{\"ethereum\":\"http://${ANVIL_HOST}:${ANVIL_PORT}\"}"}
ANVIL_LOG="${ANVIL_LOG:-${ROOT_DIR}/logs/anvil-eval.log}"
OUTPUT_DIR="${ROOT_DIR}/output"
OUTPUT_FILE="${OUTPUT_DIR}/eval-results.md"
TMP_OUTPUT="$(mktemp)"
ALICE_ACCOUNT="${ALICE_ACCOUNT:-0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266}"
BOB_ACCOUNT="${BOB_ACCOUNT:-0x8D343ba80a4cD896e3e5ADFF32F9cF339A697b28}"
TEST_FILTER=""

if [[ $# -gt 0 ]]; then
  TEST_FILTER="$1"
fi

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "Expected ${ENV_FILE} with Anthropic credentials; copy .env.template -> .env.dev first." >&2
  exit 1
fi

if ! command -v "${ANVIL_BIN}" >/dev/null 2>&1; then
  echo "Could not find '${ANVIL_BIN}' on PATH. Install Foundry (https://foundry-rs.github.io/) first." >&2
  exit 1
fi

mkdir -p "$(dirname "${ANVIL_LOG}")"
mkdir -p "${OUTPUT_DIR}"

set -a
source "${ENV_FILE}"
set +a

if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
  echo "ANTHROPIC_API_KEY missing in ${ENV_FILE}." >&2
  exit 1
fi

if [[ -z "${ALCHEMY_API_KEY:-}" ]]; then
  echo "ALCHEMY_API_KEY missing in ${ENV_FILE}." >&2
  exit 1
fi

ANVIL_FORK_URL="${ANVIL_FORK_URL:-https://eth-mainnet.g.alchemy.com/v2/${ALCHEMY_API_KEY}}"
ANVIL_FORK_BLOCK="${ANVIL_FORK_BLOCK:-}"
KEY_SUFFIX="${ALCHEMY_API_KEY: -4}"
ANVIL_FORK_DESC="Ethereum mainnet fork via Alchemy (key suffix ...${KEY_SUFFIX})"
if [[ -n "${ANVIL_FORK_BLOCK}" ]]; then
  ANVIL_FORK_DESC="${ANVIL_FORK_DESC}, block ${ANVIL_FORK_BLOCK}"
fi

FORK_ARGS=(--fork-url "${ANVIL_FORK_URL}")
if [[ -n "${ANVIL_FORK_BLOCK}" ]]; then
  FORK_ARGS+=(--fork-block-number "${ANVIL_FORK_BLOCK}")
fi

ANVIL_ARGS=(
  --host "${ANVIL_HOST}"
  --port "${ANVIL_PORT}"
  --chain-id "${ANVIL_CHAIN_ID}"
  --block-time 2
  --steps-tracing
)
ANVIL_ARGS+=("${FORK_ARGS[@]}")

export CHAIN_NETWORK_URLS_JSON="${CHAIN_CONFIG}"

cleanup() {
  if [[ -n "${ANVIL_PID:-}" ]] && ps -p "${ANVIL_PID}" >/dev/null 2>&1; then
    echo "Stopping Anvil (pid ${ANVIL_PID})"
    kill "${ANVIL_PID}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${TMP_OUTPUT:-}" && -f "${TMP_OUTPUT}" ]]; then
    rm -f "${TMP_OUTPUT}"
  fi
}
trap cleanup EXIT

echo "Default funded accounts:"
echo "  Alice (account 0): ${ALICE_ACCOUNT}"
echo "  Bob   (account 1): ${BOB_ACCOUNT}"
echo "Starting Anvil on ${ANVIL_HOST}:${ANVIL_PORT} (chain-id ${ANVIL_CHAIN_ID}) using ${ANVIL_FORK_DESC}..."
"${ANVIL_BIN}" "${ANVIL_ARGS[@]}" >"${ANVIL_LOG}" 2>&1 &
ANVIL_PID=$!

for attempt in {1..20}; do
  if curl -s "http://${ANVIL_HOST}:${ANVIL_PORT}" >/dev/null 2>&1; then
    echo "Anvil is up (attempt ${attempt})."
    break
  fi
  sleep 0.5
done

if ! ps -p "${ANVIL_PID}" >/dev/null 2>&1; then
  echo "Anvil failed to start; check ${ANVIL_LOG}" >&2
  exit 1
fi

CARGO_CMD=(cargo test -p eval --features eval-test)
if [[ -n "${TEST_FILTER}" ]]; then
  CARGO_CMD+=("${TEST_FILTER}")
fi
CARGO_CMD+=(-- --nocapture --ignored --test-threads=1)
CARGO_CMD_STR="cargo test -p eval --features eval-test"
if [[ -n "${TEST_FILTER}" ]]; then
  CARGO_CMD_STR+=" ${TEST_FILTER}"
fi
CARGO_CMD_STR+=" -- --nocapture --ignored --test-threads=1"

echo "Running eval suite (${CARGO_CMD_STR}) with Anthropic key from ${ENV_FILE}..."
pushd "${ROOT_DIR}/aomi" >/dev/null
set +e
"${CARGO_CMD[@]}" 2>&1 | tee "${TMP_OUTPUT}"
TEST_EXIT=${PIPESTATUS[0]}
set -e
popd >/dev/null

RUN_TIMESTAMP="$(date -u +"%Y-%m-%d %H:%M:%S %Z")"
{
  echo "# Eval Test Results"
  echo
  echo "- Timestamp: ${RUN_TIMESTAMP}"
  echo "- Command: ${CARGO_CMD_STR}"
  echo "- Chain: ${ANVIL_FORK_DESC}"
  echo "- Anvil log: ${ANVIL_LOG}"
  echo "- Default Alice: ${ALICE_ACCOUNT}"
  echo "- Default Bob: ${BOB_ACCOUNT}"
  echo
  echo "## Output"
  echo '```'
  cat "${TMP_OUTPUT}"
  echo '```'
} > "${OUTPUT_FILE}"
echo "Eval test results saved to ${OUTPUT_FILE}"

rm -f "${TMP_OUTPUT}"

if [[ "${TEST_EXIT}" -ne 0 ]]; then
  exit "${TEST_EXIT}"
fi
