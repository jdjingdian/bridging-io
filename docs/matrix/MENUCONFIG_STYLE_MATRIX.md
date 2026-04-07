# Menuconfig Style Matrix

This document is the canonical style matrix for `bridgingio-core menuconfig`
and any future trusted local UI/TUI host that intentionally reuses the same
menuconfig grammar.

## Scope

- `bridgingio-core menuconfig`
- future trusted local UI/TUI hosts that explicitly align to the same
  menuconfig row grammar, key semantics, popup contract, and fallback rules

## Principles

- Single-column, step-by-step navigation. No main-flow multi-column layout.
- Content area centered; text inside each row left-aligned.
- Prefixes are semantic, not decorative.
- `>` marks current focus on actionable rows only.
- `Esc` first closes the current popup, then backs out one level, then exits
  from root.
- Future menuconfig feature work must match this matrix before merge.

## Layout Matrix

| Area | Structure | Alignment | Focus / Interaction | Notes |
| --- | --- | --- | --- | --- |
| Main frame | bordered single-column screen | centered as a whole | menu list remains active while a footer button is also focusable | no split-pane primary layout |
| Menu list | vertical list of rows | block centered, row text left-aligned | `↑/↓` cycles across actionable rows only | non-focusable rows are skipped |
| Footer button bar | inline action bar | centered | `←/→` moves between `<Select> < Exit > < Help >`; `Enter` activates current footer button | focused footer button uses same selected feedback as menu rows |
| Status / description block | multi-line status panel | left-aligned inside block | read-only | may wrap |
| Popup overlay | centered modal overlay | content left-aligned unless button row requires centering | background menu is frozen | `Esc` closes current popup first |
| Resize guard | centered blocking overlay | centered or left-aligned prompt | normal navigation suspended until size recovers or operator exits | shown when viewport is smaller than minimum supported size |

## Row Grammar Matrix

| Row Kind | Focus Marker | Semantic Prefix | Canonical Grammar | Focusable | Primary Keys | Selected Feedback | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Read-only info / description | none | `---` | `--- Label` or `--- Label = Value` | no | none | none | navigation must skip |
| Fixed disabled feature | none | `- -` | `- - Label` | no | none | none | navigation must skip |
| Fixed enabled feature | none | `-*-` | `-*- Label` | no | none | none | navigation must skip |
| Single-choice toggle, off (no submenu) | `>` when selected | `< >` | `< > Label` | yes | `Space` toggles only | reverse or shared fallback | use when the row is pure on/off without follow-up view |
| Single-choice toggle, on (no submenu) | `>` when selected | `<*>` | `<*> Label` | yes | `Space` toggles only | reverse or shared fallback | pure on/off variant, no `--->` |
| Single-choice entry, off (with submenu) | `>` when selected | `< >` | `< > Label --->` or `< > Label (value) --->` | yes | `Space` toggles state; `Enter` follows `--->` | reverse or shared fallback | use when a row has both enable-state and next-step navigation |
| Single-choice entry, on (with submenu) | `>` when selected | `<*>` | `<*> Label --->` or `<*> Label (value) --->` | yes | `Space` toggles state; `Enter` follows `--->` | reverse or shared fallback | same semantics as off state |
| Multi-select option, off | `>` when selected | `[ ]` | `[ ] Label` | yes | `Space` toggles only | reverse or shared fallback | must not be combined with `--->` |
| Multi-select option, on | `>` when selected | `[*]` | `[*] Label` | yes | `Space` toggles only | reverse or shared fallback | must not be combined with `--->` |
| Text or enum field | `>` when selected | blank fixed-width prefix column | `Label (value) --->` | yes | `Enter` opens popup; `Space` may mirror entry only when explicitly intended | reverse or shared fallback | prefix column remains visually aligned |
| Submenu / action entry | `>` when selected | blank fixed-width prefix column | `Label --->` | yes | `Enter` opens submenu / popup / confirm chain | reverse or shared fallback | used for navigation and managed flows |
| Placeholder / empty-state hint | none | `---` | `--- Message` | no | none | none | search-empty / locked-hint / missing-object rows use read-only grammar |

## Field-To-Row Mapping (Current)

This table maps current menuconfig fields and entry families to canonical row
grammar. New menu fields should match one of these mappings.

| Scope | Field / Entry | Row Kind | Canonical Prefix | Canonical Grammar | Keys |
| --- | --- | --- | --- | --- | --- |
| Core | `core.instance_name` | Text field | blank prefix column | `Instance Name (value) --->` | `Enter` edits |
| Core | `core.log_level` | Enum field | blank prefix column | `Log Level (value) --->` | `Enter` opens choice popup |
| Core | `core.operator_locale` | Enum field | blank prefix column | `Core Locale (value) --->` | `Enter` opens choice popup |
| Core | `core.data_dir` | Read-only info | `---` | `--- Data Dir = value` | none |
| Storage | `storage.artifacts.backend` | Enum field | blank prefix column | `Artifact Backend (value) --->` | `Enter` opens choice popup |
| Storage | `storage.artifacts.max_bytes` | Text field | blank prefix column | `Artifact Max Bytes (value) --->` | `Enter` edits |
| Model Plane | `model_plane.http.host` | Text field | blank prefix column | `HTTP Host (value) --->` | `Enter` edits |
| Model Plane | `model_plane.http.port` | Text field | blank prefix column | `HTTP Port (value) --->` | `Enter` edits |
| Model Plane | `model_plane.http.allow_non_loopback` | Single-choice toggle (no submenu) | `< >` / `<*>` | `< > Allow Non Loopback` / `<*> Allow Non Loopback` | `Space` toggles, `Enter` does not toggle |
| Vault | `vault.unlock.trigger_policy` | Enum field | blank prefix column | `Unlock Trigger (value) --->` | `Enter` opens choice popup |
| Vault | `vault.backend` / `vault.unlock.preferred_method` / `vault.unlock.allowed_methods` | Read-only info | `---` | `--- Label = value` | none |
| Targets | `targets[*].enabled` | Multi-select toggle | `[ ]` / `[*]` | `[ ] Enabled` / `[*] Enabled` | `Space` toggles, `Enter` does not toggle |
| Targets | `targets[*].id` / `display_name` / `aliases` / connection fields | Text field | blank prefix column | `Label (value) --->` | `Enter` edits |
| Targets | `Add Target` | Submenu/action entry | blank prefix column | `Add Target --->` | `Enter` follows mode-first flow |
| Targets | `Add Target -> Choose Storage Mode` | Submenu/action entry | blank prefix column | `Plain Target --->` / `Sensitive Target --->` | `Enter` follows; sensitive path is replaced by `Unlock Vault --->` while vault is locked |
| Targets | `sensitive target detail (vault locked)` | Placeholder/read-only | `---` + action row | `--- Sensitive overlay is locked` + `Unlock Vault --->` | read-only + unlock action only |
| Targets | `SSH target -> SSH Authentication` | Submenu/action entry | blank prefix column | `SSH Authentication --->` | plain SSH row lives in `Connection Profile`; sealed SSH row lives in unlocked `Sensitive Overlay` |
| Targets | `SSH Authentication Setup` auth kind picks | Single-choice toggle (`none`) + single-choice entry (`password`/`private-key`) + info rows | `< >` / `<*>` / `---` | `<*> Use none`, `< > Use password --->`, `< > Use private-key --->`, plus `--- Authentication Kind = value` | 三者互斥且必须单选其一；`Space` 切换认证类型，`Enter` 进入已选类型的后续输入；plain password selection requires risk confirm popup |
| Targets | `SSH Authentication Setup` secure access | Single-choice toggle (plain) or read-only info (sealed) | `< >` / `<*>` or `---` | plain: `SSH Secure Access = true/false --->`; sealed: `--- SSH Secure Access = required` | plain secret-backed auth defaults `true` and can toggle; sealed secret-backed auth cannot toggle |
| Targets | `SSH Authentication Setup` private-key branch | 条件子流程（plain 仅本地路径；sealed 支持 local/vault）+ text field | blank prefix column | plain: `Local SSH Key Path (value) --->`; sealed: `Use Local Key Path --->`, `Use Imported Vault Key --->`, `Local SSH Key Path (value) --->`, `Use Imported Vault SSH Key --->`, `Import Local SSH Key Into Vault --->` | local key path edit is blocked immediately when passphrase-protected key is detected; `Continue To Detail Editor` stays disabled until key input is valid |
| Targets | `SSH target -> Test Connection` | Submenu/action entry | blank prefix column | `Test Connection --->` | `Enter` opens timeout input popup; plain SSH row lives in `Connection Profile`; sealed SSH row lives in `Sensitive Overlay` and is hidden while vault is locked |
| Security | lock/backend/count summary rows | Read-only info | `---` | `--- Label = value` | none |
| Security | unlocked notice | Fixed enabled feature | `-*-` | `-*- Credential Management Unlocked` | none |
| Security | `Init/Unlock/Import SSH Key/SSH Key Management/Create Token/Token Management/Delete Vault` | Submenu/action entry | blank prefix column | `Label --->` | `Enter` follows action flow; locked state shows aggregate `SSH Key Count` only |
| Security | `SSH Key Management` list/detail | Submenu/action entry + info rows | blank prefix column / `---` | list rows `Label [status] record-id --->`; detail rows `--- Field = value` + `Delete SSH Key --->` | detail/list are unlocked-only and display-safe only |
| Token Detail | access switch (`enabled` / `disabled`) | Single-choice toggle (no submenu) | `< >` / `<*>` | `< > Access Switch = [disabled]` / `<*> Access Switch = [enabled]` | `Space` toggles, `Enter` does not toggle |
| Token Detail | label edit | Submenu/action entry | blank prefix column | `Label = value --->` | `Enter` opens edit popup |
| Token Detail | fingerprint/expiry/status/revoke reason | Read-only info | `---` | `--- Label = value` | none |
| Token Detail | permissions/revoke/delete actions | Submenu/action entry | blank prefix column | `Label --->` | `Enter` follows action/confirm flow |
| Search Results | matched editable field | Submenu/action entry | blank prefix column | `Label (value) --->` | `Enter` navigates/focuses |
| Search Results | no-query / no-match hints | Placeholder/read-only | `---` | `--- Message` | none |

## Popup Matrix

| Popup Type | Layout | Focusable Elements | Keys | Selected Feedback | Notes |
| --- | --- | --- | --- | --- | --- |
| Confirm popup | centered modal with message and inline button row | buttons only | `←/→` move; `Enter` confirms current button; `Esc` cancels | current button must use same reverse / fallback as menu rows | applies to save/discard, delete, revoke, etc. |
| Choice popup | centered modal with vertical options list | list rows | `↑/↓` move; `Enter` or `Space` confirms; `Esc` cancels | current row must use same reverse / fallback as menu rows | single-choice editor |
| Text input popup | centered modal with one input row | input line | `Left/Right` move caret; `Backspace` deletes left char; `Enter` commits; `Esc` cancels | caret must be visible | all text fields share one editing contract |
| SSH test timeout popup | centered modal with one input row | input line | `Left/Right` move caret; `Backspace` deletes left char; `Enter` starts probe; `Esc` cancels | caret must be visible | default timeout is `2000` milliseconds |
| SSH test waiting popup | centered modal waiting state | none | `Esc` cancels pending probe | none required | background menu navigation is blocked while waiting |
| SSH test result popup | centered modal with short status text | none | `Enter` or `Esc` closes | none required | result text is concise only (`succeeded`/`failed`/`cancelled`/`timed out`) |
| Hidden passphrase popup | centered modal with one masked input row | input line | `Left/Right` move caret; `Backspace` deletes left char; `Enter` commits; `Esc` cancels | caret visible; characters rendered as mask glyph | used for encrypted SSH key import passphrase capture |
| Help popup | centered modal with read-only content | none | `Esc` closes; help shortcut may toggle | none required | read-only overlay |
| One-time reveal popup | centered modal with read-only sensitive result | none or one acknowledge button | `Enter` / `Esc` closes according to concrete flow | if an acknowledge button exists, it follows shared selected feedback | closing the popup ends the one-time reveal path |
| Risk confirm popup | centered modal with message and inline button row | buttons only | `←/→` move; `Enter` confirms current button; `Esc` cancels | current button must use same reverse / fallback as menu rows | used by `plain + ssh` create flow and by plain `password` auth selection |
| SSH auth blocked popup | centered modal with policy message | none | `Enter` or `Esc` closes | none required | shown when local SSH key path is passphrase-protected and blocked by policy |
| Resize-required popup | centered blocking overlay | optional exit button only | normal navigation blocked; `Esc` may exit from root flow | if a button exists, it follows shared selected feedback | shown for viewport smaller than minimum supported size |

## Navigation And Key Matrix

| Context | `↑/↓` | `←/→` | `Space` | `Enter` | `Esc` |
| --- | --- | --- | --- | --- | --- |
| Main menu list | move across actionable rows only, with wrap | footer button focus | toggle `[ ]/[*]` and `< >/<*>` state when applicable | follow `--->` only; no-arrow single-choice rows do not toggle on `Enter` | back one level, or exit from root |
| Footer button bar | no effect on list index | move across footer buttons | no semantic action | activate current footer button | same as screen-level `Esc` |
| Confirm popup | none | move across popup buttons | optional alias of confirm only when explicitly allowed | activate current popup button | close/cancel popup |
| Choice popup | move across choice rows | none | confirm current choice | confirm current choice | close/cancel popup |
| Text input popup | none | move caret | insert space if text input allows it | commit input | close/cancel popup |
| SSH test waiting popup | none | none | none | none | cancel probe and close waiting popup |
| SSH test result popup | none | none | none | close result popup | close result popup |

## Minimum Viewport And Overflow Rules

- Minimum supported viewport for the canonical menu layout is `80x24`.
- Below `80x24`, the UI must show a resize-required overlay instead of trying
  to render the normal menu layout.
- Menu rows must remain single-line. They must never wrap onto multiple lines.
- When a row is too long, preserve:
  - the left focus marker column
  - the semantic prefix column
  - the right-side `--->` suffix when present
- Truncate the middle text with `...` or an equivalent controlled ellipsis.
- Status/help text may wrap within their own blocks.

## Known Current Deviations

No known deviations currently remain for change
`standardize-menuconfig-style-contract`.

## Maintenance Rules

- Any menuconfig feature change that alters row grammar, popup behavior, key
  semantics, fallback rendering, viewport rules, or focus behavior must update
  this matrix and the corresponding OpenSpec spec.
- `LOCAL_OPERATOR_INTERFACE_MATRIX` records what menuconfig can do; this
  document records how menuconfig must look and behave while doing it.
- A change that introduces or modifies menuconfig rows, popups, or key
  semantics must not be considered complete until this matrix is aligned with
  the implementation.
- The change `standardize-menuconfig-style-contract` must not be
  archived while the `Known Current Deviations` list remains non-empty.
