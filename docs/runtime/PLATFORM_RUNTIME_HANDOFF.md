# Platform Runtime Handoff

This handoff is for future Linux / Windows / OpenHarmony UI teams integrating with BridgingIO core runtime.

## Core vs UI Responsibility Boundary

Core guarantees:

- host-platform adapter boundary (`HostPlatformAdapter`) and capability diagnostics
- structured invocation as execution truth source (one-shot and interactive)
- runtime root reserved layout and bootstrap checks
- control-plane transport semantics contract
- toolchain resolution diagnostics hierarchy and fallback semantics
- runtime logger and output decoder baseline behaviors

UI responsibilities:

- host lifecycle ownership (start/attach/restart/shutdown)
- runtime root location selection and persistence policy
- attach gating orchestration in UI-managed mode
- user-facing diagnostics rendering and recovery UX
- platform packaging/distribution specifics outside core contract

## Terminal Family Boundary

Core/runtime maintainers must keep three dimensions explicitly separated:

- host runtime capability: `HostPlatformAdapter` and `local_shell_runtime`
  define how local processes are launched and IO is driven.
- target transport capability: target profiles (`ssh`, `adb`, future
  `localshell`, future `serial`) enter the same target/session/channel/audit
  model.
- target shell semantics: shell dialect and quoting behavior are target-side
  attributes, independent from host OS/runtime details.

Terminal-family concurrency contract:

- `multiplexed`: same target may have multiple active transports/channels.
- `exclusive`: same target must enforce one active holder with explicit
  busy/conflict semantics.

## Workflow Ownership And Review Boundary (A/B/C)

### Workflow A: `host-platform-foundation`

- Main file scope:
  - `source/rust/bridgingio-platform/src/lib.rs`
  - `source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs`
  - `docs/runtime/RUNTIME_ROOT_LAYOUT.md`
- Review boundary:
  - platform adapter traits/implementations
  - transport/path/logger/decoder baseline
- Merge mode:
  - can run in parallel with workflow C docs/tests
  - must merge before workflow B execution-chain finalization

### Workflow B: `terminal-runtime-and-dialects`

- Main file scope:
  - `source/rust/bridgingio-platform/src/local_shell_runtime.rs`
  - `source/rust/bridgingio-providers/src/lib.rs`
  - `source/rust/bridgingio-connectors/src/lib.rs`
  - `source/rust/bridgingio-mcp/src/lib.rs`
- Review boundary:
  - interactive shell lifecycle/state model
  - target dialect + invocation pipeline
  - MCP terminal tool execution consistency
- Merge mode:
  - requires workflow A baseline already stable
  - serial merge expected for overlapping terminal chain files

### Workflow C: `platform-contract-and-parity`

- Main file scope:
  - `source/rust/bridgingio-mcp/tests/integration_workflows.rs`
  - `source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs`
  - `docs/testing/TESTING.md`
  - `docs/testing/CORE_PLATFORM_CONTRACT.md`
  - `scripts/testing/run-core-platform-contract.sh`
- Review boundary:
  - contract suites, matrix reporting, self-test expansion, handoff docs
- Merge mode:
  - can parallelize most docs and test additions
  - final archive sign-off requires A+B merged and green contract matrix records

## Archive Acceptance Criteria

Before archive, all items must be true:

- adapter baseline in A is merged and covered by unit tests
- execution-chain convergence in B is merged and covered by integration tests
- contract matrix suite and self-test in C pass on active platform(s)
- `docs/testing/CORE_PLATFORM_CONTRACT.md` and this handoff are up to date
- runtime layout/sample config assumptions remain host-aware and traceable

## Per-Workflow Handoff Checklist

### Workflow A Checklist

- Design assumptions:
  - host adapter owns shell/transport/paths/toolchain/vault/logger/decoder boundaries
- Affected modules:
  - `bridgingio-platform`, core launcher bootstrap path
- Test requirements:
  - adapter status tests for unix/windows and degraded semantics
- Rollback boundary:
  - rollback only adapter wiring and launcher integration, keep public trait contracts stable

### Workflow B Checklist

- Design assumptions:
  - structured invocation is the only execution truth source
  - host runtime and target transport boundaries stay separated (no localshell
    shortcut path bypassing target/session/channel/audit)
  - terminal-family concurrency policy remains explicit (`multiplexed` /
    `exclusive`)
- Affected modules:
  - connectors/providers/mcp terminal pipeline
- Test requirements:
  - dialect unit tests, concurrency-policy validation, and interactive lifecycle
    integration tests
- Rollback boundary:
  - rollback invocation path as a unit; avoid partial rollback that reintroduces dual truth sources

### Workflow C Checklist

- Design assumptions:
  - platform contract suite is required for release confidence
- Affected modules:
  - integration tests, self-test flow, testing docs/scripts
- Test requirements:
  - matrix record includes retries/jitter fields for flaky observation
- Rollback boundary:
  - rollback only reporting/automation wrapper if needed; keep contract semantics and required suites intact
