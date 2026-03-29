# Tauri Bundled Webview (Desktop Console)

This folder hosts bundled webview assets for the cross-platform desktop shell
and is the single source of truth for bundled desktop pages.

- `index.html`: full workspace shell and route host (`#/onboarding`, `#/workspace/targets`, `#/workspace/timeline`, `#/workspace/settings`)
- `onboarding.html`: runtime root setup gate with trusted host bridge calls
- `src-tauri/`: production Tauri host crate that owns startup routing,
  sidecar lifecycle, and trusted command bridge wiring

The onboarding page intentionally has no cancel/skip path before runtime root
selection and writable validation succeed.

The workspace page implements:

- unified status bar with runtime/core/restart state
- isolated `Targets` list/detail/edit flow
- `Timeline` actor grouping (token label first, HTTP fingerprint fallback), execution cards, and dedicated detail panel with on-demand transcript loading
- `Settings` sections for `Core Runtime`, `Storage & Cache`, `Tokens`, and `Vault`
- token summary + revoke flow before vault deep-management actions
- vault unlock and long-lived token issuance gated by trusted local user verification triggers

Automated contract coverage for these bundled flows is enforced in:

- `source/rust/bridgingio-desktop-host/src/bundle.rs`

## Sidecar Staging Contract

`src-tauri` resolves `bridgingio-core` sidecar in this order:

1. `BRIDGINGIO_CORE_SIDECAR` env override
2. `source/ui/tauri-console-web/src-tauri/bin/bridgingio-core[.exe]`
3. `source/rust/target/debug/bridgingio-core[.exe]`
4. `source/rust/target/release/bridgingio-core[.exe]`
5. packaged sidecar directory near app executable

Stage a locally built sidecar into `src-tauri/bin`:

```bash
source/ui/tauri-console-web/src-tauri/scripts/stage-sidecar.sh debug
```
