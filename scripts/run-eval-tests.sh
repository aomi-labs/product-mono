#!/usr/bin/env bash
# Run the eval test suite with Anthropic credentials
# Anvil is now auto-started by the Rust ForkProvider

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT_DIR}/.env.dev"
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

# Export fork URL for ForkProvider to use
export ANVIL_FORK_URL="${ANVIL_FORK_URL:-https://eth-mainnet.g.alchemy.com/v2/${ALCHEMY_API_KEY}}"

cleanup() {
  if [[ -n "${TMP_OUTPUT:-}" && -f "${TMP_OUTPUT}" ]]; then
    rm -f "${TMP_OUTPUT}"
  fi
}
trap cleanup EXIT

echo "Default funded accounts:"
echo "  Alice (account 0): ${ALICE_ACCOUNT}"
echo "  Bob   (account 1): ${BOB_ACCOUNT}"
echo "Anvil will be auto-started by ForkProvider with fork from Alchemy"

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
