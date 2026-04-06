# Local Cross-Platform Preflight (Optional)

This document describes an optional developer workflow for early cross-platform
feedback during local development. It is designed for fast iteration and does
not replace formal CI gates.

## Purpose And Scope

- This preflight is a recommended local helper, not a required merge gate.
- It validates the current local worktree snapshot on remote Linux/Windows
  hosts through SSH/SCP.
- It is intentionally separate from:
  - `scripts/testing/run-core-platform-contract.sh`
  - `bridgingio-core --self-test`
  - GitHub Actions gate jobs

## Host Prerequisites

Linux host:

- SSH server reachable from local machine
- Rust toolchain installed and available in PATH

Windows host:

- OpenSSH Server enabled and reachable
- Rust toolchain installed and available in PATH
- Default execution path uses PowerShell/cmd
- Git Bash is optional and only used when explicitly enabled in config

## Local Configuration

Tracked example template:

- `scripts/testing/local-preflight.example.toml`

Recommended local config path (gitignored):

- `tmp/local-preflight.toml`

Create local config from the example and edit host fields (`host`, `port`,
`user`, optional `identity_file`, `password`, and `remote_base`). Keep real
machine details in the local gitignored file only.

Authentication behavior:

- Priority is `identity_file` > `password` > interactive prompt.
- If `identity_file` is configured, SSH key auth is used.
- If `identity_file` is not configured and `password` is configured, runner uses
  non-interactive SSH/SCP authentication.
- If neither key nor password is configured, runner uses interactive SSH/SCP
  prompts.
- Password mode is for local developer convenience only; keep it in gitignored
  local config and never commit real secrets.

## Run Commands

Default mode from config:

```bash
python3 scripts/testing/run-local-cross-platform-preflight.py
```

Force mode for one run:

```bash
python3 scripts/testing/run-local-cross-platform-preflight.py --mode compile-only
```

Run only one platform:

```bash
python3 scripts/testing/run-local-cross-platform-preflight.py --target linux
python3 scripts/testing/run-local-cross-platform-preflight.py --target windows
```

Preview without remote execution:

```bash
python3 scripts/testing/run-local-cross-platform-preflight.py --dry-run
```

## Mode Semantics

- `compile-only`: default compile-first path (`cargo test --workspace --no-run`)
- `unit`: optional deeper local-host command path
- `extended`: heavier checks; on Windows this mode can run an additional
  named-pipe runtime contract probe command.

Mode command strings can be overridden per host in local config.

Windows named-pipe runtime contract probe:

- Config section: `[windows.contracts]`
- `enabled = true` enables the extra contract command.
- `modes = ["compile-only", "extended"]` controls which modes trigger the
  extra command.
- `command` defaults to
  `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File ..\..\scripts\testing\run-windows-ssh-broker-contract.ps1`
- The default command executes Windows-focused broker checks, including
  `bridgingio-core --self-test`, to cover actual named-pipe runtime execution.

## Snapshot And Transfer Behavior

- The runner packages the current local worktree snapshot.
- It excludes `.git/`, `target/`, and local preflight output paths by default.
- It uploads snapshot to remote host via SCP.
- It extracts into remote scratch directory and runs configured command.
- It does not require remote `git clone` or branch checkout.

Archive format:

- Linux: `tar.gz`
- Windows: `zip` + PowerShell `Expand-Archive`

## Windows Git Bash Optional Extra Check

- Windows primary path is always PowerShell/cmd.
- If `[windows.git_bash].enabled = true`, the runner checks whether `bash`
  exists on remote host.
- When available, it runs an additional bash-based check after the primary
  command.
- If `bash` is absent, the extra check is skipped with a clear message.

## Output And Troubleshooting

Per run outputs are written under:

- `tmp/local-preflight-runs/<run-id>/summary.json`
- `tmp/local-preflight-runs/<run-id>/linux.log` (when Linux runs)
- `tmp/local-preflight-runs/<run-id>/windows.log` (when Windows runs)

If local config is missing, the run exits as `skipped` (not failure) and writes
a summary describing `not-configured`.
