# Archive Handoff Rules

After each OpenSpec change archive, produce:

1. a commit message proposal that conforms to `.gitmessage`
2. a pull request description draft aligned with proposal/design/tasks/tests

## Commit Message Contract

Use:

```text
<type>(<scope>): <subject>
```

Rules from `.gitmessage`:

- imperative mood
- lower-case first letter in subject
- no trailing period in subject
- subject length <= 72 chars

## Pull Request Description Contract

PR description must include:

- what changed
- why the change is needed
- key implementation or design decisions
- test and validation status
- follow-up or risk notes when relevant

## Recommended Template

```markdown
## Summary
[what changed]

## Why
[motivation and problem]

## Implementation Notes
[major decisions]

## Testing
[unit, integration, ui test coverage and outcomes]

## Risks / Follow-ups
[optional]
```

