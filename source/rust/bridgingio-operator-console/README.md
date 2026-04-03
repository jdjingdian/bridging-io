# BridgingIO Operator Console

This crate provides the `ratatui`-based menuconfig experience for
`bridgingio-core`.

Current scope:

- `bridgingio-core menuconfig` command model
- tree-style configuration browsing
- help panel, search, edit, dirty tracking
- config save with apply strategy hint
- display-safe vault/runtime summaries

Highlight behavior:

- menu/footers use reverse highlight for selected state by default.
- if `NO_COLOR` is set or `TERM=dumb`, menuconfig switches to fallback highlight.
- set `BRIDGINGIO_MENUCONFIG_FORCE_FALLBACK_HIGHLIGHT=1` to force fallback-only
  selected markers during troubleshooting.
- confirm/choice popups use the same selected highlight contract as menu rows.

The operator console intentionally reuses the core-owned settings schema and
field descriptions instead of defining a TUI-private schema.

Canonical style source of truth:

- `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`
