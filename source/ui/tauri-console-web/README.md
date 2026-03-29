# Tauri Bundled Webview (Desktop Console)

This folder hosts bundled webview assets for the cross-platform desktop shell.

- `index.html`: full workspace shell and route host (`#/onboarding`, `#/workspace/targets`, `#/workspace/timeline`, `#/workspace/settings`)
- `onboarding.html`: runtime root setup gate with trusted host bridge calls

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
