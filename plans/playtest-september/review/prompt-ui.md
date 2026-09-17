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
- Attempted one offline wasm check with main's cache, session `21549`. It began
  recompiling host-side dependencies, so it was stopped (exit 143) before reaching
  Kai to avoid a cold build alongside main's ongoing native build. No check or
  test session remains active from this follow-up. No native build was started.

Command, run from the isolated worktree:

```sh
env PATH=/nix/store/dibkflwsv77qvhxginzph0yhbfivva59-rust-stable-1.98.0-1.98.0/bin:$PATH CARGO_TARGET_DIR=/tmp/kai-dusk-rose.adTLQw/wasm-target CARGO_BUILD_JOBS=3 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo check --locked --offline --lib --target wasm32-unknown-unknown
```

## Remaining limitations

The new helper tests still need execution in main's unified test run; compilation
and interactive phone testing are not verified here. Main's separately fixed
inspector closure remains untouched. Images depend on the existing art cache;
unoffered actions remain unavailable. This focused follow-up does not alter the
already-integrated selector, hotkey or chain-preview work.
