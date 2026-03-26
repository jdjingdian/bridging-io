# BridgingIO macOS Shell

This directory hosts the macOS SwiftUI shell for BridgingIO.

The UI should consume app API events and render:

- target list
- session detail
- command timeline
- artifact detail
- approval queue

Implementation starts only after design-system confirmation in
`design/macos-console/design-system/`.

## Localization Convention

- User-visible copy must come from `Localizable` resources.
- Use `L10n` as the single access point for display text (`L10n.t` / `L10n.f`).
- Do not hardcode display strings in views or view models.

### Localization Checks

- Hardcoded display copy check:
  `./scripts/check_hardcoded_display_strings.sh`
- Localization key consistency + missing-key check:
  `./scripts/check_localization_keys.sh`
