#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
RECORD_DIR="${REPO_ROOT}/tmp"
RECORD_FILE="${RECORD_DIR}/core-platform-contract-${RUN_STAMP}.jsonl"
PLATFORM_LABEL="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH_LABEL="$(uname -m | tr '[:upper:]' '[:lower:]')"
BUILD_PROFILE="debug"

mkdir -p "${RECORD_DIR}"

run_suite() {
  local suite="$1"
  shift
  local cmd=("$@")
  local start end duration status retries max_retries
  retries=0
  max_retries=0
  if [[ "${suite}" == "CP-ONE-SHOT-INTERACTIVE" ]]; then
    max_retries=2
  fi

  start="$(date +%s)"
  status="failed"
  while true; do
    if "${cmd[@]}"; then
      status="passed"
      break
    fi
    if (( retries >= max_retries )); then
      break
    fi
    retries="$((retries + 1))"
  done

  end="$(date +%s)"
  duration="$((end - start))"

  printf '{"platform":"%s","arch":"%s","build_profile":"%s","suite":"%s","status":"%s","retries":%s,"jitter_ms":0,"duration_sec":%s,"command":"%s"}\n' \
    "${PLATFORM_LABEL}" "${ARCH_LABEL}" "${BUILD_PROFILE}" "${suite}" "${status}" "${retries}" "${duration}" "${cmd[*]}" >> "${RECORD_FILE}"

  if [[ "${status}" != "passed" ]]; then
    echo "[contract] suite failed: ${suite}" >&2
    echo "[contract] partial record: ${RECORD_FILE}" >&2
    exit 1
  fi
}

run_suite "CP-OPERATOR-I18N-LITERALS" "${REPO_ROOT}/scripts/testing/check-core-operator-i18n-literals.sh"

cd "${REPO_ROOT}/source/rust"

run_suite "CP-TOOLCHAIN-FALLBACK" cargo test -p bridgingio-connectors
run_suite "CP-RUNTIME-PATHS" cargo test -p bridgingio-platform
run_suite "CP-ONE-SHOT-INTERACTIVE" cargo test -p bridgingio-mcp --test integration_workflows -- --test-threads=1
run_suite "CP-SELF-TEST" cargo run -p bridgingio-mcp --bin bridgingio-core -- --self-test

echo "[contract] all suites passed"
echo "[contract] record file: ${RECORD_FILE}"
