#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

CLI_FILE="${REPO_ROOT}/source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs"
MENU_FILE="${REPO_ROOT}/source/rust/bridgingio-operator-console/src/lib.rs"

failures=0

report_failure() {
  local title="$1"
  local details="$2"
  failures=1
  echo "[i18n-literals] ${title}" >&2
  echo "${details}" >&2
  echo >&2
}

check_cli_help_literals() {
  local usage_block
  usage_block="$(sed -n '/^fn build_usage_text(/,/^fn print_version(/p' "${CLI_FILE}")"
  if [[ -z "${usage_block}" ]]; then
    report_failure "cannot extract build_usage_text() block" "file: ${CLI_FILE}"
    return
  fi

  local hits
  hits="$(
    printf '%s\n' "${usage_block}" \
      | rg --pcre2 -n 'lines\.push\([^)]*"(?:(?=[^"]*\p{Han})|(?=[^"]*[[:space:]])|(?=[^"]*[A-Z]))[^"]*"[^)]*\)' \
      | rg -v 'catalog\.t\(' \
      | rg -v '"bridgingio-core \{\}"' \
      || true
  )"

  if [[ -n "${hits}" ]]; then
    report_failure \
      "hardcoded CLI help display literal detected (use catalog key instead)" \
      "${hits}"
  fi
}

check_menuconfig_literals() {
  local production_block
  production_block="$(sed -n '1,/^#\[cfg(test)\]/p' "${MENU_FILE}" | sed '$d')"
  if [[ -z "${production_block}" ]]; then
    report_failure "cannot extract menuconfig production block" "file: ${MENU_FILE}"
    return
  fi

  local hits
  hits="$(
    printf '%s\n' "${production_block}" \
      | rg --pcre2 -n \
        'Line::from\(\s*"[^"]*[\p{Han}A-Za-z][^"]*"|Span::raw\(\s*"[^"]*[\p{Han}A-Za-z][^"]*"|Paragraph::new\(\s*"[^"]*[\p{Han}A-Za-z][^"]*"|\.title\(\s*"[^"]*[\p{Han}A-Za-z][^"]*"|last_status\s*=\s*"[^"]*[\p{Han}A-Za-z][^"]*"|menu_entry\(\s*"[^"]*[\p{Han}A-Za-z][^"]*"' \
      || true
  )"

  if [[ -n "${hits}" ]]; then
    report_failure \
      "hardcoded menuconfig display literal detected (use catalog key instead)" \
      "${hits}"
  fi
}

check_cli_help_literals
check_menuconfig_literals

if [[ "${failures}" -ne 0 ]]; then
  echo "[i18n-literals] FAILED" >&2
  exit 1
fi

echo "[i18n-literals] PASS"
