# BridgingIO Engine (Core-Owned Settings)

This crate owns the standalone core settings schema and runtime metadata
boundary.

## Versioned TOML Schema

The MVP standalone config format is TOML with:

- `schema_version`
- `[core]`
- `[storage]`
- `[storage.artifacts]`
- `[vault]`
- `[control_plane]`
- `[model_plane.http]`
- `[model_plane.http.auth]`
- `[toolchains.<name>]`
- `[policies.defaults]`
- `[[targets]]`

The parser and validator live in `src/lib.rs` (`CoreSettings`).

## Field Descriptions

Programmatic field docs are exposed by:

- `CoreSettings::field_descriptions()`

This is used by app/control-plane APIs so UI and CLI can render consistent
descriptions from a core-owned source of truth.

## Sample Configs

Two samples are included for pre-UI standalone validation:

- minimal: `tests/fixtures/standalone-minimal.toml`
- complete: `tests/fixtures/standalone-complete.toml`

`CoreSettings::minimal_example()` and `CoreSettings::complete_example()` load
these fixtures directly.
