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
- Required user flows:
  - target selection or creation
  - session state view
  - command timeline view
  - artifact detail or refine view
  - approval request handling

Future platform UI test suites should mirror the same required flows.

## Run Conventions

The default run plan is:

```bash
cd source/rust
cargo test
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
