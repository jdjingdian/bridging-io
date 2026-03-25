# BridgingIO Cross-Platform UI Test Contract

This checklist defines the minimum UI Test contract for every platform UI:

- macOS (SwiftUI)
- Linux
- Windows
- OpenHarmony PC

All platform implementations must provide equivalent automated UI coverage before
release, even if test frameworks differ.

## Required User Flows

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

## Test Case Matrix

| ID | Flow | Minimum assertion |
| --- | --- | --- |
| UI-01 | Target selection/create | Selected target appears in target list and detail panel |
| UI-02 | Session detail | Session state and environment summary are rendered |
| UI-03 | Timeline | Command card list renders and supports collapse/expand |
| UI-04 | Artifact | Artifact detail opens and derived artifact appears after refine |
| UI-05 | Approval | Pending -> approved/denied/failed transitions are observable |

## Platform Mapping

- macOS: XCUITest under `source/ui/swiftui-macos/Tests/UITests/`
- Linux: framework to be selected by platform team, must keep UI-01..UI-05
- Windows: framework to be selected by platform team, must keep UI-01..UI-05
- OpenHarmony PC: framework to be selected by platform team, must keep
  UI-01..UI-05

## Exit Criteria

- A platform UI change is not complete without automated tests covering UI-01
  through UI-05.
- Archive handoff should report which platform suites ran and their outcomes.
