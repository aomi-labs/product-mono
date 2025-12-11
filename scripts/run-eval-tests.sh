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
TEST_FILTERS=("$@")

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
export EVAL_COLOR="${EVAL_COLOR:-1}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

# Prepare temp output
: > "${TMP_OUTPUT}"

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

CARGO_BASE=(cargo test -p eval --features eval-test)
COMMANDS_RUN=()
TEST_EXIT=0

pushd "${ROOT_DIR}/aomi" >/dev/null
set +e

if [[ ${#TEST_FILTERS[@]} -le 1 ]]; then
  CARGO_CMD=("${CARGO_BASE[@]}")
  if [[ ${#TEST_FILTERS[@]} -eq 1 ]]; then
    CARGO_CMD+=("${TEST_FILTERS[0]}")
  fi
  CARGO_CMD+=(-- --nocapture --ignored --test-threads=1)
  CARGO_CMD_STR="${CARGO_CMD[*]}"
  COMMANDS_RUN+=("${CARGO_CMD_STR}")
  echo "Running eval suite (${CARGO_CMD_STR}) with Anthropic key from ${ENV_FILE}..."
  "${CARGO_CMD[@]}" 2>&1 | tee -a "${TMP_OUTPUT}"
  TEST_EXIT=${PIPESTATUS[0]}
else
  idx=1
  for filter in "${TEST_FILTERS[@]}"; do
    CARGO_CMD=("${CARGO_BASE[@]}" "${filter}" -- --nocapture --ignored --test-threads=1)
    CARGO_CMD_STR="${CARGO_CMD[*]}"
    COMMANDS_RUN+=("${CARGO_CMD_STR}")
    echo "Running eval suite (${CARGO_CMD_STR}) with Anthropic key from ${ENV_FILE}... [${idx}/${#TEST_FILTERS[@]}]"
    "${CARGO_CMD[@]}" 2>&1 | tee -a "${TMP_OUTPUT}"
    exit_code=${PIPESTATUS[0]}
    if [[ ${exit_code} -ne 0 ]]; then
      TEST_EXIT=${exit_code}
    fi
    idx=$((idx + 1))
  done
fi

set -e
popd >/dev/null

SUMMARY_TABLE="$(
  python - "$TMP_OUTPUT" <<'PY'
import re
import sys

ansi = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")
path = sys.argv[1]

with open(path, "r", encoding="utf-8", errors="ignore") as f:
    lines = [ansi.sub("", line.rstrip("\n")) for line in f]

tests = set()
failures = set()
in_fail_section = False

for line in lines:
    if line.startswith("test "):
        parts = line.split()
        if len(parts) >= 2:
            if parts[1] == "result:":
                continue
            tests.add(parts[1])

    stripped = line.strip()
    if stripped == "failures:":
        in_fail_section = True
        continue

    if in_fail_section:
        if not stripped:
            continue
        if stripped.startswith("test result:"):
            in_fail_section = False
            continue
        if stripped.startswith("failures:"):
            continue
        if re.match(r"^[A-Za-z0-9_:]+$", stripped):
            failures.add(stripped)
            tests.add(stripped)
        continue

# Fallback: if nothing parsed, bail with a friendly note
if not tests:
    print("No tests parsed from cargo output.")
    sys.exit(0)

rows = []
for name in sorted(tests):
    status = "failed" if name in failures else "passed"
    emoji = "❌" if status == "failed" else "✅"
    rows.append(f"| {name} | {emoji} {status} |")

print("| Test | Result |")
print("| --- | --- |")
for row in rows:
    print(row)
print()
print(f"Total: {len(tests)}, Passed: {len(tests) - len(failures)}, Failed: {len(failures)}")
PY
)"

EVAL_TABLE="$(
  python - "$TMP_OUTPUT" <<'PY'
import re
import sys

ansi = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")
path = sys.argv[1]

with open(path, "r", encoding="utf-8", errors="ignore") as f:
    lines = [ansi.sub("", line.rstrip("\n")) for line in f]

blocks = []
idx = 0
while idx < len(lines):
    if lines[idx].strip() == "EVALUATION RESULTS":
        start = idx + 1
        end = start
        while end < len(lines):
            if lines[end].startswith("Test #"):
                break
            end += 1
        block = [ln for ln in lines[start:end] if ln.strip()]
        if block:
            blocks.append("\n".join(block))
        idx = end
    else:
        idx += 1

if not blocks:
    sys.exit(0)

print("\n\n".join(blocks))
PY
)"

echo "Test summary:"
echo "${SUMMARY_TABLE}"
if [[ -n "${EVAL_TABLE}" ]]; then
  echo
  echo "Evaluation summary:"
  echo "${EVAL_TABLE}"
fi

RUN_TIMESTAMP="$(date -u +"%Y-%m-%d %H:%M:%S %Z")"
{
  echo "# Eval Test Results"
  echo
  echo "- Timestamp: ${RUN_TIMESTAMP}"
  echo "- Command(s):"
  for cmd in "${COMMANDS_RUN[@]}"; do
    echo "  - ${cmd}"
  done
  echo "- Chain: ${ANVIL_FORK_DESC}"
  echo "- Anvil log: ${ANVIL_LOG}"
  echo "- Default Alice: ${ALICE_ACCOUNT}"
  echo "- Default Bob: ${BOB_ACCOUNT}"
  echo
  echo "## Summary"
  echo "${SUMMARY_TABLE}"
  echo
  echo "## Evaluation Summary"
  if [[ -n "${EVAL_TABLE}" ]]; then
    echo '```'
    echo "${EVAL_TABLE}"
    echo '```'
  else
    echo "_No evaluation summary captured_"
  fi
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
