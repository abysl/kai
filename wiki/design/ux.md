# Interface design

Audience: interface contributors and reviewers. Assumes familiarity with the
[player guide](../playing.md), not with Bevy or the game's engine.

## Organize around the player's task

The home screen selects a game or opens the deck library. The lobby prepares a
table. The table handles play. The deck library and editor have their own screens;
a deck-selection sheet should not become a second editor.

Keep one clear primary action for each state. If it cannot run, explain what is
missing beside it. Do not make a player interpret a module hash or connection
identifier to discover how to start.

## Screen size and input are separate

Viewport size determines arrangement. Input kind determines affordances.
A touch user needs selectable controls without hover; a mouse attached to a
small device does not turn that device into a desktop-sized viewport.

The viewport classifier lives in `src/viewport.rs`. Layout and theme tokens
have shared owners. Reuse them rather than copying threshold numbers or colors
into another panel.

## Selection, prompts, and errors

Selecting a card should make relevant actions visible. Dragging is an additional
way to request an action, not the only way. A prompt must explain who is expected
to act and how to answer. Inactive players should see that the game is waiting,
not an unexplained disabled interface.

Show refusals close to the attempted action. Preserve enough history to inspect
what happened, but do not leave obsolete errors in the way of the next decision.

## Navigation

Back/Escape closes the highest-priority overlay before leaving a screen. Keep
one navigation ladder rather than adding independent handlers that both consume
the same event. Preserve the origin when entering the deck library from a lobby
or table.

## Review checklist

Check narrow portrait, short landscape, tablet, and desktop layouts. Test
long names, missing art, an empty collection, touch input, text entry, and
both themes. Verify that the primary action and its disabled explanation
remain visible without covering the player's cards.

Screenshots demonstrate layout, not rules correctness. Pair them with tests
for pure layout/gesture decisions and the relevant interaction scenario.
