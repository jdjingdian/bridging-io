# Runtime Root Layout Contract

This contract defines how `bridgingio-core` owns and uses directories under a runtime root.

## Scope

- Applies to `ui-managed-ephemeral` runtime roots and any host-managed runtime root with core-owned config.
- Standalone default startup uses the canonical runtime root under `~/.bridgingio` when `--config` is omitted.
- Host platform differences are resolved by `RuntimePathsAdapter`.

## Reserved By Core

`bridgingio-core` owns the following paths and may create/update/remove files inside them:

- `config/managed-core.toml`
- `state/`
- `artifacts/`
- `logs/`

Within `state/`, core currently reserves:

- `metadata.sqlite3`
- `managed-instance.json` (instance discovery hint, ownership/probe summary seed)
- `control-plane.sock` on Unix hosts
- named-pipe endpoint `\\.\pipe\bridgingio-<sanitized-instance>-control-plane` on Windows hosts

## Consumable By UI

UI hosts may read from:

- `config/managed-core.toml` (read-only unless using core APIs to update settings)
- `logs/` for diagnostics presentation
- `artifacts/` only through core APIs (direct file assumptions are not stable contract)
- `state/metadata.sqlite3` should be treated as core-internal and not read directly

## Write Boundary

- UI hosts should not write into `state/`, `artifacts/`, or `logs/` directly.
- UI hosts should request settings/profile changes through control-plane APIs.
- Runtime root selection and persistence belong to the host UI; directory internals remain core-owned.

## Bootstrap Requirements

Before startup, runtime bootstrap must verify:

- runtime root and reserved subdirectories are directories
- runtime root, `state/`, `artifacts/`, and `logs/` are writable
- metadata parent directory is writable

If checks fail, startup should return explicit recovery guidance (choose another runtime root or fix permissions).
