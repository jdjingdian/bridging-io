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
- Screen implementations must choose a template first, then fill data.
- Screen implementations must not handcraft semantic prefixes or `--->`.

## Template-First Rules

- Any new row must map to one `row template`.
- Any mutually-exclusive option family must map to one `group template`.
- Any popup or overlay must map to one `popup template`.
- Template-level key semantics override screen-local conventions.
- `boolean-toggle-row` and `exclusive-choice-row` are different semantics and
  must never be merged into one ambiguous type.
- `blocked-action-row` and `required-readonly-row` must stay visually and
  behaviorally distinct from normal info rows and action rows.

## Row Template Catalog

| Template ID | Intent | Focusable | Canonical Grammar | Key Contract | Notes |
| --- | --- | --- | --- | --- | --- |
| `info-row` | Read-only status/description | no | `--- Label` or `--- Label = Value` | none | navigation must skip |
| `fixed-disabled-row` | Permanently disabled feature projection | no | `- - Label` | none | not an actionable control |
| `fixed-enabled-row` | Permanently enabled feature projection | no | `-*- Label` | none | not an actionable control |
| `boolean-toggle-row` | Pure boolean on/off, no follow-up page | yes | `< > Label` / `<*> Label` | `Space` toggles; `Enter` must not toggle | must not carry `--->` |
| `exclusive-choice-row` | One option inside an exclusive group | yes | `< > Label` / `<*> Label` | `Space` toggles group selection; `Enter` does not change selection | used with `exclusive-choice-group` |
| `exclusive-choice-entry-row` | Exclusive option that also owns a detail entry | yes | `< > Label --->` / `<*> Label --->` | `Space` toggles group selection; `Enter` enters detail of current selected option | used with `exclusive-choice-group` |
| `multi-select-row` | Independent multi-select option | yes | `[ ] Label` / `[*] Label` | `Space` toggles only | must not carry `--->` |
| `field-entry-row` | Field editor entry (text/enum/etc.) | yes | `Label (value) --->` | `Enter` opens editor popup | prefix column kept aligned and blank |
| `action-row` | Navigation/action entry | yes | `Label --->` | `Enter` opens flow | used for submenu and managed actions |
| `blocked-action-row` | Action exists but currently blocked | no | `--- Label (blocked: reason)` | none | reason must be visible inline |
| `required-readonly-row` | Policy/system-enforced fixed state | no | `--- Label = required` | none | must not respond to toggle keys |

## Group Template Catalog

| Template ID | Intent | Allowed Row Templates | Group Contract | Key Contract |
| --- | --- | --- | --- | --- |
| `exclusive-choice-group` | Mutually-exclusive mode/strategy chooser | `exclusive-choice-row`, `exclusive-choice-entry-row` | exactly one option selected at any time | `Space` changes selected option; `Enter` only enters selected option detail when supported |
| `selection-picker-group` | Mutually-exclusive object picker (key/profile/backend) | `exclusive-choice-row`, `exclusive-choice-entry-row` | exactly one option selected; selection binds to external object reference | same as `exclusive-choice-group`; picker outcome must be persisted |

## Popup Template Catalog

| Template ID | Intent | Primitive Composition | Key Contract | Notes |
| --- | --- | --- | --- | --- |
| `message-modal` | Read-only policy/help/info popup | `modal-shell` + body text (+ optional `button-row`) | `Esc` closes; `Enter` closes when acknowledge exists | used by help/auth-block/reveal-like read-only notices |
| `confirm-modal` | Confirm/cancel decision popup | `modal-shell` + body + `button-row` | `Left/Right` focus button; `Enter` confirms current button; `Esc` cancels | used for delete/revoke/risk/exit confirms |
| `text-input-modal` | One-line text input editor | `modal-shell` + `input-line` (+ optional `button-row`) | `Left/Right` move caret; `Backspace` deletes; `Enter` commits; `Esc` cancels | used by generic edit and timeout input flows |
| `choice-list-modal` | One-of-many option picker | `modal-shell` + `choice-list` | `Up/Down` move; `Enter` or `Space` confirms; `Esc` cancels | used by enum/choice editing |
| `waiting-modal` | In-progress blocking state | `modal-shell` + waiting body | usually `Esc` cancel when operation supports it | background menu is frozen |
| `result-modal` | Operation result acknowledgement | `modal-shell` + result body (+ optional `button-row`) | `Enter`/`Esc` acknowledge and close | used by unlock/test results |
| `one-time-reveal-modal` | Sensitive value reveal with explicit close | `modal-shell` + reveal body (+ optional `button-row`) | `Enter`/`Esc` closes by flow contract | closing ends reveal lifetime |
| `blocking-overlay` | Hard gate overlay (resize/policy lock) | `modal-shell` + blocking message (+ optional `button-row`) | normal navigation disabled until resolved/exit | used when menu cannot safely continue |

## Popup Primitive Mapping

| Primitive | Responsibility | Shared Contract |
| --- | --- | --- |
| `modal-shell` | centered rect + clear + bordered container + title/body slots | all popup templates use one shell layout baseline |
| `button-row` | horizontal button strip with one active button | selected button uses same fallback highlighting as menu rows |
| `choice-list` | vertical selectable options list | active option uses same fallback highlighting as menu rows |
| `input-line` | editable one-line input with caret | caret always visible, left/right/backspace contract stable across editors |

## Layout Matrix

| Area | Structure | Alignment | Focus / Interaction | Notes |
| --- | --- | --- | --- | --- |
| Main frame | bordered single-column screen | centered as a whole | menu list remains active while a footer button is also focusable | no split-pane primary layout |
| Menu list | vertical list of rows | block centered, row text left-aligned | `↑/↓` cycles across actionable rows only | non-focusable rows are skipped |
| Footer button bar | inline action bar | centered | `←/→` moves between `<Select> < Exit > < Help >`; `Enter` activates current footer button | focused footer button uses same selected feedback as menu rows |
| Status / description block | multi-line status panel | left-aligned inside block | read-only | may wrap |
| Popup overlay | centered modal overlay | content left-aligned unless button row requires centering | background menu is frozen | `Esc` closes current popup first |
| Resize guard | centered blocking overlay | centered or left-aligned prompt | normal navigation suspended until size recovers or operator exits | shown when viewport is smaller than minimum supported size |

## Row Grammar Matrix (Template-Derived)

| Row Template | Focus Marker | Semantic Prefix | Canonical Grammar | Focusable | Primary Keys | Selected Feedback | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `info-row` | none | `---` | `--- Label` or `--- Label = Value` | no | none | none | navigation must skip |
| `fixed-disabled-row` | none | `- -` | `- - Label` | no | none | none | navigation must skip |
| `fixed-enabled-row` | none | `-*-` | `-*- Label` | no | none | none | navigation must skip |
| `boolean-toggle-row` (off/on) | `>` when selected | `< >` / `<*>` | `< > Label` / `<*> Label` | yes | `Space` toggles only | reverse or shared fallback | no `--->` |
| `exclusive-choice-row` (off/on) | `>` when selected | `< >` / `<*>` | `< > Label` / `<*> Label` | yes | `Space` toggles group selection | reverse or shared fallback | must be inside a group template |
| `exclusive-choice-entry-row` (off/on) | `>` when selected | `< >` / `<*>` | `< > Label --->` / `<*> Label --->` | yes | `Space` toggles group selection; `Enter` enters detail | reverse or shared fallback | selection and detail are split responsibilities |
| `multi-select-row` (off/on) | `>` when selected | `[ ]` / `[*]` | `[ ] Label` / `[*] Label` | yes | `Space` toggles only | reverse or shared fallback | must not be combined with `--->` |
| `field-entry-row` | `>` when selected | blank fixed-width prefix column | `Label (value) --->` | yes | `Enter` opens popup | reverse or shared fallback | prefix column remains visually aligned |
| `action-row` | `>` when selected | blank fixed-width prefix column | `Label --->` | yes | `Enter` opens submenu / popup / confirm chain | reverse or shared fallback | used for navigation and managed flows |
| `blocked-action-row` | none | `---` | `--- Label (blocked: reason)` | no | none | none | must expose blocking reason |
| `required-readonly-row` | none | `---` | `--- Label = required` | no | none | none | fixed by policy/system |
| `info-row` empty-state variant | none | `---` | `--- Message` | no | none | none | search-empty / locked-hint / missing-object rows |

## Field-To-Template Mapping (Current)

This table maps current menuconfig fields and entry families to canonical row
grammar. New menu fields should match one of these mappings.

| Scope | Field / Entry | Template Mapping | Canonical Prefix | Canonical Grammar | Keys |
| --- | --- | --- | --- | --- | --- |
| Core | `core.instance_name` | `field-entry-row` | blank prefix column | `Instance Name (value) --->` | `Enter` edits |
| Core | `core.log_level` | `field-entry-row` + `choice-list-modal` | blank prefix column | `Log Level (value) --->` | `Enter` opens choice popup |
| Core | `core.operator_locale` | `field-entry-row` + `choice-list-modal` | blank prefix column | `Core Locale (value) --->` | `Enter` opens choice popup |
| Core | `core.data_dir` | `info-row` | `---` | `--- Data Dir = value` | none |
| Storage | `storage.artifacts.backend` | `field-entry-row` + `choice-list-modal` | blank prefix column | `Artifact Backend (value) --->` | `Enter` opens choice popup |
| Storage | `storage.artifacts.max_bytes` | `field-entry-row` + `text-input-modal` | blank prefix column | `Artifact Max Bytes (value) --->` | `Enter` edits |
| Model Plane | `model_plane.http.host` | `field-entry-row` + `text-input-modal` | blank prefix column | `HTTP Host (value) --->` | `Enter` edits |
| Model Plane | `model_plane.http.port` | `field-entry-row` + `text-input-modal` | blank prefix column | `HTTP Port (value) --->` | `Enter` edits |
| Model Plane | `model_plane.http.allow_non_loopback` | `boolean-toggle-row` | `< >` / `<*>` | `< > Allow Non Loopback` / `<*> Allow Non Loopback` | `Space` toggles, `Enter` does not toggle |
| Vault | `vault.unlock.trigger_policy` | `field-entry-row` + `choice-list-modal` | blank prefix column | `Unlock Trigger (value) --->` | `Enter` opens choice popup |
| Vault | `vault.backend` / `vault.unlock.preferred_method` / `vault.unlock.allowed_methods` | `info-row` | `---` | `--- Label = value` | none |
| Targets | `targets[*].enabled` | `multi-select-row` | `[ ]` / `[*]` | `[ ] Enabled` / `[*] Enabled` | `Space` toggles, `Enter` does not toggle |
| Targets | `targets[*].id` / `display_name` / `aliases` / connection fields | `field-entry-row` | blank prefix column | `Label (value) --->` | `Enter` edits |
| Targets | `Add Target` | `action-row` | blank prefix column | `Add Target --->` | `Enter` follows mode-first flow |
| Targets | `Add Target -> Choose Storage Mode` | `action-row` | blank prefix column | `Plain Target --->` / `Sensitive Target --->` | `Enter` follows; sensitive path is replaced by `Unlock Vault --->` while vault is locked |
| Targets | `sensitive target detail (vault locked)` | `info-row` + `action-row` | `---` + blank prefix column | `--- Sensitive overlay is locked` + `Unlock Vault --->` | read-only + unlock action only |
| Targets | `SSH target -> SSH Authentication` | `action-row` | blank prefix column | `SSH Authentication --->` | plain SSH row lives in `Connection Profile`; sealed SSH row lives in unlocked `Sensitive Overlay` |
| Targets | `SSH Authentication Setup` auth kind picks | `exclusive-choice-group` with `exclusive-choice-row` (`none`) + `exclusive-choice-entry-row` (`password`/`private-key`) + `info-row` | `< >` / `<*>` / `---` | `<*> Use none`, `< > Use password --->`, `< > Use private-key --->`, plus `--- Authentication Kind = value` | exactly one auth kind selected; `Space` switches choice; `Enter` enters selected detail; plain password selection requires `confirm-modal` |
| Targets | `SSH Authentication Setup` secure access | plain: `boolean-toggle-row`; sealed: `required-readonly-row` | `< >` / `<*>` or `---` | plain: `SSH Secure Access = true/false --->`; sealed: `--- SSH Secure Access = required` | plain secret-backed auth defaults `true` and can toggle; sealed secret-backed auth cannot toggle |
| Targets | `SSH Authentication Setup` private-key branch | `action-row` + `field-entry-row` + `blocked-action-row` | blank prefix column / `---` | plain: `Local SSH Key Path (value) --->`; sealed: `Use Local Key Path --->`, `Use Imported Vault Key --->`, `Local SSH Key Path (value) --->`, `Use Imported Vault SSH Key --->`, `Import Local SSH Key Into Vault --->`; blocked: `--- Continue To Detail Editor (blocked: key input required)` | local key path edit is blocked immediately when passphrase-protected key is detected; continue row remains blocked until input is valid |
| Targets | `SSH target -> Test Connection` | `action-row` + `text-input-modal` + `waiting-modal` + `result-modal` | blank prefix column | `Test Connection --->` | `Enter` opens timeout input popup; plain SSH row lives in `Connection Profile`; sealed SSH row lives in `Sensitive Overlay` and is hidden while vault is locked |
| Security | lock/backend/count summary rows | `info-row` | `---` | `--- Label = value` | none |
| Security | unlocked notice | `fixed-enabled-row` | `-*-` | `-*- Credential Management Unlocked` | none |
| Security | `Init/Unlock/Import SSH Key/SSH Key Management/Create Token/Token Management/Delete Vault` | `action-row` | blank prefix column | `Label --->` | `Enter` follows action flow; locked state shows aggregate `SSH Key Count` only |
| Security | `SSH Key Management` list/detail | `action-row` + `info-row` | blank prefix column / `---` | list rows `Label [status] record-id --->`; detail rows `--- Field = value` + `Delete SSH Key --->` | detail/list are unlocked-only and display-safe only |
| Token Detail | access switch (`enabled` / `disabled`) | `boolean-toggle-row` | `< >` / `<*>` | `< > Access Switch = [disabled]` / `<*> Access Switch = [enabled]` | `Space` toggles, `Enter` does not toggle |
| Token Detail | label edit | `field-entry-row` + `text-input-modal` | blank prefix column | `Label = value --->` | `Enter` opens edit popup |
| Token Detail | fingerprint/expiry/status/revoke reason | `info-row` | `---` | `--- Label = value` | none |
| Token Detail | permissions/revoke/delete actions | `action-row` + `confirm-modal` | blank prefix column | `Label --->` | `Enter` follows action/confirm flow |
| Search Results | matched editable field | `field-entry-row` or `action-row` | blank prefix column | `Label (value) --->` | `Enter` navigates/focuses |
| Search Results | no-query / no-match hints | `info-row` | `---` | `--- Message` | none |

## Popup Template Matrix

| Popup Template | Layout | Focusable Elements | Keys | Selected Feedback | Notes |
| --- | --- | --- | --- | --- | --- |
| `confirm-modal` | centered modal with message and inline button row | buttons only | `←/→` move; `Enter` confirms current button; `Esc` cancels | current button must use same reverse / fallback as menu rows | applies to save/discard, delete, revoke, risk confirm |
| `choice-list-modal` | centered modal with vertical options list | list rows | `↑/↓` move; `Enter` or `Space` confirms; `Esc` cancels | current row must use same reverse / fallback as menu rows | single-choice editor |
| `text-input-modal` | centered modal with one input row | input line | `Left/Right` move caret; `Backspace` deletes left char; `Enter` commits; `Esc` cancels | caret must be visible | all text fields and timeout editors share one contract |
| `waiting-modal` | centered modal waiting state | none | `Esc` cancels pending operation when supported | none required | background menu navigation is blocked while waiting |
| `result-modal` | centered modal with short status text | none or one acknowledge button | `Enter` or `Esc` closes | if a button exists, it follows shared selected feedback | result text is concise (`succeeded`/`failed`/`cancelled`/`timed out`) |
| `message-modal` | centered modal with read-only content | none or one acknowledge button | `Esc` closes; `Enter` closes when acknowledge exists | if a button exists, it follows shared selected feedback | used by help and auth-block style notices |
| `one-time-reveal-modal` | centered modal with read-only sensitive result | none or one acknowledge button | `Enter` / `Esc` closes according to flow | if a button exists, it follows shared selected feedback | closing ends one-time reveal path |
| `blocking-overlay` | centered blocking overlay | optional exit button only | normal navigation blocked; `Esc` may exit from root flow | if a button exists, it follows shared selected feedback | shown for viewport smaller than minimum supported size |

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
`abstract-menuconfig-row-templates`.

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
