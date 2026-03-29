#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DIST_DIR="${WEB_ROOT}/dist"

mkdir -p "${DIST_DIR}"

for name in index.html onboarding.html; do
  src="${WEB_ROOT}/${name}"
  dst="${DIST_DIR}/${name}"
  if [[ ! -f "${src}" ]]; then
    echo "missing web asset: ${src}" >&2
    exit 1
  fi
  cp "${src}" "${dst}"
done

echo "synced web assets to ${DIST_DIR}"
