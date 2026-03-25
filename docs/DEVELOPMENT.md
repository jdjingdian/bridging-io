# BridgingIO Development Guide

## Project Structure

```text
/
├─ openspec/
├─ design/
├─ docs/
└─ source/
   ├─ rust/
   │  ├─ bridgingio-domain
   │  ├─ bridgingio-engine
   │  ├─ bridgingio-artifacts
   │  ├─ bridgingio-policy
   │  ├─ bridgingio-secrets
   │  ├─ bridgingio-connectors
   │  ├─ bridgingio-providers
   │  ├─ bridgingio-mcp
   │  └─ bridgingio-app-api
   └─ ui/
      └─ swiftui-macos/
```

## MVP Scope

Current MVP scope:

- connectors: SSH, ADB
- providers: terminal, git
- core: target/session domain, artifact pipeline, policy and approval baseline,
  metadata store, logical session reuse, multi-channel tracking
- MCP: capability discovery, typed tool requests, raw command fallback with
  policy check
- macOS UI: currently design-first, implementation after design confirmation

Out of current MVP:

- serial, docker, HTTP/Postman-style debug, OpenGrok, Gerrit
- Linux/Windows/OpenHarmony PC production UI implementation

## Configuration Baseline

- Tool executable resolution follows:
  1. user override path
  2. system PATH
  3. built-in fallback
- Credentials are represented by references (`CredentialRef`) and must not be
  stored as plaintext in config.
- Session reuse behavior is controlled by `SessionReusePolicy`:
  `always_new`, `reuse_if_alive`, `resume_or_create`.

## Testing Requirements

- Core Rust changes require unit tests in the touched crate.
- Cross-module workflows require integration tests (for example
  `source/rust/bridgingio-mcp/tests/`).
- Platform UI work must include UI tests; future platforms must follow the same
  user-flow contract in `docs/testing/UI_PLATFORM_CONTRACT.md`.

Default test command:

```bash
cd source/rust
cargo test
```

## Archive Handoff Requirements

After archive:

- generate commit message proposal that follows `.gitmessage`
- generate PR description based on proposal/design/spec/tasks/test context

See:

- `docs/contributing/ARCHIVE_HANDOFF.md`
- `docs/contributing/ARCHIVE_OUTPUT_TEMPLATE.md`
