# Prompt UI review

## Changed paths

- `src/table/chain.rs`
- `src/table/inspector.rs`
- `src/table/mod.rs`
- `src/table/plugin_ui.rs`
- `src/table/ui.rs`
- `wiki/design/table.md`

## Root cause

The compact chain rail cleared its hover target, discard rows could not supply an inspector target, and large prompt affordances were rendered as an unfiltered chip row. A plugin-supplied `x` hotkey also remained active for a no answer.

## Result

Chain and discard browsing now drive a visibility-checked inspector preview. Public discard stays limited to declared discard zones and uses the accepted face visibility. Large non-card prompt options use a searchable selector backed only by the existing affordances. Yes and no map to `1` and `2`; `x` no longer fires a no answer.

## Tests

- `cargo fmt --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/home/rae/atlas/orgs/abysl/projects/agni/kai/target CARGO_BUILD_JOBS=3 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked --lib table::plugin_ui::tests --no-fail-fast` is compiling the cold Bevy graph at the time of this summary.

## Remaining limitations

The selector deliberately leaves card-target prompts on the felt and existing faceless-card tray. It does not manufacture card choices or inspect hidden faces.
