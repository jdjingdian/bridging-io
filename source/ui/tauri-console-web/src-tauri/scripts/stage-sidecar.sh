#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../../../.." && pwd)"
MODE="${1:-debug}"
BIN_NAME="bridgingio-core"
HOST_TRIPLE="$(rustc -vV | awk '/host: / {print $2}')"

if [[ "${OS:-}" == "Windows_NT" || "${OSTYPE:-}" == msys* || "${OSTYPE:-}" == cygwin* ]]; then
  BIN_NAME="bridgingio-core.exe"
fi

SOURCE_BIN="${REPO_ROOT}/source/rust/target/${MODE}/${BIN_NAME}"
DEST_DIR="${REPO_ROOT}/source/ui/tauri-console-web/src-tauri/bin"
DEST_BIN="${DEST_DIR}/${BIN_NAME}"
DEST_TRIPLE_BIN="${DEST_DIR}/bridgingio-core-${HOST_TRIPLE}"

if [[ ! -f "${SOURCE_BIN}" ]]; then
  echo "missing sidecar binary: ${SOURCE_BIN}" >&2
  echo "build it first with: cd source/rust && cargo build -p bridgingio-mcp --bin bridgingio-core" >&2
  exit 1
fi

mkdir -p "${DEST_DIR}"
cp "${SOURCE_BIN}" "${DEST_BIN}"
cp "${SOURCE_BIN}" "${DEST_TRIPLE_BIN}"
echo "staged sidecar: ${DEST_BIN}"
echo "staged sidecar: ${DEST_TRIPLE_BIN}"
