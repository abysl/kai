# Prompt UI: phone trash preview and offered actions

## Changed paths

- `src/table/ui.rs`
- `wiki/design/table.md`
- `plans/playtest-september/review/prompt-ui.md`

## Root cause

Trash taps selected an inspector target, but phones have no inspector rectangle.
The full-screen trash sheet therefore showed no preview. Its rows also lacked
buttons for the plugin's already-offered card actions.

## Result

Trash rows are touch-sized inspection buttons. On phones, the selected row shows
its image inline, bounded by the available width and viewport height. Image
lookup requires a public discard zone and a revealed, non-hidden face, for either
seat. Missing art shows an explicit unavailable label.

Separate row buttons expose shown, enabled plain card affordances, including
Reflow or prompt answers when offered. Buttons use `hud::Sender` with the original
affordance; no moves or request bytes are manufactured. Hidden, disabled,
other-card, protocol-only and menu-only options are excluded. Another seat's
prompt and ended sessions suppress row actions but not inspection.

The system has fourteen parameters, including the existing HUD art and sender
SystemParams. No inspector, rules, networking, art-loader, dependency, lockfile,
ABI or version changes.

## Tests and checks

- Added helper tests for public revealed previews for both seats, concealed faces
  and private-zone rejection, enabled/offered action filtering and unchanged
  payloads, prompt ownership, and portrait/landscape preview bounds.
- `rustfmt --edition 2021 --check src/table/ui.rs` and `git diff --check` pass.
- Native tests require the renderer dependency graph to finish compiling; the
  release integration runs the full library suite, including the prompt tests.

## Remaining limitations

The new helper tests still need execution in main's unified test run; compilation
and interactive phone testing are not verified here. Main's separately fixed
inspector closure remains untouched. Images depend on the existing art cache;
unoffered actions remain unavailable. This focused follow-up does not alter the
already-integrated selector, hotkey or chain-preview work.
