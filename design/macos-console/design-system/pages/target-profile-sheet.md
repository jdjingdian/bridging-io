# Target Profile Sheet

## Purpose

Structured editor for all target types. This is the main settings surface for MVP and future expansion, not an afterthought.

The editor must inherit the active workspace appearance mode:

- if the workspace is in `Dark`, the editor is also `Dark`
- if the workspace is in `Light`, the editor is also `Light`
- if the workspace follows system appearance, the editor follows it too

## Entry Points

- toolbar `New Target`
- target row context action `Edit`
- diagnostic panel action `Override Tool Path`

## Structure

```text
┌──────────────────────────────────────────────────────┐
│ Edit Target                                   Save  │
├──────────────────────────────────────────────────────┤
│ Types: [SSH] [ADB] [Serial] [Docker] [HTTP] [OpenGrok]
│                                                      │
│ General                                              │
│ Name                [ ops-prod                    ]  │
│ Kind                [ SSH v ]                       │
│ Alias for model     [ prod-bastion               ]  │
│ Notes               [ production jump host       ]  │
│                                                      │
│ Connection                                           │
│ Host                [ 10.0.0.8                   ]  │
│ Port                [ 22                         ]  │
│ Username            [ ops                        ]  │
│                                                      │
│ Credential                                           │
│ Key reference       [ vault:ssh-key:ops-prod v  ]  │
│ Import new key ref   [Import] [Reveal in Vault]     │
│                                                      │
│ Tooling                                              │
│ SSH source          system PATH                      │
│ Override path       [ /custom/bin/ssh           ]  │
│                                                      │
│ Policy                                               │
│ [x] approve write  [x] approve delete                │
│ [x] approve sudo   [x] approve sensitive read        │
└──────────────────────────────────────────────────────┘
```

## Rules

- No free-form command string as the primary editing model
- Credential selection must show references, never secret values
- Alias field is operator-controlled metadata intended for model-facing naming
- Tool source row must show effective source and any pending override
- Keep shared fields visually stable while only the type-specific module changes
- The selected target kind tab must always match the currently visible module content
- Do not include a `Type Preview Matrix` in the actual editor UI; it is explanatory noise during editing
- Use the freed space for current-type helper content, validation hints, sample values, or diagnostics related to the selected target type

## Shared Fields

- name
- kind
- alias for model
- notes
- credential reference
- tooling diagnostics and overrides
- policy defaults

## Type-Specific Modules

### SSH

- host
- port
- username

### ADB

- serial
- transport

### Serial

- device path
- baud rate
- optional framing or parity hint

### Docker

- container name
- context
- shell preference

### HTTP Debug

- base URL
- auth reference
- environment preset
- request template or headers preset

### OpenGrok

- endpoint
- repository scope
- search API token reference
- default query scope

## Replacing The Matrix

Instead of a preview matrix for all target kinds, the right-side or lower support area should show only context for the active target kind. Examples:

- SSH: connection test hint, bastion note, host key or tool source diagnostics
- ADB: serial discovery status, transport hint, detected device info
- Serial: device path validation, baud-rate guidance
- Docker: container discovery and context check
- HTTP Debug: environment preset, auth ref state, sample request hint
- OpenGrok: endpoint health, repo scope hint, token ref status
