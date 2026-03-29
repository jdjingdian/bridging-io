# Tauri Shell Runtime Handoff

This handoff defines the host/UI integration contract for the cross-platform
desktop console (`Tauri Shell + bundled webview + bridgingio-core`).

## Ownership Boundary

Host (Tauri shell) responsibilities:

- own single-window lifecycle and tray/menu activation
- own runtime-root onboarding gate and persistence
- own managed core start/attach/restart/shutdown orchestration
- expose trusted bridge commands to bundled webview
- trigger trusted local verification before high-risk vault/token actions

Core/control-plane responsibilities:

- provide bootstrap/settings/timeline/token/vault state projections
- keep management continuity independent from model-plane HTTP address changes
- persist config updates and return apply strategy (`live_applied` /
  `restart_required`)

Bundled webview responsibilities:

- render onboarding, workspace navigation, timeline, settings security views
- call host bridge only (no external localhost management dependency)
- remain display-safe for token/vault projections

## Bridge Contract Checklist

Minimum command surface expected by bundled webview:

- `get_bootstrap_state`
- `get_settings`
- `update_settings`
- `list_agent_tokens`
- `revoke_agent_token`
- `unlock_vault` (after trusted verification)
- `create_agent_token` (long-lived token issuance path)
- `read_artifact` (timeline detail transcript snippet fallback path)

## Continuity Requirement

When model-plane `host/port` changes:

- control-plane update returns `restart_required` when restart is needed
- bootstrap must keep
  `management_plane.requires_management_address_handoff = false`
- workspace remains recoverable through trusted control-plane bridge semantics

Reference tests:

- `source/rust/bridgingio-mcp/src/lib.rs`
  - `update_settings_persists_and_returns_restart_required`
  - `timeline_payload_and_bootstrap_include_source_group_summary`
  - `control_plane_create_list_revoke_agent_token_contract`
  - `control_plane_get_settings_includes_vault_status_projection`

## UI Contract Automation Gate

Bundled UI contract tests must pass before archive:

- `cargo test -p bridgingio-desktop-host`
- key test coverage:
  - onboarding gate (no skip/cancel bypass)
  - workspace primary navigation
  - timeline actor grouping (`token_label` + `http_fingerprint`)
  - token revoke entrypoint in settings
  - vault unlock trusted verification entrypoint

## Design Reference For macOS Refactor

The cross-platform desktop console design assets are the reference baseline for
future macOS UI restructuring:

- `openspec/changes/design-cross-platform-tauri-console/design-system/bridgingio-desktop-console/ASSET_INDEX.md`

Do not treat current SwiftUI layout as the only source of truth when planning
the next macOS console iteration.
