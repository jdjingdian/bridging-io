# Local Operator Interface Matrix

This document is the canonical interface matrix for trusted local operator
surfaces:

- `bridgingio-core menuconfig`
- standalone CLI management routes
- trusted local control-plane / app API
- future local UI/TUI hosts

## Interface Matrix

| Surface | Command / Interface | Callers | Input | Output | Status / Error Highlights | Apply Strategy |
| --- | --- | --- | --- | --- | --- | --- |
| Config | `core.operator_locale` (`managed-core.toml` / standalone config) | local operator, `bridgingio-core` | `en-US` or `zh-CN` | core-owned CLI/TUI locale selection | strict enum validation; unsupported locale is rejected | persisted by `menuconfig` save; runtime applies on restart |
| CLI | `bridgingio-core menuconfig [--config <path>]` | local operator, future UI tooling | optional config path | interactive TUI session, saved config path | display-safe diagnostics only, no secret plaintext display; Security page is state-gated (`uninitialized -> Init Vault`, `locked -> Unlock/Delete Vault`, `unlocked -> Create Token/Token Management/Delete Vault`) | save returns equivalent restart hint when config changed |
| CLI | `bridgingio-core --help` / `bridgingio-core help` | local operator, docs tooling | optional `--config <path>` for locale source | localized help with grouped sections (`Usage` / `Commands` / `Options` / `Notes`) | locale falls back to `en-US`; non-TTY output remains plain text without ANSI escapes | none |
| CLI | `bridgingio-core --version` / `-V` | local operator, automation | none | core version string from workspace Cargo truth | format validated as `YYMM.DD.BuildNumber` with Cargo semver constraints | none |
| CLI | `bridgingio-core run [--config <path>]` | local operator | optional config path | foreground runtime process | startup lifecycle errors use shared error/status contract | N/A |
| CLI | `bridgingio-core -d [--config <path>]` | local operator, launcher | optional config path, one-shot startup carrier if needed | detached runtime process | detached mode overrides vault trigger to `on-core-start`; startup remains fail-closed | N/A |
| CLI | `bridgingio-core vault init` | local operator | config path | display-safe initialization summary | validation/dependency errors are structured and display-safe | live action |
| CLI | `bridgingio-core vault delete` | local operator | config path | display-safe delete summary (`lock_state=uninitialized`) | requires local-admin attestation + confirmation; only deletes current runtime vault store; does not delete shared os-native keyring entries | live action |
| CLI | `bridgingio-core vault import` | local operator | config path, secret input route | display-safe import summary | secret input conflicts fail closed; no plaintext argv | live action |
| CLI | `bridgingio-core vault unlock` | local operator | config path, allowed local carrier | display-safe unlock summary | locked / verification / unavailable paths remain structured | live action |
| CLI | `bridgingio-core auth token create` | local operator | config path, label, optional expiry | one-time reveal result | attestation/verification errors remain structured; expiring token create is blocked when rollback clock anomaly is detected | live action |
| CLI | `bridgingio-core auth token revoke` | local operator | config path, token id | display-safe revoke summary | not-found / validation / dependency errors remain structured | live action |
| CLI | `bridgingio-core auth token delete` | local operator | config path, token id | display-safe delete summary (`status=deleted`) | requires local-admin attestation + confirmation; only `revoked` token can be deleted (`expired` must be revoked first) | live action |
| Control Plane | `attach_ui` | trusted local UI host | host id, ui session id, ui kind | attached response, readiness state | ownership conflict and not-ready are structured | `live_applied` |
| Control Plane | `probe_host_instance` | trusted local UI/TUI host | none | ownership/runtime summary | startup / recovery state is structured | none |
| Control Plane | `get_settings` | trusted local UI/TUI host | none | `CoreSettingsView` | display-safe settings snapshot | none |
| Control Plane | `update_settings` | trusted local UI/TUI host | validated settings delta | accepted + `apply_strategy` | validation/not-ready/method-not-implemented are structured | `live_applied` or `restart_required` |
| Control Plane | `get_vault_state` | trusted local UI/TUI host | none | `VaultStateProjectionView` | display-safe lock/protector summary | none |
| Control Plane | `unlock_vault` | trusted local UI/TUI host | attestation + allowed method | `vault_unlocked` response | locked/verification/mismatch remain structured | live action |
| Control Plane | `create_local_admin_intent` / `complete_local_admin_attestation` | trusted local UI/TUI host | action context | intent / attestation records | verification and mismatch errors remain structured | live action |
| Control Plane | `init_vault` / `delete_vault` | trusted local UI/TUI host | current runtime vault context + attestation for delete | `vault_initialized` / `vault_deleted` response | delete requires attestation + confirmation; delete returns `uninitialized` and removes token management entry points | live action |
| Control Plane | `create_agent_token` / `list_agent_tokens` / `revoke_agent_token` / `delete_agent_token` / `update_agent_token_label` / `update_agent_token_access` | trusted local UI/TUI host | display-safe token management input + attestation for create/delete | one-time reveal or display-safe summaries | `Token Management` uses list -> detail topology: list row exposes `(stable-serial) label [expiry] [status] --->`; detail includes label edit, access toggle, display-safe fingerprint, expiry/status, `Permissions Management --->` placeholder and status-gated revoke/delete. Delete is hidden until `revoked`; runtime still enforces `delete requires revoked`. | live action |
| Model Plane HTTP | `POST /tool/terminal.exec` bearer token authn | MCP clients, local automation | bearer token + target/tool context | execution result or structured authn rejection | bearer token authn rejection uses `domain=authn`, `common_code=credential_rejected`, module subcodes `agent_token_invalid` / `agent_token_disabled` / `agent_token_revoked` / `agent_token_expired`, plus display-safe message and recovery hint | live action |

## Maintenance Rules

- Every new trusted local command, event, or settings mutation path must update
  this matrix.
- `core.operator_locale` is a core-owned operator surface setting only. Future
  UI implementations must not treat it as UI locale source of truth.
- Core version truth is `source/rust/Cargo.toml` `[workspace.package].version`.
  CLI/version metadata must stay aligned with it; do not add duplicate version
  constants in code.
- `bridgingio-core menuconfig` visual grammar, focusability, popup behavior,
  key semantics, and viewport rules must stay aligned with
  `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`; feature changes that alter these
  behaviors must update that matrix together with the relevant OpenSpec spec.
- `method_not_implemented`, `not_ready`, `restart_required`, and equivalent
  formal states must be captured here instead of only in prose docs.
- This matrix must stay aligned with the shared error/status contract and the
  self-test / contract matrix.
