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

## Verify a visual change

Run at desktop and phone dimensions, using pointer and touch interactions.
Check the empty table, a full hand, an open prompt, and an overlay or menu.
The screenshot harness accepts `KAI_WINDOW=1280x800`, `KAI_SHOT=<path>`,
and `KAI_SHOT_FRAME=<frame>`; delay capture until the UI has settled.

Use the [UX reference](ux.md) for layout principles and the
[connectivity guide](../../tests/connectivity/README.md) for multi-client checks.
