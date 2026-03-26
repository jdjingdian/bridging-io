# Pixso Export Assets

This folder stores exported reference images from Pixso for implementation
alignment.

## Why Keep Exports

- prevent visual drift between design intent and SwiftUI implementation
- provide stable references for UI review and UI tests
- preserve snapshots for future cross-platform adaptation

## Naming Convention

`<page>-<variant>-<tag>.<ext>`

Examples:

- `workspace-console-dark-v1.0.png`
- `workspace-console-light-v1.0.png`
- `target-profile-sheet-dark-v1.0.png`
- `macos-console-dev-frame.png`

## Capture Notes

- 2026-03-26 attempt 1: Pixso MCP returned `Nothing is selected`.
- 2026-03-26 attempt 2: selected frame `dev` captured successfully.
  - frame id: `3:1`
  - frame size: `2387 x 2858`
  - `get_node_dsl`: success
  - `get_image`: success
  - `get_export_image`: success
- 2026-03-26 attempt 3: final baseline asset exported manually in PNG.
  - output file: `macos-console-dev-frame.png`
  - output size: `4774 x 5716`
  - note: this file replaces the discarded lightweight preview.

## Export Workflow

1. Select target frame in Pixso (for example `dev`).
2. Use Pixso MCP `get_export_image` with target constraint.
3. Save exported artifact into this folder with naming convention above.
4. Update this README with date and frame id for traceability.
