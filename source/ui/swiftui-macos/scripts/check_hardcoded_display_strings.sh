#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT_DIR/BridgingIO"

PATTERN='Text\("([^"\\]|\\.)*"|Label\("([^"\\]|\\.)*"|Button\("([^"\\]|\\.)*"|TextField\("([^"\\]|\\.)*"|Toggle\("([^"\\]|\\.)*"|Stepper\("([^"\\]|\\.)*"'

matches="$(
  rg -n "$PATTERN" "$TARGET_DIR" \
    --glob '!**/Localization/**' \
    --glob '!**/*.lproj/**' \
    --glob '!**/Tests/**' \
    --glob '!**/*Tests.swift' \
    || true
)"

if [[ -n "$matches" ]]; then
  echo "Found hardcoded display strings. Please use L10n.t/L10n.f instead:"
  echo "$matches"
  exit 1
fi

echo "OK: no hardcoded display string literals found in UI display APIs."
