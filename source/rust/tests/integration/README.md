# Integration Tests

Workspace-level integration scenarios are primarily implemented in crate-level
`tests/` directories (for example `bridgingio-mcp/tests/`), because this
workspace root is virtual-only.

Suggested scenario coverage:

- session lifecycle plus capability discovery
- command execution to artifact generation flow
- policy decision to approval request flow
- multi-agent isolation and logical session resume
- multi-channel concurrency tracking
