#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/BridgingIO"
EN_FILE="$APP_DIR/en.lproj/Localizable.strings"
ZH_FILE="$APP_DIR/zh-Hans.lproj/Localizable.strings"

if [[ ! -f "$EN_FILE" || ! -f "$ZH_FILE" ]]; then
  echo "Missing Localizable files. Expected:"
  echo "  $EN_FILE"
  echo "  $ZH_FILE"
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

extract_keys() {
  rg --no-filename '^[[:space:]]*"' "$1" | sed -E 's/^[[:space:]]*"([^"]+)".*/\1/' | sort -u
}

extract_keys "$EN_FILE" > "$tmp_dir/en.keys"
extract_keys "$ZH_FILE" > "$tmp_dir/zh.keys"

comm -23 "$tmp_dir/en.keys" "$tmp_dir/zh.keys" > "$tmp_dir/missing_in_zh"
comm -13 "$tmp_dir/en.keys" "$tmp_dir/zh.keys" > "$tmp_dir/extra_in_zh"

if [[ -s "$tmp_dir/missing_in_zh" ]]; then
  echo "Missing keys in zh-Hans localization:"
  cat "$tmp_dir/missing_in_zh"
  exit 1
fi

if [[ -s "$tmp_dir/extra_in_zh" ]]; then
  echo "Extra keys in zh-Hans localization (not present in en):"
  cat "$tmp_dir/extra_in_zh"
  exit 1
fi

rg -o 'L10n\.(t|f)\(\"[A-Za-z0-9_.]+"' "$APP_DIR" \
  --glob '!**/Localization/L10n.swift' \
  --no-filename \
  | sed -E 's/.*\(\"([A-Za-z0-9_.]+)"/\1/' \
  | sort -u > "$tmp_dir/code.keys"

comm -23 "$tmp_dir/code.keys" "$tmp_dir/en.keys" > "$tmp_dir/missing_in_en"
if [[ -s "$tmp_dir/missing_in_en" ]]; then
  echo "Keys referenced in code but missing in en Localizable.strings:"
  cat "$tmp_dir/missing_in_en"
  exit 1
fi

echo "OK: localization key sets are consistent and all code keys are defined."
