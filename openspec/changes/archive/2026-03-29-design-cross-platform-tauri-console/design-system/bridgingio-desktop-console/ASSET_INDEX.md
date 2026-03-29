# BridgingIO Desktop Console Design Asset Index

This index tracks design assets for the cross-platform desktop console change.
These assets are the reference baseline for future macOS console refactor work.

## Canonical Design Rules

- `MASTER.md`  
  Global visual/system rules for the cross-platform console baseline.

- `pages/onboarding.md`  
  Onboarding page-specific overrides.

- `pages/workspace.md`  
  Workspace shell and navigation page-specific overrides.

- `pages/settings-vault.md`  
  Settings/Vault page-specific overrides.

## Runtime/UI Contract References

- Bundled webview source:
  - `source/ui/tauri-console-web/onboarding.html`
  - `source/ui/tauri-console-web/index.html`
- Embedded host assets:
  - `source/rust/bridgingio-desktop-host/assets/onboarding.html`
  - `source/rust/bridgingio-desktop-host/assets/index.html`
- Contract automation:
  - `source/rust/bridgingio-desktop-host/src/bundle.rs`

## Migration Note

When macOS UI is reworked, start from this cross-platform design index and
handoff docs, then apply platform-specific refinements as an incremental layer.
