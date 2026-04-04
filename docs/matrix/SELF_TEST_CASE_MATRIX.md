# Self-Test And Contract Case Matrix

This document is the canonical case matrix for `bridgingio-core --self-test`
and the minimum core platform contract suites.

## Support Levels

### Current Targets

- macOS
- Linux `x86_64`
- Linux `aarch64`
- Windows

### Long-Term Planning

- OpenHarmony

OpenHarmony stays in the matrix as a planning placeholder. It must not be
reported as fully validated until real contract runs exist.

## Build Profile Rules

- `--self-test` is a debug-only runtime test framework.
- Debug builds must support `bridgingio-core --self-test`.
- Release builds must reject `bridgingio-core --self-test` with a clear
  diagnostic instead of running the full suite.
- Production health checks must not rely on `--self-test`.

## Case Matrix

| Case ID | Scope | Platform | Arch | Host Mode | Build Profile | Command / Trigger | Expected Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `ST-01` | self-test gating | macOS/Linux/Windows | host arch | standalone | debug | `bridgingio-core --self-test` | full runtime self-test executes |
| `ST-02` | self-test gating | macOS/Linux/Windows | host arch | standalone | release | `bridgingio-core --self-test` | command rejects with debug-only diagnostic |
| `ST-03` | one-shot shell | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[1/8]` | one-shot exec path succeeds |
| `ST-04` | interactive shell | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[2/8]` | open/write/read/interrupt/close succeeds |
| `ST-05` | shell cwd/env | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[3/8]` | cwd/env semantics remain deterministic |
| `ST-06` | path quoting | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[4/8]` | space-containing path and arg handling succeeds |
| `ST-07` | vault/auth smoke | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[5/8]` | canonical ref, fail-closed, broker-only secret use, attestation single-use all succeed |
| `ST-08` | model-plane bind | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[6/8]` | bind probe on `127.0.0.1:19718` succeeds or returns actionable failure |
| `ST-09` | runtime execute | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[7/8]` | standalone runtime execute path succeeds |
| `ST-10` | host platform contract | macOS/Linux/Windows | host arch | standalone | debug | self-test step `[8/8]` | transport/path/toolchain/vault/logger/decoder snapshot succeeds |
| `ST-11` | sensitive target reconcile | macOS/Linux/Windows | host arch | standalone | debug | integration contract (`unlock_vault` -> reconcile) | vault-authoritative `target-profile` restores missing public cache and reports `tamper` / `repair-needed` diagnostics deterministically |
| `ST-12` | SSH key import/management path | macOS/Linux/Windows | host arch | standalone | debug | unit+integration contract (`import ssh key` / duplicate-name rejection / delete-and-reimport / target bind) | unlocked import succeeds, encrypted key requires hidden passphrase capture, duplicate key name is rejected until delete, delete clears bound target refs, target flow binds canonical `credential_ref` only |
| `ST-13` | menuconfig SSH test connection | macOS/Linux/Windows | host arch | standalone | debug | unit+integration contract (`Test Connection` popup flow + worker lifecycle + logs) | plain draft can probe without Apply/Save; failure/timeout/cancel paths are deterministic; sealed entry is hidden while locked and available when unlocked; plain + vault-backed credential locked path returns controlled failure; session logs persist stable `flow_id` with display-safe `ssh.test_connection` phases |
| `CP-01` | core platform contract | macOS | arm64 or x86_64 | standalone | debug | `scripts/testing/run-core-platform-contract.sh` | contract suites produce JSONL record |
| `CP-02` | core platform contract | Linux | x86_64 | standalone | debug | `scripts/testing/run-core-platform-contract.sh` | contract suites produce JSONL record |
| `CP-03` | core platform contract | Linux | aarch64 | standalone | debug | `scripts/testing/run-core-platform-contract.sh` | contract suites produce JSONL record |
| `CP-04` | core platform contract | Windows | x86_64 | standalone | debug | `scripts/testing/run-core-platform-contract.sh` or equivalent manual run | contract suites produce JSONL-equivalent record |

## Cross Validation Rules

- On non-Linux hosts, local validation must be paired with a same-architecture
  Linux contract run.
- Apple Silicon macOS validation must be paired with Linux `aarch64` contract
  validation.
- `x86_64` macOS or Windows validation must be paired with Linux `x86_64`
  contract validation.
- `cross` is the preferred mechanism when the host cannot run the required
  Linux target natively.

## Record Fields

Contract records must include:

- `platform`
- `arch`
- `build_profile`
- `suite`
- `status`
- `retries`
- `jitter_ms`
- `duration_sec`
- `command`
