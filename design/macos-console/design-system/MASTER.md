# BridgingIO macOS Console Design System

Status: proposed v1 baseline for task 5.2 / 5.3.

Note: Figma MCP sync was attempted on 2026-03-26 but the remote MCP endpoint was unreachable, so this file is the current source of truth until the sketch can be mirrored into Figma.

## Product Stance

BridgingIO for macOS is not a pure audit screen.

It is a desktop operator console with two equally important jobs:

- observe and audit AI activity
- configure targets, credentials, aliases, and tool resolution

## Confirmed Decisions

### Visual Direction

- Theme system: support `Follow System`, `Light`, and `Dark`
- Tone: restrained operator console, not marketing-heavy
- Theme behavior:
  - default to `Follow System`
  - allow explicit operator override to `Light` or `Dark`
  - preserve identical layout and interaction semantics across themes
- Palette, dark mode:
  - canvas `#0F172A`
  - panel `#111827`
  - elevated `#1F2937`
  - border `#334155`
  - text primary `#F8FAFC`
  - text secondary `#CBD5E1`
  - muted `#94A3B8`
  - success `#22C55E`
  - info `#38BDF8`
  - warning `#F59E0B`
  - danger `#EF4444`
- Palette, light mode:
  - canvas `#F3F7FB`
  - panel `#FFFFFF`
  - elevated `#E8EEF5`
  - border `#C9D5E4`
  - text primary `#0F172A`
  - text secondary `#334155`
  - muted `#64748B`
  - success `#15803D`
  - info `#0369A1`
  - warning `#B45309`
  - danger `#B91C1C`

### Typography

- Heading and metric labels: `JetBrains Mono`
- Body and long-form content: `IBM Plex Sans`
- Implementation fallback, if bundling is delayed: `SF Pro Display` and `SF Pro Text`

### Layout

- Primary shell: three-pane desktop workspace
- Bottom artifact drawer: always available, collapsed by default
- Timeline density: compact by default, expandable per card
- Approval placement: fixed in the right pane under session summary
- Global appearance control: toolbar segmented selector for `Auto / Light / Dark`
- All subordinate surfaces, including the target profile editor, must inherit the active workspace appearance mode instead of hard-coding a separate theme

## Workspace Blueprint

```text
┌────────────────────────────────────────────────────────────────────────────────────────────┐
│ BridgingIO                Workspace: local-operator   Theme: Auto Light Dark  Core: active│
│ Search targets / commands                    + New Target      Open Session   Settings    │
├───────────────────────┬──────────────────────────────────────────────┬────────────────────┤
│ Targets               │ Timeline                                      │ Session + Control │
│                       │                                               │                    │
│ Filters               │ Session: ops-prod / ssh                       │ State              │
│ [All] [SSH] [ADB]     │ agent-a • active • last 12s                  │ Connected          │
│                       │                                               │ Ubuntu 24.04 x64   │
│ • ops-prod            │ [09:42] git status                 success    │ zsh / git / ssh    │
│   SSH  Connected      │ artifact: art-001  stdout stderr             │                    │
│   alias: prod         │                                               │ Capabilities       │
│                       │ [09:43] tail app.log              streaming   │ terminal git       │
│ • pixel-8             │ artifact: art-002  preview refine            │ artifacts          │
│   ADB  Idle           │                                               │                    │
│   alias: android-main │ [09:44] rm release.apk            approval    │ Approval Queue     │
│                       │ waiting for operator                          │ [Pending] delete   │
│ • lab-host            │                                               │ reason + actions   │
│   SSH  Degraded       │ [09:45] uname -a                  success     │                    │
│                       │ artifact: art-003                             │ Tool Sources       │
│ + Create Target       │                                               │ ssh: system PATH   │
│ Import Key Ref        │                                               │ adb: bundled       │
│                       │                                               │ override: none     │
├───────────────────────┴──────────────────────────────────────────────┴────────────────────┤
│ Artifact Drawer: art-002 preview | refine keyword [error] | range [120..220] | Open Full │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Information Architecture

### Left Pane

- target filters
- target list
- quick actions for create target, import/select credential ref, and open settings
- optional target-type quick switch for future target families

### Center Pane

- command timeline
- timeline header tied to current logical session
- cards show summary first, raw output only on demand

### Right Pane

- session state and environment fingerprint
- capability chips
- approval queue
- tool source diagnostics and override summary

### Bottom Drawer

- artifact preview
- refine controls
- pagination / chunk navigation
- jump-to-raw action

## Settings Surfaces

Settings should not live in a detached preferences-only screen.

The console must expose settings in-context through:

- `New Target` flow
- target edit sheet
- credential reference picker
- tool source diagnostics panel with optional override action
- global appearance control for `Follow System`, `Light`, and `Dark`

## Target Model Coverage

The target settings surface must not be visually hard-coded to SSH and ADB only.

The design should use a universal target editor with:

- shared fields for all target kinds
  - target name
  - target kind
  - alias for model
  - notes
  - credential reference
  - policy defaults
  - tooling diagnostics and overrides
- one type-specific module that swaps by target kind

Target kinds expected by the design baseline:

- SSH
- ADB
- Serial
- Docker
- HTTP Debug / Postman-like
- OpenGrok / code search

Type-specific module examples:

- SSH: host, port, username
- ADB: serial, transport
- Serial: device path, baud rate
- Docker: container, context, shell preference
- HTTP Debug: base URL, auth ref, environment, headers preset
- OpenGrok: endpoint, repository scope, search API token ref

## Interaction Rules

- Use icon + label for all states; never rely on color alone
- Keep motion subtle, 150ms to 250ms
- Preserve keyboard traversal across target list, timeline, right rail, and artifact drawer
- Collapse long command output by default
- Approval cards must stay visible while pending
- Show the currently effective tool source next to each relevant connector
- Keep theme switching instant and non-destructive; only colors, surfaces, and contrast should change
- Preserve the same field order for shared target settings regardless of target type
- The active target type selector and the visible type-specific form module must always match
- Avoid reviewer-only explanatory widgets inside the actual product surface; the editor should prioritize current-task clarity over showcasing every future type at once

## MVP Page Set

- `pages/workspace-console.md`
- `pages/target-profile-sheet.md`

## Pixso Capture Baseline

To reduce design drift during implementation, canonical Pixso exports should be
stored under `design/macos-console/design-system/assets/`.

Capture status:

- 2026-03-26 (attempt 1): Pixso MCP fetch returned `Nothing is selected`.
- 2026-03-26 (attempt 2): selected frame `dev` fetch succeeded via Pixso MCP.
  - frame id: `3:1`
  - frame size: `2387 x 2858`
  - preview retrieval: success (`get_image`)
  - export retrieval: success (`get_export_image`)
- 2026-03-26 (attempt 3): final baseline artifact saved as PNG.
  - file: `design/macos-console/design-system/assets/macos-console-dev-frame.png`
  - image size: `4774 x 5716`
  - source: manually exported from Pixso after validation
