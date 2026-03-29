# BridgingIO Desktop Host (Tauri Shell Contract)

This crate provides the host-side contract primitives for the cross-platform
`Tauri Shell + bundled webview + bridgingio-core sidecar` architecture.

Included capabilities:

- single-window + tray/menu lifecycle model
- runtime root startup preference persistence and startup routing
- trusted local control-plane bridge (line codec over local transport)
- managed restart state + recovery action model
- onboarding gate contract (no cancel path before valid runtime root)

The crate intentionally focuses on host/runtime state and contract tests so
other platform shells can reuse the same behavior.
