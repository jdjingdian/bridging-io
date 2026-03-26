# Workspace Console

## Purpose

Primary operator screen for monitoring AI activity while keeping target setup and approvals one click away.

## Key Decisions

- Default view after launch
- Three panes plus bottom drawer
- Timeline is the visual center of gravity
- Settings entry stays visible from the left pane and toolbar
- Theme mode switch is visible in the top toolbar instead of buried in preferences

## Pane Sizing

- Left pane: 260 to 300 px
- Center pane: flexible, preferred minimum 720 px
- Right pane: 320 to 360 px
- Bottom drawer: collapsed to 44 px rail, expanded to 240 to 300 px

## Primary States

- empty state: no target selected
- connected state: session active and timeline streaming
- approval-blocked state: pending request pinned in right pane and echoed in timeline
- degraded state: session still visible with warning styling, not hidden

## Preview

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Toolbar: workspace | search | auto/light/dark | new target | open session  │
├───────────────┬──────────────────────────────────────────┬──────────────────┤
│ Targets       │ Timeline                                  │ Right Rail       │
│               │                                            │                  │
│ ops-prod      │ [Card] git status                          │ Session Summary  │
│ pixel-8       │ [Card] log tail                            │ Capabilities     │
│ lab-host      │ [Card] approval required                   │ Approval Queue   │
│               │ [Card] uname -a                            │ Tool Sources     │
│ + New Target  │                                            │                  │
├───────────────┴──────────────────────────────────────────┴──────────────────┤
│ Artifact Drawer: preview | refine | chunk nav | open full                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Card Anatomy

- top row: time, command preview, state badge
- middle row: concise summary or first lines
- footer row: stdout, stderr, artifact id, expand, refine

## Right Rail Order

1. session status
2. fingerprint summary
3. capability chips
4. pending approvals
5. tool source diagnostics

## Theme Notes

- `Auto` follows the macOS appearance setting
- `Light` and `Dark` are explicit per-user overrides
- The toolbar selector should stay visible even when no target is selected
