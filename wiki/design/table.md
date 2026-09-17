# Rendering and table interaction

Audience: renderer contributors who have read the [architecture overview](../architecture.md).
This is an implementation guide, not a player manual.

## State comes from the framework

The displayed table is a projection of Agni's accepted state. Input systems emit
requests such as `CardDropped`; they must not implement their own game-state
transition. Session routing belongs in `src/net/`.

Rendering can use floating point, frame time, easing, and camera interpolation.
Those values must not become inputs to the deterministic game decision.

## Ownership

`src/table/mod.rs` registers the table systems and shared resources.
`sync.rs` maps the accepted view to entities. `zones.rs` computes geometry
from game-declared zones; `layout.rs` places cards; `anim.rs` eases them toward
their targets. `interaction.rs` and `gesture.rs` classify player input.

A zone declaration supplies placement and layout information. Do not add a
game-specific card-location rule to the camera or renderer because one game
currently needs a special arrangement.

The visible seat can differ from the host's seat. Use the shared seat transforms
for positioning and orientation rather than assuming player zero faces the camera.

## Requests, refusals, and private information

Available-action controls come from the view. An unavailable action must remain
unavailable when invoked by a shortcut rather than a button. Refusals should be
shown near the attempted action and must not partially move the underlying card.

Hidden cards require checks from both the owner and other seats. Hiding one
image is insufficient if labels, previews, hover state, or a stale entity still
reveal the face.

Discard labels and an open prompt's counted trash buttons open a browse sheet
for either visible player's public discard.
Hovering a listed card or a chain thumbnail drives the inspector only when the
accepted view permits its face. Large prompt option sets open a searchable
selector over the existing affordances; its choices still emit those
affordances and never synthesize an intent.
Yes/no prompts reserve `1` for yes and `2` for no, regardless of a game's
legacy cancel hotkey. Kai normalizes that presentation hotkey while preserving
the offered request bytes.

## Forced answers and ordered prompts

Completed recycling prompts have no remaining card choice. When the only
enabled action is a randomness commitment, `auto::decide` sends it without the
normal pass delay, for every contributing seat regardless of prompt ownership.
This protocol step is independent of auto-pass and forced-choice preferences.
Reveals still use the existing commit/reveal path; the renderer does not pick
the shuffle order. Opening rolls and unfinished mulligan choices stay manual.

Agni's host dealer obtains fresh system entropy for shuffled deck groups. The
seeded CLI soak pre-shuffles its fixtures and clears each group's `shuffle`
flag before dealing, so the live-game randomness does not override a test seed.

`auto::offer` is the shared classification used by player automation and the
AI pilot. Requiring every option does not make an ordering prompt forced:
with multiple trigger choices, return `Offer::Choice`. The order can change
the result even though the selected set is identical. One remaining trigger
can still be answered automatically unless the player's settings ask to pause.

Dusk Rose Lab and Temporary are a regression case: placing Temporary first
lets the Lab resolve first and offer the unit as a sacrifice. Rule resolution
stays in the plugin; Kai must not choose that order on the player's behalf.

The rules plugin also publishes its computed Might as a counter delta from
printed Might after accepted actions. This includes conditional statics, buffs,
and combat modifiers. Kai displays that value without evaluating card scripts.

Token art uses the plugin's print ID, with a known-token name fallback for older
manifests. Catalog gaps use fixed public image URLs. Browser art polling retries
asynchronous loads while the table is idle; image bytes stay in runtime caches,
not in the source repository.

## Verify a visual change

Run at desktop and phone dimensions, using pointer and touch interactions.
Check the empty table, a full hand, an open prompt, and an overlay or menu.
The screenshot harness accepts `KAI_WINDOW=1280x800`, `KAI_SHOT=<path>`,
and `KAI_SHOT_FRAME=<frame>`; delay capture until the UI has settled.

Use the [UX reference](ux.md) for layout principles and the
[connectivity guide](../../tests/connectivity/README.md) for multi-client checks.
