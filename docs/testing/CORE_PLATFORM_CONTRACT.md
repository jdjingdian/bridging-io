# Core Platform Contract

This document defines the Rust core cross-platform contract baseline for `establish-cross-platform-core-adapters`.

## Contract Scope

The contract must cover these capability groups:

- shell: one-shot exec and interactive shell lifecycle
- IPC: local control-plane transport semantics
- paths: runtime root, metadata/artifacts/logs/temp and endpoint resolution
- toolchain: target/global/PATH/built-in fallback resolution behavior
- vault: `os-native` backend status and degraded semantics
- logger: runtime event categories and level-controlled output
- decoder: platform-compatible text decode plus newline normalization
- self-test: debug-build-only `bridgingio-core --self-test` validates key contract paths, including vault/auth contract smoke and default model-plane bind probe on `127.0.0.1:19718`

## Matrix Suites

| Suite ID | Capability | Primary coverage |
| --- | --- | --- |
| `CP-ONE-SHOT` | one-shot exec | `source/rust/bridgingio-mcp/tests/integration_workflows.rs` (`validates_ssh_adb_git_flow_with_artifacts_and_approval`) |
| `CP-INTERACTIVE` | interactive shell open/write/read/interrupt/close and isolation | `source/rust/bridgingio-mcp/tests/integration_workflows.rs` (`validates_interactive_shell_mode_context_isolation_and_lifecycle`, `validates_interactive_shell_long_running_interrupt_flow`) |
| `CP-TRANSPORT` | control-plane attach semantics (unix) and host transport contract snapshot | `source/rust/bridgingio-mcp/tests/integration_workflows.rs` unix IPC tests + `bridgingio-core --self-test` step `[8/8]` |
| `CP-RUNTIME-PATHS` | host-aware runtime path resolution and endpoint contract | `source/rust/bridgingio-platform/src/lib.rs` unit tests + `bridgingio-core --self-test` step `[8/8]` |
| `CP-TOOLCHAIN-FALLBACK` | toolchain fallback and diagnostics hierarchy | `source/rust/bridgingio-mcp/src/lib.rs` toolchain tests + connector/runtime integration tests |
| `CP-VAULT-LOGGER-DECODER` | native vault degraded semantics, broker-only secret use, local admin verification, logger categories, decoder normalization | `source/rust/bridgingio-secrets/src/lib.rs` unit tests + `source/rust/bridgingio-platform/src/lib.rs` unit tests + `bridgingio-core --self-test` steps `[5/8]` and `[8/8]` |
| `CP-SELF-TEST` | end-to-end CLI self-test smoke for contract-critical paths, including vault/auth smoke, non-loopback safety defaults, and default model-plane bind diagnostics on `127.0.0.1:19718` | `cargo run -p bridgingio-mcp --bin bridgingio-core -- --self-test` (debug builds only; release rejects) |

## Standard Matrix Run

From repository root:

```bash
scripts/testing/run-core-platform-contract.sh
```

The script runs the minimum reproducible local contract suites and writes a JSONL execution record.

Canonical matrix details live in:

- `docs/matrix/SELF_TEST_CASE_MATRIX.md`

## Execution Record Format (CI And Manual)

Every platform run (macOS, Windows, Linux) should persist one JSONL line per suite:

```json
{
  "platform": "macos|windows|linux",
  "arch": "x86_64|aarch64|arm64",
  "build_profile": "debug",
  "suite": "CP-INTERACTIVE",
  "status": "passed|failed",
  "retries": 0,
  "jitter_ms": 0,
  "duration_sec": 12,
  "command": "cargo test -p bridgingio-mcp --test integration_workflows"
}
```

Required fields:

- `platform`: host OS label used in matrix reports
- `arch`: host CPU architecture label used in matrix reports
- `build_profile`: build profile used for the run
- `suite`: contract suite id
- `status`: final status
- `retries`: number of retries used to pass
- `jitter_ms`: observed timing jitter if flaky behavior was detected
- `duration_sec`: total execution time for the suite
- `command`: exact command used for the run

## Flaky Observation Rules

For interactive shell and transport-related suites:

- keep `retries=0` by default
- if retry is needed, increase `retries` and record observed `jitter_ms`
- mark the run as `failed` if retries exceed project policy for the branch
