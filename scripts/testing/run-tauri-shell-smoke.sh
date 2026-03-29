#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNTIME_ROOT="${ROOT_DIR}/tmp/tauri-shell-smoke-runtime"

mkdir -p "${RUNTIME_ROOT}"

echo "[smoke] running desktop host contract tests"
(
  cd "${ROOT_DIR}/source/rust"
  cargo test -p bridgingio-desktop-host
)

echo "[smoke] running desktop host startup smoke"
(
  cd "${ROOT_DIR}/source/rust"
  cargo run -p bridgingio-desktop-host --bin desktop-host-smoke -- "${RUNTIME_ROOT}"
)

echo "[smoke] completed"
