# Canonical Vault Operator Guide

This guide defines the baseline operator flow for canonical vault management.

## Security Model Summary

- Canonical vault backend is `builtin-encrypted`; protectors unlock/rewrap keys.
- High-risk actions (`vault unlock`, long-lived token create) require trusted
  local verification and attestation binding.
- Secret-backed runtime actions stay fail-closed when vault is
  `locked/unavailable/uninitialized`.
- Vault lifecycle is explicit: `uninitialized -> init -> locked -> unlocked`;
  deleting vault returns to `uninitialized`.
- Token lifecycle is explicit: `active/expired -> revoked -> deleted`;
  `revoked` is the only delete precondition.
- Desktop settings and standalone management routes must use the same vault and
  token authority truth.

## Protector And Unlock Expectations

- Primary protector is host-policy driven (commonly `os-native`).
- Recovery protector should include `passphrase` (Argon2id profile).
- Unlock policy should be explicit:
  - `trigger_policy`
  - `allowed_methods`
  - `preferred_method`
  - `cache_ttl_sec`
  - `require_fresh_user_verification`

## Standalone Management Routes

All commands require `--config <path>`.

- `bridgingio-core vault init`
- `bridgingio-core vault delete`
- `bridgingio-core vault import --reference <vault://...> [--label ...]`
- `bridgingio-core vault unlock [--method os-native|passphrase]`
- `bridgingio-core auth token create --label <name> [--expires-in-seconds <u64>]`
- `bridgingio-core auth token revoke --token-id <id> [--reason ...]`
- `bridgingio-core auth token delete --token-id <id>`

Lifecycle and confirmation constraints:

- `vault delete` is a destructive action and must be confirmed; it only deletes
  the current runtime store and does not clear shared host keyring entries.
- `token delete` is a destructive action and must be confirmed; only
  `revoked` token can be deleted (including `expired` token, which must first
  be revoked).
- `token create` returns plaintext token only once at creation time.
- Expiring token creation depends on local system time and is guarded by
  rollback detection; when local clock rollback anomaly is detected, expiring
  token create is blocked until clock health is restored.

Secret input source contract:

- accepted routes: `--from-fd`, `--from-stdin`, `--from-file`, `--from-tty-prompt`
- priority when auto-selecting: `fd > stdin > file > tty-prompt`
- explicit multi-source declaration fails closed
- audit emits `source_kind`/`intent_id`/source digest metadata, never plaintext

Startup unlock contract:

- standalone foreground (`run`) keeps the configured `trigger_policy`
- if foreground startup needs passphrase material, it must use hidden local TTY input
- standalone detached (`-d`) overrides `trigger_policy` to `on-core-start`
- detached startup may only consume a one-shot parent-provided local carrier
  (currently piped stdin from the launcher path)
- if detached startup does not receive valid unlock material for an
  `on-core-start` path, startup stays fail-closed instead of silently deferring
  unlock to a later secret access

## Operational Checklist

1. Initialize vault metadata and confirm lock/protector state projection.
2. Import secrets via fd/stdin/file/tty routes (never plaintext argv).
3. For foreground startup, unlock vault through trusted verification or hidden
   local prompt as required by policy.
4. For detached startup, provide startup unlock material via the launcher
   carrier before backgrounding the child process.
5. Create least-privilege tokens, capture one-time reveal, and record label/expiry intent.
6. Revoke stale or expired tokens before deleting them from active management view.
7. Rotate secrets/protector policy periodically; use `vault delete` only for controlled local reset/testing.
