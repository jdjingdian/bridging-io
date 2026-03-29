# BridgingIO Cross-Platform UI Test Contract

This checklist defines the minimum automated UI contract for every platform UI:

- macOS (SwiftUI)
- Linux
- Windows
- OpenHarmony PC

All platform implementations must provide equivalent automated UI coverage before
release, even if test frameworks differ.

## Required User Flows

### Baseline Console Flows

1. Target selection or creation
- User can create or select an SSH/ADB target profile.
- UI displays the selected target type and identity.

2. Session state visibility
- User can open session detail.
- UI shows connection state, last activity, environment fingerprint summary, and
  capability summary.

3. Command timeline interaction
- User can view command/event timeline.
- Long output cards are collapsible and can be expanded.
- Timeline cards expose execution status and linked artifact id.

4. Artifact detail and refine
- User can open artifact details from timeline.
- User can run a second-stage filter/refine action and view derived result.

5. Approval request lifecycle
- UI shows pending approval requests with context.
- UI state transitions for approved, denied, and failed/expired requests are
  visible.

### Bundled Desktop Console Flows (Tauri Shell + Bundled Webview)

6. Runtime-root onboarding gate
- First launch enters onboarding and requires runtime-root selection.
- UI does not expose skip/cancel/bypass path before writable validation.

7. Workspace primary navigation
- Workspace exposes isolated first-level entries for `Targets`, `Timeline`,
  and `Settings`.
- Navigation switches view via bundled hash routing without relying on an
  external localhost management page.

8. Timeline source grouping
- Timeline actor grouping supports both token-label grouping and HTTP
  fingerprint/user-agent fallback grouping.
- Execution detail remains split from list view and can load transcript
  snippets on demand.

9. Token summary and revoke flow
- Settings shows display-safe token summaries.
- Revoke action is available in the same trusted settings surface and refreshes
  list state.

10. Vault unlock trusted-entry flow
- Settings exposes vault unlock entrypoint.
- Unlock path requires trusted local verification trigger before host bridge
  unlock call.

## Test Case Matrix

| ID | Flow | Minimum assertion |
| --- | --- | --- |
| UI-01 | Target selection/create | Selected target appears in target list and detail panel |
| UI-02 | Session detail | Session state and environment summary are rendered |
| UI-03 | Timeline | Command card list renders and supports collapse/expand |
| UI-04 | Artifact | Artifact detail opens and derived artifact appears after refine |
| UI-05 | Approval | Pending -> approved/denied/failed transitions are observable |
| UI-06 | Onboarding gate | Runtime-root onboarding requires choose + validate before continue |
| UI-07 | Workspace nav | `Targets`/`Timeline`/`Settings` are isolated and route-switchable |
| UI-08 | Timeline grouping | Both `token_label` and `http_fingerprint` actor grouping paths are covered |
| UI-09 | Token revoke | Token summary list exposes revoke and reflects revoked status |
| UI-10 | Vault unlock | Vault unlock entrypoint enforces trusted local verification trigger |

## Platform Mapping

- macOS: XCUITest under `source/ui/swiftui-macos/Tests/UITests/`
- Bundled desktop console contract automation:
  `source/rust/bridgingio-desktop-host/src/bundle.rs` tests
  (`bundled_assets_match_tauri_console_web_sources`,
  `workspace_navigation_and_timeline_grouping_ui_contract`,
  `settings_security_ui_contract_covers_token_revoke_and_vault_unlock`,
  plus onboarding/workspace markup contract tests)
- Linux: framework to be selected by platform team, must keep UI-01..UI-10
- Windows: framework to be selected by platform team, must keep UI-01..UI-10
- OpenHarmony PC: framework to be selected by platform team, must keep
  UI-01..UI-10

## Exit Criteria

- A platform UI change is not complete without automated tests covering UI-01
  through UI-10.
- Archive handoff should report which platform suites ran and their outcomes.
