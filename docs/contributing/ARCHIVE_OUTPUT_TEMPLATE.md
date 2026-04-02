# Archive Output Template

Use this template after `openspec archive` for any change.

## 1. Commit Message Proposal

Read `.gitmessage` first, then produce at least one commit title candidate:

```text
<type>(<scope>): <subject>
```

Rules:

- imperative mood
- lower-case first letter in subject
- no trailing period
- subject <= 72 chars

Example:

```text
feat(openspec): add logical session reuse and multi-channel tracking
```

## 2. PR Description Template

The PR text must be grounded in current change context, not generic boilerplate.
Reference:

- `proposal.md` (why)
- `design.md` (decisions and tradeoffs)
- `tasks.md` (implemented scope)
- `specs/**/*.md` (requirements)
- matrix docs when relevant (`docs/matrix/SELF_TEST_CASE_MATRIX.md`, `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`)
- latest test/validation results

Suggested structure:

```markdown
## Summary
- [key implemented outcomes]

## Why
- [problem and motivation from proposal]

## Design Notes
- [important architecture/policy decisions]

## Scope Mapping
- [completed tasks or requirement IDs]

## Testing
- [unit tests]
- [integration tests]
- [UI tests or UI test contract]
- [validation commands and results]

## Risks / Follow-ups
- [known limits and next steps]
```
