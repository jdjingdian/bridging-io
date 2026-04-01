# Canonical Vault Operator Guide

This guide defines the baseline operator flow for canonical vault management.

## Security Model Summary

- Canonical vault backend is `builtin-encrypted`; protectors unlock/rewrap keys.
- High-risk actions (`vault unlock`, long-lived token create) require trusted
  local verification and attestation binding.
- Secret-backed runtime actions stay fail-closed when vault is
  `locked/unavailable/uninitialized`.
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
- `bridgingio-core vault import --reference <vault://...> [--label ...]`
- `bridgingio-core vault unlock [--method os-native|passphrase]`
- `bridgingio-core auth token create --label <name> [--expires-in-seconds <u64>]`
- `bridgingio-core auth token revoke --token-id <id> [--reason ...]`

Secret input source contract:

- accepted routes: `--from-fd`, `--from-stdin`, `--from-file`, `--from-tty-prompt`
- priority when auto-selecting: `fd > stdin > file > tty-prompt`
- explicit multi-source declaration fails closed
- audit emits `source_kind`/`intent_id`/source digest metadata, never plaintext

## Operational Checklist

1. Initialize vault metadata and confirm lock/protector state projection.
2. Import secrets via fd/stdin/file/tty routes (never plaintext argv).
3. Unlock vault through trusted verification flow.
4. Create least-privilege long-lived tokens and capture one-time reveal.
5. Revoke stale tokens and rotate secrets/protector policy periodically.
