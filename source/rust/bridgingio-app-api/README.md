# BridgingIO App API Boundary

This crate defines the boundary contract between trusted local operator
surfaces and the Rust core engine.

## Design Goals

- Keep request and response structures typed and explicit.
- Support event streaming for timeline updates and approval status.
- Keep transport-agnostic models so IPC can be JSON-RPC, UDS, gRPC, or others.

## Request Model

- `ApiRequest`
- `AppCommand`

Commands currently cover:

- UI attach (`attach_ui`) with stable `host_id` and per-session `ui_session_id`
- startup probe (`probe_host_instance`) and structured ownership conflict semantics
- target listing
- profile detail read (`get_profile`) and upsert
- settings snapshot and update (`update_settings`)
- artifact cache clear
- sessions/approvals/diagnostics listing
- session opening
- command execution
- artifact reading
- approval request dispatch

For trusted local UI hosts, profile/settings writes are core-owned: the core
validates and persists changes, then returns `apply_strategy` (for example
`live_applied` or `restart_required`) so host state machines can drive managed
restart flows.

## Response Model

- `ApiResponse` includes success payloads and typed error payload.
- Each response returns `request_id` for correlation.

## Event Model

- `ApiEvent::SessionStateChanged`
- `ApiEvent::CommandTimelineEntry`
- `ApiEvent::ApprovalUpdated`

These events are intended for UI timeline rendering and sensitive operation
feedback.

## IPC Line Codec

`AppApiLineCodec` provides a transport-ready line protocol that can encode and
decode typed `ApiRequest`/`ApiResponse` payloads for local IPC adapters.

## Error Model

- `ApiError` carries a shared error contract:
  - `status`
  - `domain`
  - `common_code`
  - optional `module_code`
  - display-safe `message`
  - `retriable`
  - optional `recovery_hint`
- Error fields are intended to stay stable across local control-plane,
  standalone operator flows, and future UI/TUI hosts.

Canonical local operator surface coverage lives in:

- `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`
