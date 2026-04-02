# BridgingIO Operator Console

This crate provides the `ratatui`-based menuconfig experience for
`bridgingio-core`.

Current scope:

- `bridgingio-core menuconfig` command model
- tree-style configuration browsing
- help panel, search, edit, dirty tracking
- config save with apply strategy hint
- display-safe vault/runtime summaries

The operator console intentionally reuses the core-owned settings schema and
field descriptions instead of defining a TUI-private schema.
