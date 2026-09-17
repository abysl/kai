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

Chain and discard browsing now drive a visibility-checked inspector preview. Public discard stays limited to declared discard zones and uses the accepted face visibility. Large prompt options use a searchable selector backed only by the existing affordances. Yes and no map to `1` and `2`; `x` no longer fires a no answer.

The follow-up groups inspector inputs in one `SystemParam`, gives an explicit
chain or pile hover priority over stale selected cards unless an inspector pin
is active, and adds counted trash buttons while a prompt is open. The selector
now includes enabled card and non-card prompt options. Its matching uses the
public visible card name plus matching catalog group names, tags, and rules
text; The List's offered tag labels remain the authoritative selectable list.
Yes/no prompt hotkeys are normalized in Kai so dispatch, visual labels,
keyboard claiming, cancel detection, and strip digit handling all use `1`/`2`.
Selector metadata terms are cached per prompt and catalog generation, and are
only built after a non-empty search. Card options may match public catalog tags
and rules text; hidden card faces contribute only the face-down label.

## Tests

- `cargo fmt --check`
- `git diff --check`
- Native tests require the renderer dependency graph to finish compiling; the
  release integration runs the full library suite, including the prompt tests.

## Remaining limitations

The selector does not manufacture choices or inspect hidden faces. Existing felt
and faceless-card-tray affordances remain available alongside its large-option
list.
