# BridgingIO Tauri Shell Host

This crate is the production desktop host for bundled `tauri-console-web`.

## Dependency Boundaries

- `src-tauri` owns Tauri app bootstrap, windowing, sidecar process lifecycle,
  and invoke command registration.
- `source/rust/bridgingio-desktop-host` owns host contract behavior:
  startup routing, onboarding/runtime-root validation, bridge semantics, and
  contract tests.
- `source/ui/tauri-console-web/index.html` and `onboarding.html` are the single
  bundled asset source; host crates consume these files directly.
- `bridgingio-core ui-managed-ephemeral` remains an external sidecar process,
  not an in-process library link.

## Development Commands

Build sidecar:

```bash
cd source/rust
cargo build -p bridgingio-mcp --bin bridgingio-core
```

Stage sidecar into Tauri package layout:

```bash
source/ui/tauri-console-web/src-tauri/scripts/stage-sidecar.sh debug
```

Sync bundled web assets into Tauri `dist/` (auto-runs in `cargo tauri dev/build`):

```bash
source/ui/tauri-console-web/src-tauri/scripts/sync-web-assets.sh
```

Run smoke validation:

```bash
scripts/testing/run-tauri-shell-smoke.sh
```
