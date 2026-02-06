#!/usr/bin/env bash
# Run the eval test suite with Anthropic credentials
# Anvil is now auto-started by the Rust ForkProvider

set -euo pipefail

# Ensure Foundry (anvil) and Cargo are in PATH
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${ROOT_DIR}/output"
OUTPUT_FILE="${OUTPUT_DIR}/eval-results.md"
TMP_OUTPUT="$(mktemp)"
TEST_FILTERS=("$@")


export EVAL_COLOR="${EVAL_COLOR:-1}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

# Prepare temp output
: > "${TMP_OUTPUT}"

cleanup() {
  if [[ -n "${TMP_OUTPUT:-}" && -f "${TMP_OUTPUT}" ]]; then
    rm -f "${TMP_OUTPUT}"
  fi
}
trap cleanup EXIT


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
  echo "Running eval suite (${CARGO_CMD_STR}) with Anthropic key..."
  "${CARGO_CMD[@]}" 2>&1 | tee -a "${TMP_OUTPUT}"
  TEST_EXIT=${PIPESTATUS[0]}
else
  idx=1
  for filter in "${TEST_FILTERS[@]}"; do
    CARGO_CMD=("${CARGO_BASE[@]}" "${filter}" -- --nocapture --ignored --test-threads=1)
    CARGO_CMD_STR="${CARGO_CMD[*]}"
    COMMANDS_RUN+=("${CARGO_CMD_STR}")
    echo "Running eval suite (${CARGO_CMD_STR}) with Anthropic key... [${idx}/${#TEST_FILTERS[@]}]"
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
