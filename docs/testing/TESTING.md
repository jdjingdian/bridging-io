# BridgingIO Testing Baseline

This document defines testing conventions for Rust core and platform UI tests.

## Rust Test Layers

1. Unit tests live close to code in each crate (`src/lib.rs` or module tests).
2. Cross-crate integration tests live in crate-level `tests/` directories
   (for example `source/rust/bridgingio-mcp/tests/`).
3. Cross-module workflows should be validated via integration tests before task
   archive.

## UI Test Baseline

- macOS UI tests live in `source/ui/swiftui-macos/Tests/UITests/`.
- bundled desktop console UI contract tests live in
  `source/rust/bridgingio-desktop-host/src/bundle.rs`.
- Required user flows:
  - target selection or creation
  - session state view
  - command timeline view
  - artifact detail or refine view
  - approval request handling
  - runtime-root onboarding gate
  - workspace primary navigation (`Targets`/`Timeline`/`Settings`)
  - timeline source grouping (`token_label` + `http_fingerprint`)
  - token revoke in trusted settings
  - vault unlock trusted verification entrypoint

Future platform UI test suites should mirror the same required flows.

## Run Conventions

The default run plan is:

```bash
cd source/rust
cargo test
```

Bundled desktop console contract suite:

```bash
cd source/rust
cargo test -p bridgingio-desktop-host
```

Tauri shell host smoke verification:

```bash
scripts/testing/run-tauri-shell-smoke.sh
```

## Core Platform Contract

Cross-platform core verification must follow:

- `docs/testing/CORE_PLATFORM_CONTRACT.md`
- `scripts/testing/run-core-platform-contract.sh`

Recommended matrix baseline (macOS, Windows, Linux):

```bash
scripts/testing/run-core-platform-contract.sh
```

The script emits a JSONL execution record under `tmp/` and includes:

- per-suite status
- retries/jitter observation fields for flaky tracking
- command-level trace for manual/CI parity
