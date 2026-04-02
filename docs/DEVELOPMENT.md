# BridgingIO Development Guide

## Project Structure

```text
/
├─ openspec/
├─ design/
├─ docs/
└─ source/
   ├─ rust/
   │  ├─ bridgingio-domain
   │  ├─ bridgingio-engine
   │  ├─ bridgingio-artifacts
   │  ├─ bridgingio-policy
   │  ├─ bridgingio-secrets
   │  ├─ bridgingio-connectors
   │  ├─ bridgingio-providers
   │  ├─ bridgingio-mcp
   │  └─ bridgingio-app-api
   └─ ui/
      └─ swiftui-macos/
```

## MVP Scope

Current MVP scope:

- connectors: SSH, ADB
- providers: terminal, git
- core: target/session domain, artifact pipeline, policy and approval baseline,
  metadata store, logical session reuse, multi-channel tracking
- MCP: capability discovery, typed tool requests, raw command fallback with
  policy check
- macOS UI: currently design-first, implementation after design confirmation

Terminal provider runtime notes:

- `one-shot exec` keeps command-level isolation.
- `interactive shell` is channel-scoped and stateful (`cwd`, env, transcript,
  interrupt, close).
- Interactive shell prefers a PTY backend for TTY-dependent commands (`top`,
  `stty`); if PTY allocation fails it falls back to pipe backend and emits a
  transcript-level runtime hint.

Cross-platform architecture references:

- core platform contract: `docs/testing/CORE_PLATFORM_CONTRACT.md`
- self-test / contract matrix: `docs/matrix/SELF_TEST_CASE_MATRIX.md`
- local operator interface matrix: `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`
- runtime root layout contract: `docs/runtime/RUNTIME_ROOT_LAYOUT.md`
- platform handoff and workflow boundaries: `docs/runtime/PLATFORM_RUNTIME_HANDOFF.md`
- tauri shell host/webview handoff: `docs/runtime/TAURI_SHELL_HANDOFF.md`

Cross-platform implementation rules:

- `HostPlatformAdapter` is the only boundary for host-local shell, IPC, paths,
  toolchain locator, vault binding, logger, and decoder capabilities.
- structured invocation from connectors is the execution truth source for
  one-shot, interactive, and diagnostics paths.
- target shell dialect selection is independent from host platform; do not
  infer remote dialect from local OS semantics.

Terminal target family baseline:

- `ssh` and `adb` are terminal-family targets and use the shared
  `TerminalConnector` contract for one-shot, interactive, probe, and invocation
  diagnostics paths.
- terminal family metadata uses `targets.terminal.family` and
  `targets.terminal.concurrency_policy` (or profile metadata equivalents).
- default concurrency policy is `multiplexed` for SSH/ADB and `exclusive` for
  serial-like targets.
- `exclusive` targets use target-wide lease semantics in metadata/runtime:
  concurrent holders must get explicit busy/conflict errors.
- future `localshell` is treated as a target transport in the same
  target/session/channel/audit flow, not as a shortcut bypassing target models.

Out of current MVP:

- serial, docker, HTTP/Postman-style debug, OpenGrok, Gerrit
- Linux/Windows/OpenHarmony PC production UI implementation

## Configuration Baseline

- Tool executable resolution follows:
  1. user override path
  2. system PATH
  3. built-in fallback
- Credentials are represented by references (`CredentialRef`) and must not be
  stored as plaintext in config.
- Session reuse behavior is controlled by `SessionReusePolicy`:
  `always_new`, `reuse_if_alive`, `resume_or_create`.

## Testing Requirements

- Core Rust changes require unit tests in the touched crate.
- Cross-module workflows require integration tests (for example
  `source/rust/bridgingio-mcp/tests/`).
- Platform UI work must include UI tests; future platforms must follow the same
  user-flow contract in `docs/testing/UI_PLATFORM_CONTRACT.md`.
- Bundled desktop console contract updates must run
  `cargo test -p bridgingio-desktop-host`.
- Tauri desktop host smoke verification must run
  `scripts/testing/run-tauri-shell-smoke.sh`.

Default test command:

```bash
cd source/rust
cargo test
```

Cross-platform contract baseline:

```bash
scripts/testing/run-core-platform-contract.sh
```

Tauri shell startup smoke:

```bash
scripts/testing/run-tauri-shell-smoke.sh
```

## Tauri Shell Host Layout

Cross-platform desktop host implementation lives under:

- `source/ui/tauri-console-web/src-tauri` (Tauri app crate)
- `source/rust/bridgingio-desktop-host` (host contract/state library)
- `source/ui/tauri-console-web/*.html` (bundled page source of truth)

`bridgingio-core` sidecar path conventions:

1. `BRIDGINGIO_CORE_SIDECAR` (explicit override)
2. `source/ui/tauri-console-web/src-tauri/bin/bridgingio-core[.exe]` (staged)
3. `source/rust/target/{debug,release}/bridgingio-core[.exe]` (local build)
4. packaged sidecar directory adjacent to app executable

Sidecar staging helper:

```bash
source/ui/tauri-console-web/src-tauri/scripts/stage-sidecar.sh debug
```

## Standalone Core Quick Start

1. Optional: prepare or edit config through `menuconfig`.

```bash
cd source/rust

# use the canonical default config under ~/.bridgingio
cargo run -p bridgingio-mcp --bin bridgingio-core -- menuconfig

# or edit a specific config file
cargo run -p bridgingio-mcp --bin bridgingio-core -- menuconfig --config /absolute/path/to/config.toml
```

`menuconfig` is the common configuration mode for `bridgingio-core`; it is not
standalone-specific. Existing `vault ...` and `auth ...` subcommands remain
available as compatibility management routes for now.

2. Optional: prepare a standalone config (TOML) manually. You can start from:

- `source/rust/bridgingio-engine/tests/fixtures/standalone-minimal.toml`
- `source/rust/bridgingio-engine/tests/fixtures/standalone-complete.toml`

3. Start core.

Default standalone startup now uses the canonical runtime root under the user
home directory:

- `~/.bridgingio`
- `config/managed-core.toml` is load-or-create when `--config` is omitted

Examples:

```bash
cd source/rust

# use default runtime root + default config
cargo run -p bridgingio-mcp --bin bridgingio-core --

# use explicit standalone config override
cargo run -p bridgingio-mcp --bin bridgingio-core -- --config /absolute/path/to/standalone.toml
```

4. Validate model-plane HTTP:

```bash
curl http://127.0.0.1:19718/health
```

For local verification, model-plane endpoints currently include:

- `GET /health`
- `GET /state/sessions`
- `GET /state/logical-sessions`
- `POST /mcp` (JSON-RPC MCP entrypoint, for AI model clients)
- `POST /tool/terminal.exec` (key-value body, debug/internal compatibility)

MCP client config example:

```json
{
  "type": "http",
  "url": "http://127.0.0.1:19718/mcp"
}
```

Minimal JSON-RPC verification:

```bash
curl -s http://127.0.0.1:19718/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Artifact refine quick example (regex-like post filter without inline `grep`):

```bash
curl -s http://127.0.0.1:19718/mcp \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":3,
    "method":"tools/call",
    "params":{
      "name":"bridgingio.artifacts.refine",
      "arguments":{
        "source_artifact_id":"mcp-artifact-000001",
        "pattern":"system_server|netd",
        "mode":"auto",
        "grep_flags":"Ei"
      }
    }
  }'
```

Alias-based tool call example (`target = "local"` is alias from config, not raw `adb -s ...`):

```bash
curl -s http://127.0.0.1:19718/mcp \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":2,
    "method":"tools/call",
    "params":{
      "name":"bridgingio.target.inspect_basic",
      "arguments":{
        "target":"local",
        "agent_id":"agent-local",
        "run_id":"run-1",
        "client_session_id":"client-1",
        "reuse_policy":"reuse_if_alive"
      }
    }
  }'
```

## Core Launch Modes And Lifecycle

`bridgingio-core` supports three operator-facing launch modes:

- `menuconfig`: common TUI configuration mode for editing a config file before
  runtime startup.
- `run`: standalone foreground mode. Core stays attached to the current terminal.
- `-d`: standalone detached mode. A launcher process spawns a background core child and returns immediately.
- `ui-managed-ephemeral`: bundled mode for platform UI hosts (for example macOS SwiftUI) that own the core lifecycle.

Examples:

```bash
cd source/rust

# standalone foreground (default config)
cargo run -p bridgingio-mcp --bin bridgingio-core -- run

# menuconfig using the default config
cargo run -p bridgingio-mcp --bin bridgingio-core -- menuconfig

# standalone foreground (explicit config override)
cargo run -p bridgingio-mcp --bin bridgingio-core -- run --config /absolute/path/to/standalone.toml

# standalone detached (default config)
cargo run -p bridgingio-mcp --bin bridgingio-core -- -d

# standalone detached (explicit config override)
cargo run -p bridgingio-mcp --bin bridgingio-core -- -d --config /absolute/path/to/standalone.toml

# bundled UI-managed mode (load-or-create from runtime root)
cargo run -p bridgingio-mcp --bin bridgingio-core -- ui-managed-ephemeral --runtime-root /absolute/path/to/runtime-root
```

Bundled lifecycle contract (`ui-managed-ephemeral`):

- Core discovery identity is runtime-root-stable by default (`state/control-plane.sock` on Unix or platform-equivalent endpoint); host should not depend on per-UI temporary endpoint for normal product flow.
- `--control-plane-socket-override` is a debug/testing escape hatch and should not be used as bundled default.
- Core opens local control-plane first and publishes managed instance metadata in runtime root (`state/managed-instance.json`).
- Core owns `config/managed-core.toml` under runtime root and performs load-or-create on startup.
- Core reserves `state/`, `artifacts/`, and `logs/` under runtime root; UI should treat them as core-owned internals.
- Default model-plane listener is `127.0.0.1:19718` in freshly initialized runtime root config.
- UI must attach through control-plane (`attach_ui`) using stable `host_id` plus per-process `ui_session_id` before model-plane is considered ready.
- Host can call `probe_host_instance` before attach to get structured instance/ownership summary.
- Attach ownership conflicts return structured `ownership_conflict` payloads (instead of only generic validation failure text).
- Before UI attach succeeds, `/mcp` and equivalent model-plane entrypoints return explicit not-ready semantics.
- Settings/profile writes are core-owned; UI submits updates through local control-plane IPC.
- For updates marked `restart_required`, UI must perform managed core restart before treating changes as active.
- When UI exits normally or performs controlled restart, UI host should run two-phase shutdown (`request_shutdown` -> wait process exit -> wait endpoint/model-plane release), escalating only on timeout.

Standalone lifecycle contract (`run` and `-d`):

- Model-plane is available immediately after core startup completes.
- Attach gating is not enforced.
- `run` and `-d` reuse the same runtime/configuration semantics.
- Omit `--config` to use the canonical default runtime root under `~/.bridgingio`.
- Use `--config` only when intentionally overriding the default config path.
- `run` keeps the configured vault `trigger_policy`; if startup needs passphrase
  material, the core uses hidden local TTY input.
- `-d` is still a standalone sub-mode, but it overrides vault startup semantics
  to `on-core-start`; if startup unlock material is required, the launcher must
  provide it through a one-shot local carrier instead of waiting for later
  interaction.
- `menuconfig` is the intended pre-run configuration flow; use it to inspect
  and update config before `run` / `-d`.

Future extension point:

- Keep lifecycle ownership in a host adapter layer so we can later switch from UI child-process hosting to system service hosting (for example `SMAppService`/`SMJobBless`-style paths) without changing control-plane or MCP protocol boundaries.

Runtime root layout contract:

- See `docs/runtime/RUNTIME_ROOT_LAYOUT.md` for core-reserved directories, UI read boundaries, and bootstrap write checks.

## Archive Handoff Requirements

After archive:

- generate commit message proposal that follows `.gitmessage`
- generate PR description based on proposal/design/spec/tasks/test context

See:

- `docs/contributing/ARCHIVE_HANDOFF.md`
- `docs/contributing/ARCHIVE_OUTPUT_TEMPLATE.md`
