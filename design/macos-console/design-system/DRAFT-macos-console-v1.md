# BridgingIO macOS Console Design Draft (For Review)

Status: draft, waiting for project-owner confirmation.

This draft is generated with `ui-ux-pro-max` and adjusted for an operator
console use case (not a marketing landing page).

## 1) Raw Skill Outputs (Condensed)

- Pattern suggestion: `Product Demo + Features`
- Style suggestion: `Vibrant & Block-based`
- Color suggestion:
  - Primary `#1E293B`
  - Secondary `#334155`
  - CTA `#22C55E`
  - Background `#0F172A`
  - Text `#F8FAFC`
- Typography suggestion:
  - Heading: `JetBrains Mono`
  - Body: `IBM Plex Sans`
- SwiftUI stack guidance:
  - prefer `NavigationStack` + `navigationDestination`
  - use `@Environment(\.dismiss)` for programmatic dismiss
- UX constraints:
  - text contrast >= 4.5:1
  - error and state indicators cannot rely on color only

## 2) Console-Specific Adaptation

The raw pattern/style recommendations are useful for visual direction but too
marketing-heavy for a high-density desktop console. BridgingIO should use:

- information-dense, split-pane desktop layout
- restrained motion and reduced decorative geometry
- strong hierarchy for session state, timeline, and approvals

### Proposed Information Architecture

1. Left sidebar: targets and quick filters
2. Center pane: command timeline with collapsible cards
3. Right pane: session detail, capability summary, approval queue
4. Bottom drawer: artifact preview and refine controls

## 3) Design Tokens (Draft)

```text
--bg-canvas: #0F172A
--bg-panel: #111827
--bg-elevated: #1F2937
--text-primary: #F8FAFC
--text-secondary: #CBD5E1
--text-muted: #94A3B8
--accent-success: #22C55E
--accent-info: #38BDF8
--accent-warning: #F59E0B
--accent-danger: #EF4444
--border-subtle: #334155
```

Typography:

- title and metric labels: `JetBrains Mono`
- body and long-form content: `IBM Plex Sans`

## 4) Component Priorities (MVP)

1. Target list item (status + last seen + type badge)
2. Session header (state, fingerprint summary, capability chips)
3. Timeline command card (request time, status, stdout/stderr toggle, artifact id)
4. Approval card (pending/approved/denied with context)
5. Artifact panel (preview, chunk pagination, refine keyword input)

## 5) Interaction and Accessibility Baseline

- Support keyboard navigation across sidebar, timeline, and approval queue.
- Use icon + label for state, not color alone.
- Keep state transitions subtle (150ms to 250ms).
- Respect reduced motion settings for timeline updates.
- Maintain minimum 4.5:1 contrast for body text.

## 6) Open Items For Confirmation (Task 5.2)

The following items require project-owner decision before implementation:

1. Final palette direction (dark-first vs dual-theme at MVP).
2. Typography finalization (`JetBrains Mono + IBM Plex Sans` or alternative).
3. Layout priority for three-pane vs two-pane + drawer.
4. Timeline default density (compact by default or expanded by default).
5. Approval queue placement (right pane fixed vs modal workflow).

## 7) Next Action

After confirmation, persist the approved version to:

- `design/macos-console/design-system/MASTER.md`
- optional page overrides in `design/macos-console/design-system/pages/*.md`

