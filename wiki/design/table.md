# kai — The Card Table

> Status: implemented, minimally. Seven cards in hand, pick one up, drop it on
> the table. Everything below that is not marked *open* is in `kai`.

The renderer is game-agnostic on purpose. It knows about cards, zones and a
table surface; it knows nothing about Magic, Riftbound, or any other game. A
game supplies a `GameTable` and reads back `CardDropped`.

Card art arrives over spirit, peer to peer. [peers.md](peers.md) covers the
debug panel over that exchange and — more usefully — what iroh can and cannot
tell us about the peers on the other end.

## Why the renderer cannot move a card

Dropping a card emits `CardDropped` — a *request*, not a fact. The card has not
moved in game state, and the renderer never mutates the table itself.

This looks like ceremony while there are no rules, and it is the whole point:
when two peers play, each has to be able to reject a move the other proposes.
If the renderer could move cards, "what happened" would be whatever each
client's UI decided, and there would be nothing to validate against. The rules
engine plugs in exactly where `net::route_drops`' solo arm sits today — a
stand-in that accepts everything, added by the game explicitly so that
replacing it later is a one-line change rather than an untangling.

## Determinism does not reach the renderer

The determinism rules in [architecture.md](../../../agni/wiki/design/architecture.md) bind the simulation,
not this crate. Easing, hover, camera framing and frame timing are all free to
be non-deterministic, because none of them can change what happened in the game
— they only change how it looks while it happens.

That separation is what lets one rule set drive a Bevy table, a Godot scene and
an e-ink card. It is also why `agni-core` does not depend on Bevy, and why
`GameTable` is a newtype in `kai` rather than a `Resource` derive in
core: the firmware consumer cannot take a Bevy dependency, and the rules have to
run there too.

## Layout

Cards ease toward a `Slot` — a resting position and tilt — rather than snapping
to it. One lerp per card, frame-rate independent (`1 - exp(-rate * dt)`, so the
motion is identical at 60Hz and 144Hz). Easing is most of what makes the table
feel physical, and it is why a card returning from a failed drag reads as
"falling back into your hand" rather than teleporting.

Every seat carries a **yaw**: 0 for the near row, π for the far row. Board
slots, the hand fan, opponent card backs and the camera pose all spin their
local offsets and rotations by the owning seat's yaw, so the two rows face each
other the way players at a physical table do — the far side reads upside-down
from the near side. Hopping the view to seat K moves the camera behind K's edge
(same spin applied to the orbit offset and up vector), which makes K's zone read
upright and everyone else rotated relative to K.

Hand cards are billboarded with a **no-roll basis**: face normal at the camera
eye, width axis locked to the projection of the viewed seat's right axis (world
X spun by the view yaw). The shortest-arc rotation
(`from_rotation_arc`) is wrong here — for off-centre cards the minimal arc adds
roll, and a rolled card's horizontal footprint is `W·cos + H·sin`, so tall
cards lean into their neighbours. With roll eliminated, hand spacing is simply
card width plus a gap (`HAND_SPACING = CARD_W + HAND_GAP`) and cards cannot
overlap. No compression yet: a very large hand grows wider, it does not fan.

Slots are recomputed only when the table actually changes, in one pass over all
cards. A still table costs nothing.

- **Hand** — a shallow arc along the near edge, cards overlapping slightly and
  tilted back toward the camera so their faces stay readable. Each card sits a
  hair higher than the last so the fan layers correctly without z-fighting.
- **Board** — flat, centred, evenly spaced.

The camera is the one Hearthstone and MTG Arena converge on: fixed, tilted
forward, the whole board in frame, your side nearest and largest, the
opponent's receding at the top, your hand hanging off the bottom edge and
rising to meet you when hovered. Nothing pans; the view hops seats by spinning.

The framing is solved, not tuned. `scene::framing` takes the player count,
the window aspect, the tilt (`PITCH_ARENA`, 62° from the table) and the zoom
multiplier and finds the camera distance and focus point that pin the far mat
edge to the top of the viewport and the near mats' outer band (`QUAD_D −
FRAME_NEAR_INSET`) to the bottom — two bisections, alternated until both
edges sit at ±1 in screen space. The mat width then follows from the near
edge: it is the widest line on screen under a tilt, so it is cut to fill the
window exactly, floored at `QUAD_W_MIN` for portrait phones. At 90° the same
solver degenerates to the top-down fit. `fit_table` re-solves whenever the
window, tilt, zoom or seat count changes and publishes the result as
`Extent`; `apply_camera` reads the distance and focus from it.

The hand is placed against that frame rather than against the mat: `HAND_NEAR`
puts it just past the outer band and `hand_y` lifts it toward the camera, so
at the default tilt its centre projects to −0.95 — the top half of every card
shows above the bottom edge, and the hand reads about a fifth larger than the
board because it is nearer the lens. A hovered hand card lifts along the
camera's own up vector (`hand_rise`, not the board's `hover_rise`), which is
what brings it fully on screen the way both games do; the numbers are pinned
by `the_arena_camera_hangs_the_hand_off_the_bottom_until_it_is_hovered`, which
projects the resting and hovered hand through the solved rig. Opponents' hands
are fanned backs at the far edge, smaller by distance.

Tilt and zoom stay tunable (45–90° and 0.5–2.0×); the camera lock defaults
on so a stray middle-drag cannot break the frame, and edge drift is off.
`tuning.json` carries a `view_version`; a file from before this framing is
reset to it once on load.

## Playmats and the hand's hover reach

The felt is a `StandardMaterial`; a playmat is the same material with a
texture. `Tuning.playmat` names the choice and `sync_seats` looks the image up
in the `ArtCache` under `playmat:<name>` (or a battlefield card's own name),
which is exactly where card art lives — so the picker is a thin layer over the
existing on-demand art pipeline: a library entry is an `ArtRequest::playmat`
with a URL, fetched once by the art worker into the store's `playmats`
journal, restored from that journal at startup, and never bundled. The crop is
`cover_uv`: scale the shorter axis of the image to cover the mat, centre, and
flip the far row's copy so the opponent's mat faces them.

Hand cards used to un-hover the moment they rose, because the lifted mesh
left the pointer, then drop, then hover again. Now a hand card's hover is
settled per frame by `settle_hand_hover`: it stays while the cursor is inside
`hand_reach` — the resting footprint of the card, projected through the
camera, extended down to the bottom of the window — and mesh-exit events are
ignored for hand cards. A pointer entering a neighbour hands the hover over.

A mat is per seat, not per screen. Your choice rides the roster as
`SeatInfo.playmat` — a link for a library mat, `card:<name>` for a battlefield
from your deck — and `route_playmat_picks` sends it whenever the tuning
changes; every client fetches the other seats' mats through the same art
pipeline (`fetch_roster_mats`) and `sync_seats` paints each seat with its own
(`mat_of_seat`), your own always from your local tuning. A headless seat sets
its mat with `kai-cli --playmat`.

## Descriptor-driven zones

When a table's genesis pins a zone table (`ZoneDecl` rows from the plugin
design), the renderer stops being hand+board only. `zones.rs` is the pure
layout engine; `sync_zones` spawns one translucent drop quad per placement and
`layout_cards` places every card by its zone's declared layout. The zone
table reaches the renderer through the `Mirror` resource — a clone of the
session's `TableView`, refreshed once per folded entry, never per frame —
which also carries per-card `rotated` and badge state. An empty zone table
(the default `TableConfig`) renders exactly the old free-form table.

Where a zone sits is the plugin's call, not the renderer's: every `ZoneDecl`
declares a `ZonePlace` band and a `span` weight, and `zones.rs` turns those two
numbers into anchors. The renderer never knows a zone by name.

Placement rules:

- **`Inner`** — the per-seat row nearest the table center (`INNER_Z`);
  **`Outer`** — the per-seat row nearest the player's own edge (`OUTER_Z`).
  Within a band, zones lay out left to right in declaration order, each taking
  a share of the seat quad proportional to its `span`, all offsets spun by the
  seat's yaw so opposing rows read facing each other.
- **`Center`** — shared zones (battlefields) line up across the seam between
  the two seat quads at yaw 0, in a band `CENTER_DEPTH` deep so it straddles
  both players' halves the way contested terrain should. One battlefield is
  anchored per player, capped at the declared count, so a duel contests two
  and a four-seat table three. Because the band straddles the seam, its quads
  are opaque felt in the battlefield's own colour rather than a translucent
  tint — a tint over two differently coloured seat felts split every
  battlefield into two visibly different halves, and three battlefields read
  as six.
- **`Fan`** is the existing hand fan for your own seat (a plugin fan zone and
  the built-in `Hand` merge into one fan, one scroll); other seats' fan zones
  render as card-back arcs at their own edge, hidden while you view that seat.
- **`Offstage`** — the zone exists in the log and replicates like any other,
  but gets no anchor, no drop quad and no label, and its cards are held
  hidden. This is how a sideboard lives in the game state while never
  occupying table space; `sideboard.rs` is the panel that reaches it.
- **Pile** stacks in place; the overlay label carries the count
  (`Label · N`), which only ever shows the top card's face on the felt. A
  `ZoneKind::Discard` pile's label is also a click target once it holds a
  card — trash and banishment, in Riftbound, either seat's — opening a right
  sheet (`hud::sheet`, `ui::pile_sheet_ui`) that lists every card in it,
  newest on top, through the same `plugin_ui::card_label` a hidden card
  already answers to. Decks stay unclickable; their pile is secret by
  `ZoneVisibility`, not just stacked. **Row** spaces at board spacing,
  compressed to fit the slot. **Spread** overlaps at half-card spacing.
  **Grid** wraps at `√n` columns, rows stepping toward the table center.
- Zone labels render as a subtle egui overlay projected from the zone's near
  edge.
- Cards in `visibility = none` or unrevealed zones carry the hidden face and
  read as backs; no face bytes exist client-side for them at all.

Exhaust/tap is the `Annotate` log action (`key = "exhausted"`), rendered as a
90° in-place turn of any flat card. Toggle by `E`, or by the second click of
a quick pair on a free table (the first click selects; see "The verb
grammar" below).

Hotkeys (desktop + web keyboards; every verb also has a pointer path so
android loses nothing):

| Key | With | Does |
|---|---|---|
| `E` | a card hovered | toggle exhausted (`WireIntent::Annotate`) |
| click | a card | select it and show its chips; a quick second click fires the default chip (exhaust on a free table; a click that ends a drag is ignored) |
| double-click | one of your hand cards | play it onto the chain (same as `P`) |
| `D` | a deck pile hovered | draw: top card of that deck → your fan zone |
| `T` | a card hovered | send it to your trash/discard zone |
| `P` | a card hovered | play it onto the chain |
| `H` | one of your hand cards hovered | play it face down onto your base (`WireIntent::MoveHidden`: the host appends no reveal, so only you see its face) |
| `R` | your face-down card hovered | reveal it (`WireIntent::Reveal`; the host supplies the face it dealt) |
| `K` | a free table | the drawer's tokens tab (a placement mode; U9) |
| `L` · `C` | — | the drawer's log and chat tabs (U9) |
| right-click · long-press | a card | pin the inspector on it (recycle and trash are chips on the card) |

Dragging works between any visible zones — each zone quad is a drop target
routing `WireIntent::Move` to its declared zone, the seat plane keeps the
hand-strip/board fallback — and dragging a deck's top card into your hand is
the pointer form of a draw. A draw out of a hidden deck into an owner-visible
zone makes the host send that card's face privately to the drawing seat
(`HostSession::private_faces`), the same channel a deal uses.

## Tokens, hidden plays and what a face knows

A token is a card the table mints on request: `LogAction::Spawn { face, to,
seat }` gives it a fresh id, marks it revealed and puts it where asked. The
token window (`K`, `table/tokens.rs`) lists the tokens the plugin's manifest
declares (`PluginManifest.tokens`, remembered by `engine::modules` on both the
host and the join path) plus a custom name and might, one button per base or
battlefield. Nothing about tokens is game-specific in kai; a plugin that
declares none still gets the custom row.

A face-down play is a move with no reveal. `H` sends `WireIntent::MoveHidden`
for a hand card; the host folds the move but appends no `Reveal`. Every seat,
the owner included, renders a back on the table for an unrevealed card in a
public zone (`sync::face_down`), so a hidden card never feels revealed; the
owner, who holds the face from the deal, peeks at it through the hover
preview, which shows the real art captioned "face down". `R` asks the host to
reveal it. The plugin
sees a hidden card as a unit with no cost, which is the honest reading of a
face it cannot inspect.

Faces now carry `kind`, `energy`, `power` and `might` from the importer, so
the hover preview captions a card's kind and cost (`ui::stat_line`), kai-cli
prints them beside each card, and the Riftbound plugin can charge a play and
tell a unit from terrain without any lookup of its own.

## Turns come from the plugin

The renderer knows nothing about phases, and it stays that way: the turn strip
at the top of a Riftbound table is drawn from the plugin's `view` answer
(`plugin_ui.rs`). After every folded entry the session asks its plugin for this
seat's `PluginView` — status lines plus affordances, each a label, an optional
hotkey and the `Game` bytes to send when pressed — and kai renders the lines,
expands `{seat N}` and `{zone N}` into its own colour names, and routes a press
back as `WireIntent::Game` through the same path a card move takes. The plugin
decides whose turn it is and who holds focus; a refused press comes back as a
refused entry and the strip simply does not change. The design and the rules
it follows are in agni's `plugins.md` (W8) and `games/riftbound/rules`.

Since W9 the strip also carries what the plugin scores: every seat's points,
who holds each battlefield, and the winner at eight. Beginning-of-turn
choreography (awaken, hold, channel, draw), paying a card's cost from the rune
pool, and conquer scoring when a showdown settles all arrive as verdict
effects from the plugin, folded like any entry; kai draws the results and
offers "resolve Battlefield N" for the cases combat still settles by hand.

When the plugin names a winner (`PluginView.winner`, set by the Riftbound
presenter at eight points), `table/winner.rs` opens a dialog naming the seat
with three ways out: leave table (drops the session and returns to the games
menu), new game (the host resets the table through the redeal path; a guest
sends `ClientMsg::NewGame` and the host does it for them) and select deck
(back to the lobby with the session kept). "Keep looking" dismisses it until
the next deal.

## The riftbound mat

The riftbound plugin's placement mirrors Rift Atlas, so a player who has used
that client reads our table without relearning it. Local seat, left to right:

| Band | Left → right |
|---|---|
| `Center` | Battlefield 1 · 2 · 3 on the seam, one per player, neutral grey |
| `Offstage` | Chain (shared stack, drawn as the top-right panel) · Sideboard |
| `Inner` | Runes · Base · Legend · Champion |
| `Outer` | Rune Deck · Main Deck · Trash |
| `Fan` | Hand, across the near edge |

`Base` carries the widest span and `Runes` the next, because those are the
bands that actually fill with cards; Legend and Champion hold one card each.
The far seat is the same mat spun by π, which is Rift Atlas's 180° rotation
expressed as a yaw. `Sideboard` is `Offstage`.

## Colour names the seats; numbers name the battlefields

Every seat claims one of the five pickable colours — blue, red, green, gold,
purple — and the seat's felt is painted a dim version of it, the view-switch
buttons read that colour, and the roster lists who is who. Colour is per-seat
session state carried in `SeatInfo` on the roster, the one channel every
client already agrees on. `join` hands out the first free colour so play never
waits on a choice; picking is first-come-first-served, and a seat that loses a
race sees the winner in the roster and is prompted to pick again.

The shared battlefields are not coloured. They were once given the reserved
tail of the palette (teal, white, pink) so "move it to blue" could never mean
two places, but a translucent tint over two differently coloured felts split
each battlefield in two and read as six, and the names meant nothing at the
paper table. They are neutral grey opaque tiles now, labelled by the plugin —
Battlefield 1, 2, 3 — and the turn strip says "showdown at Battlefield 2". The
palette tail stays reserved in `agni_net::session` so seats can never take it.

## The chain

The Riftbound table declares one more zone: `chain`, kind `Stack`, shared,
`Offstage`. It exists in the log and replicates like any zone, gets no anchor
on the mat, and is drawn as a panel top-right (`stack_hud`): every card in the
zone, newest first, with its art, and for your own entries the two ways it
leaves — *resolve → base* and *resolve → trash*, plain `CardDropped` moves.
`P` with a card hovered plays it onto the chain. The plugin already treats a
move by the focus holder during a showdown as a play, so putting a spell on
the chain hands focus on exactly as rule 343 says. Nothing resolves itself;
the chain is a shared place to see what is pending, not a rules engine.

## The sideboard panel

`sideboard.rs` is the between-games sideboarding screen. A button above the
hand toggles it; the window puts two lists side by side — your main deck on
the left, your sideboard on the right — each row a thumbnail + name + count,
and a click (on the row or its `⇄`) moves one copy across.

**Both lists come from the deck list, not the table.** The `main-deck` zone
is `ZoneVisibility::None`: it is face down even to its owner, and the client
holds no face bytes for it at all, so there is nothing there to enumerate. The
honest source of your own deck's contents is the `SeatedDeck` record the
import panel staged — the resolved list plus its per-card faces. Sideboarding
therefore edits *the list*, which the panel says plainly ("changes apply on
reload"). Nothing about another seat's deck is readable by this path, because
a client only ever holds its own record.

The bottom of the window is **reload deck**, which emits
`ReloadDeckRequested`. `net::route_deck_reloads` turns it into
`HostSession::reload_groups` at the host, or `ClientMsg::ReloadDeck` from a
joiner — a `LogAction::Clear { seat }` sweeping every card that seat brought
(its own zones and its share of the shared battlefields), then the ordinary
deal choreography over the edited list. A reload replaces; it never
duplicates. "reset to imported list" puts the list back the way it was
imported; the panel notices a fresh import by the card pool changing, since
sideboarding preserves it.

## Deck import

The `deck import` egui window (all platforms) takes a paste box — text list,
PA deck code, or code list, sniffed by `agni_importers::riftbound::parse_any`
— and a URL field. Native and android resolve through the importer query path
(local Riftcodex catalog from the spirit store when ingested, the live
Riftcodex API otherwise; link fetching included). The browser can fetch
neither links (CORS) nor a local catalog, so both fields go to the gateway's
`/gateway/resolve/deck`; resolver errors, including the paste-fallback
guidance for Cloudflare-walled sites, surface verbatim. A resolved deck shows
legend/champion, per-zone counts and unresolved warnings; "seat this deck"
stages the `ResolvedDeck` plus a riftbound_id→`CardFace` map (spirit-store art
natively, gateway-bridge art on web, honest tinted placeholders when absent)
in the `SeatedDeck` resource for the table integration to deal from.

Seating a Riftbound deck with more than one battlefield opens the
**choose your battlefield** prompt (`deck/battlefield.rs`): the deck's
battlefields side by side with their art, one click to pick. The pick lives on
the seated record and the deal is gated on it — `auto_deal` and the deal button
both wait — so `deal_plan_for` sends `Battlefields::One(pick)` and the shared
band receives exactly one battlefield per player, the mode's contribution rule
(a War's first player sits out) still applied. A deal marks the pick played;
the next table that goes active clears it and asks again, because the choice
is per game, not per deck.

## Picking up a card

Bevy's picking gives us `Pointer<Over>`, `Out`, `DragStart`, `DragOver`,
`DragDrop` and `DragEnd` as entity observers.

The one non-obvious part: **while a card is held it is set to
`Pickable::IGNORE`**. The held card floats under the cursor, which puts it
directly between the pointer and the table — so it intercepts the ray that
reports where on the table the pointer is, and the card ends up blocking its own
drop target. Ignoring it during the drag is what makes the drop land.

Position while held comes from `DragOver` on the table surface
(`event.hit.position`), not from the drag's screen-space delta. The surface is
already the thing we want to project onto, so there is no camera maths to get
wrong.

Releasing anywhere ends the drag. A card dropped over nothing simply eases back
to its slot, which makes a mis-drag harmless rather than destructive.

## Dropping decides zone and order

A drop carries a position, and the position decides everything: release in the
hand strip — past `HAND_STRIP` along the viewed seat's outward axis — and the
card goes to hand, otherwise the board; the insertion index is how many of the
zone's other cards sit before the release point along the target seat's own
left-to-right axis (its yaw-spun world X), so ordering matches what that seat's
player sees. So dragging between two cards inserts between them,
and the same mechanism reorders within a zone and moves across zones.

During a drag, **every** card is set `Pickable::IGNORE`, not just the held one.
A drop aimed between two fanned cards would otherwise land on a neighbouring
card's mesh instead of the table, and the drop would go nowhere. With all cards
ignored, the table surface is the only thing under the pointer, so `DragOver`
tracks it continuously and `DragDrop` always lands. `DragEnd` restores
pickability.

`Intent::MoveCard` carries the index; an index past the end appends. Core
reorders one `Vec` whose order *is* every zone's presentation order, so peers
replaying the same intents see the same layout.

## The screenshot harness

Every UX milestone proves itself with the four reference sizes of
[ux.md §5.1](ux.md) captured headlessly, so the harness came first (U1,
v0.9.2). Three pieces, all in `src/app.rs` and `src/viewport.rs`:

- **`KAI_WINDOW=WxH`** sets the primary window's logical size at creation
  (`parse_window`; `1280x800`, `800×360` and `360X800` all parse). Without a
  window manager the size is honoured as asked, so the phase-1 `xdotool
  windowsize` dance is only needed to change size mid-run.
- **`KAI_SHOT=path`** names the screenshot. Every **F12** press spawns bevy's
  `Screenshot::primary_window()` with `save_to_disk`, which lands on the next
  rendered frame at `{W}x{H}-{stem}.{ext}` beside `path` — so
  `KAI_SHOT=shots/table-default.png` pressed at each size yields exactly the
  INDEX names `1280x800-table-default.png`, `1024x768-…`, `800x360-…`,
  `360x800-…`. When the variable is set, one shot of the first screen is also
  written to `path` itself on frame six, so a run with no input at all still
  proves the app rendered. Without `KAI_SHOT`, F12 writes `{W}x{H}-kai.png` in
  the working directory. The log line `shot N at WxH → path` precedes bevy's
  own `Screenshot saved to …`.
- **`Viewport`** and **`InputKind`** resources (`viewport.rs`, registered by
  `CardTablePlugin`, refreshed in `PreUpdate`): `viewport_class(logical)` is
  the pure §5.1 rule — phone portrait when the width is under 600 and the
  height is at least the width, phone landscape when the height is under
  480, tablet under 1100 wide, desktop otherwise — and the resource carries
  the class, the logical size and the scale factor. `InputKind` flips to
  `Touch` on any touch press and back to `Pointer` on the first mouse motion;
  Android starts on `Touch`, everything else on `Pointer`. A class change is
  logged (`viewport 800x360 → phone landscape`), which is how a capture
  script confirms the resize took. Nothing reads the class yet; U2 onward
  does.

The one-command procedure, as run for U1 (the script lives with the phase
scratch files, `ux/u1/table-default.sh`): a private `Xvfb :99 -screen 0
1600x1200x24`, `VK_DRIVER_FILES` pointed at the lavapipe ICD with
`WGPU_BACKEND=vulkan`, `SPIRIT_STORE` at a scratch copy of `~/.spirit/store`
and an empty `XDG_CONFIG_HOME` so the user's store and tuning are never
touched, then `KAI_WINDOW=1280x800 KAI_SHOT=$OUT/table-default.png cargo run
--bin kai --features fast-compile`. The script waits for `serving N blobs`,
clicks the Riftbound tile, ticks rules enforced, hosts, goes to the table,
picks Rockfall Path in the chooser (aim at the card art: hovering the row
raises egui's scrollbar over the label button), waits for `status=deck
dealt`, presses F12, and repeats F12 after each `xdotool windowsize` to
1024×768, 800×360 and 360×800. lavapipe renders a frame in tens of
milliseconds, so three seconds after F12 is plenty; the clicks need a
`mousemove`, a pause and then `click 1`, since a move and a press in the same
frame can miss a freshly laid-out button.

## Stopping the bleeding

U2 (v0.9.3) closed the eight INDEX defects of the UX phase without redesigning
anything, so the four reference sizes are usable while U3+ rebuild the table.

- **The wrap rule.** `settings::boxed` is a `ScrollArea::both` pinned to the
  box with `wrap_mode = Some(Wrap)` on its style. Every label, button and
  chip inside a window wraps at the box width, and a row that still
  overflows scrolls inside the box instead of widening the window and
  pushing it off the screen (defect 1: the pinned-deck note, the mode note,
  the playmat row, the settings header). The playmat and seat colour rows
  are `horizontal_wrapped` of fixed-size *widgets* — a painted `TILE` per
  playmat, a `Button::selectable` per colour — because `horizontal_wrapped`
  of `vertical` groups does not wrap.
- **`panel_metrics(class, box) -> PanelMetrics`** replaces the scattered
  constants (`HEADER_W`, the 320/140 field widths, `PANEL_W`, `LIST_H`,
  `CHOICE_W`): one column on phones and two otherwise, the identity box
  beside the tabs only when 240 pt remain for them, the sideboard list
  height from the box height, the chooser tile halved on phones. Tested at
  1280×800, 1024×768, 800×360 and 360×800.
- **The 312 dp walk** (`settings::walk`): a bevy `World` with every panel
  resource and a headless `egui::Context` lay out the games screen, every
  lobby column for every game with rules enforced on and off, seated and
  not, and every settings tab, and assert nothing is wider than the 312 dp
  box a 360-wide phone leaves. Telemetry's filter row carries the one
  allowed overflow until `telemetry.rs` wraps it.
- **Tiles** are fixed at `TILE` with wrapped text and laid out
  `tile_columns` per row (defect 6); the lobby's back button is `‹ games`
  (defect 4); the settings header has a × (defect 5).
- **The wheel** over any egui area no longer zooms the camera
  (`zoom_camera` checks `egui_wants_pointer_input`), and a wheel zoom is
  written through `bypass_change_detection`, so `save_tuning` never persists
  it (defect 2). A later slider save still writes the live zoom; the
  separate wheel axis is U7's.
- **The battlefield chooser** is `Order::Foreground` and moved to the top
  each frame while open, so it shows over the lobby (defect 3) and is sized
  to its tile row.
- **The sideboard toggle** is a button in the lobby's deck column; the badge
  no longer floats over the hand (the sideboard half of defect 7 — the
  score window moves to the seat plates in U4).
- **Off the table nothing is drawn**: `hide_viewed_hand` hides every card,
  opponent-hand back and zone quad while `Menu` is not at the table, so the
  stale deal never shows behind the lobby or the games screen (defect 8).

The run script for the four sizes lives beside the shots (`ux/u2/shots.sh`):
one launch, the lobby, the table (F12 per size), close table, the games
screen and settings › table at every size.

## Refusals, the tools gate and selection

### Emergency manual recovery

The table menu prioritizes the plugin's `disable rules enforcement` capability.
Any seated player can confirm it locally; the emergency command bypasses the
Riftbound decision/settlement engine. The legacy consensual free-table proposal
remains supported, but the emergency path enters fully manual play rather than
the older assisted free mode. No reset, redeal, cost, scoring or cleanup runs.
The engine still checks seat identity, private-zone access and table integrity.

The plugin advertises `manual controls` after recovery. `table/manual.rs` renders
the controls inside the existing table-menu sheet, with the same touch sizing,
scrolling and input cover. `net::route_manual` sends `Requested(WireIntent)`
through the normal host/client path. Neither the panel nor its availability gate
mutates a replica. The panel offers all declared table/seat/card counters and all
zones, including offstage zones; unknown faces are never named from cached data.

The bug icon occupies its own HUD slot beside the menu on every viewport,
including a 48 dp touch target on phones. It opens the GitHub new-issue form in a
browser/tab while the table stays open. It uses a drawn icon to avoid relying on
an emoji font and includes an accessible name and pointer tooltip.

The manual controls follow the physical-table approach of
[Riftatlas](https://riftatlas.com/play), including private deck looks and public
zone manipulation. Timeline rewind is outside this change; manual repairs are
ordinary synchronized actions.

The panel opens its scores, turn, battlefield and deck groups so the available
verbs are visible on first entry; the selected-card status group is also open.
Card rows and action rows wrap inside the sheet, including the 360 dp phone
layout. Roster names replace seat numbers wherever the session provides them,
and changing or losing a selected card clears its destination, position,
face-down toggle and label draft. The drawn report icon sends egui's existing
URL-open command, which uses the native Android browser opener and the browser
tab opener on wasm.

U3 (v0.9.4) makes the rules engine's contract true at the last metre: a
refused play is shown where the player is looking, the free-table verbs are
disabled rather than refused under rules enforced, and a card stays chosen
while the pointer walks to the button that acts on it.

- **The refusal queue.** `net::my_intent_failed` wraps `session_failed` for
  every intent this seat sends (a drop, a redeal, a deal, a reload, a game
  action, a counter nudge, an exhaust toggle, and `send_intent`'s reveals and
  spawns) and pushes the engine's own `Refusal::label` onto
  `SessionInfo.notices`; the `HostMsg::Notice` handler pushes the expanded
  notice with its `refused: ` prefix stripped (`net::notice_text`), so a
  joiner and the host read the same sentence, and so does kai-cli, which
  still prints the Notice. Another seat's refusals stay in `info.status` for
  the lobby and never become a toast. `toast::collect_refusals` drains the
  queue into the `Refusals` resource — `current: Option<Refusal { text, card, at }>`
  plus the last five texts in `log` — stamping bevy's clock and the card of
  the `LastIntent` when that intent is at most two seconds old.
  `toast::note_intents` reads every `CardDropped`, `ExhaustToggled`,
  `RevealRequested` and `PluginActionRequested` (which now carries the
  affordance's card as its second field) and writes `LastIntent` before the
  routes run, so a host refusal in the same frame already has its anchor.
- **Three renderings.** The plugin strip opens with an amber
  (`toast::strip_row`) line for 2.5 s — and draws itself for that line even
  when the view is empty; `toast::card_toast_ui` floats the same bubble 8 pt
  above the refused card's screen rect for 1.5 s (an `Area` with a
  `CENTER_BOTTOM` pivot at the projected rim's top edge) and paints an amber
  rim that fades over 0.6 s; `animate_cards` adds `toast::shake` — three
  damped half-cycles along the camera's right vector, 120 ms — to the
  refused card's eased position. `expire_refusals` clears `current` after
  the strip's 2.5 s so nothing is redrawn from a stale clock.
- **`Tools { free }`** (`plugin_ui::refresh_tools`, from `enforced(&view)`)
  gates every free-table verb with one check at its draw or send site: the
  single-click exhaust and the right-click recycle (`interaction`), a drag
  of a card with no rim (`interaction::may_lift`: a card lifts only when
  `Rims::kind` names it or the table is free), the D/T/E/K keys
  (`ui::key_allowed`), the chain's "resolve →" buttons, the score ± and the
  card-counters popup (`counters::nudges_allowed`), the tokens window
  (closed the frame the mode flips), and the sideboard's "reload deck"
  (`sideboard::reload_allowed`: any time on a free table, between games —
  `view.winner` set — under rules enforced). `enforced` reads the
  presenter's turn line, so the roll and mode lobby before the first turn
  counts as free: the plugin accepts every move there, and a control is
  drawn only if the plugin could accept it. The M6-style fixture test
  (`interaction::click_tests`) asserts each gate reads inert for the
  "rules enforced" view.
- **`Selected(Option<Entity>)`** sits beside `Hovered`. A left click on a
  card selects it (before any affordance or double-click logic), a click on
  the felt or a zone quad clears it (`interaction::on_click_felt`, a global
  observer), and `expire_selection` drops it when the card despawns or the
  menu leaves the table. The preview, the strip's hidden-card offer row, the
  card-counters popup and the H/P/R keys read `Hovered.or(Selected)`
  (`table::Focus` for the two that only need the `CardView`), so the offer
  row survives the pointer moving to its buttons — the third leg of the
  facedown blocker.
- **The click plan** is one pure function, `interaction::click_plan`: a
  card an enabled affordance names answers the prompt; two offers leave the
  click to the strip; while my prompt is open any other card is inert
  rather than sent to be refused; otherwise a card that is neither in my
  hand nor my own facedown board card exhausts on a free table and does
  nothing under rules enforced; my hand card or my facedown card selects on
  the first click and plays to the chain (`interaction::chain_drop`, the
  same bytes as the P key) on a second click within 350 ms. The
  "double-click routes every non-hand card to exhaust" arm is gone.

The harness run (`ux/u3/shots.sh`) hosts an enforced table, rolls, goes
first, clicks the champion on the board — no status line is logged, so no
`ExhaustToggled` reached the session — then presses P over it during the
mulligan: the shot shows the amber row on the strip, the bubble over the
champion and its amber rim, and the log's only refusal is
`answer the open question first`. "deal a sample hand" is gated too since
U5: `settings/advanced.rs` passes `redeal_allowed(tools.free, between_games)`
into `tuning_section`.

## The HUD frame

U4 (v0.10.0) replaces eleven self-anchored egui areas with one layout and
one owner per region, so the table's surface is the [ux.md §3](ux.md) frame:
a turn plate that says whose move it is, a primary button that says what
pressing it does, a strip with five states, a chain panel that is also the
drop target, an inspector that never moves, seat plates instead of a score
window, and a table menu instead of Esc-means-settings.

- **`hud::layout(class, screen, insets, drawer, chain_len, seats) -> HudRects`**
  is the one pure function. Every slot is a named rect: `menu_button`,
  `turn_plate`, `phase_bar` (reserved, drawn only once U11 carries the
  phase list), `strip`, `toast`, `chain`, `seats`, `history` (reserved for
  U9), `inspector`, `secondary`, `primary`, `drawer_tabs`, `drawer`, the
  reserved `hand` band, the floating `banner` and the `stage`. The desktop
  row is §3.2 to the point (12 pt gutters, a 220 pt right column, the strip
  `min(560, the span between the turn plate and the column)` centred on the
  screen, the hand band `W − 2·192` wide and `0.28·H` tall); the tablet row
  narrows the column to 200 and the inspector to `0.16·W`. The right column
  is budgeted against the buttons and the hand band: the chain shows three
  full rows, then one, then only its header and "and N more", and the seat
  plates drop from 64 to 40 pt, before anything would overlap. A raised
  drawer (U9) pushes the column and the strip left. The phone classes go
  through `Metrics::compact`, the seam U7 replaces with the §3.4/§3.5 rows:
  the strip under the top row on portrait, beside it on landscape, the
  primary above the hand band, no inspector, history or tabs. The test runs
  the four reference sizes × chain 0/1/5 × drawer tucked/raised × 2/4 seats
  and asserts no two placed slots intersect, every slot sits inside the safe
  rect, the touch slots are ≥ 48 pt on phones and a phone keeps at least
  half its height for the stage. `refresh_hud` recomputes the rects every
  frame into the `Hud` resource; every HUD system draws with
  `hud::slot(ctx, id, rect, …)` — `fixed_pos` plus `set_max_size` — and
  nothing under `src/table` calls `.anchor()` any more.
- **`plate::classify_status(line) -> StatusLine`** parses the presenter's
  own strings until U11 carries structured fields: the turn line, points and
  xp, the winner, held/contested, the chain line, showdown and combat, the
  focus, the might totals, the damage assigner, every "waiting for" form,
  setup, the roll lines, the mode, the free-table proposal, and narration as
  the fallback. Its test table quotes the strings verbatim, so a presenter
  wording change fails a kai test rather than the player. `turn_plate`
  turns a view into the plate's words — "your action", "waiting for claude",
  "respond or pass", "showdown at Battlefield 2", "setup · mulligans", "your
  choice", "you won the roll", "claude wins" — with the "turn 7 · action
  phase" detail, the mode chip (neutral **enforced**, amber **free**), the
  acting seat's colour on the border, and a 2 s pulse while `acting(view)`.
  `seat_plates` reads points and xp from the counters first and the points
  line second, the victory score from the table options, the other seats'
  hand and deck counts from the `GameTable`, and the roster's connectivity;
  my plate comes last. The score ± render only on a free table; on free and
  FFA tables a plate is clickable to look from that seat and the viewed one
  carries ◉ (the seat buttons are gone).
- **`colors::seat_label(roster, colors, me, seat) -> (name, rgb, is_me)`**
  is the one naming rule: the roster name, "Player N" when there is none.
  The strip's `{seat N}`, the plates, the chain rows, the combat plate, the
  zone labels and the winner banner all read it; a colour is never a name.
- **`primary::primary_of(view) -> Option<Primary { affordance, label, tone }>`**:
  a prompt's done/keep option first, else pass — reading **resolve** when
  the chain is non-empty — else end turn; amber while `plays_remain` (a
  legal row or another enabled offer), green when it is the only thing left,
  grey and disabled ("waiting") when it is not my move, `None` when there is
  no view. `secondary_of` is end turn when the primary is something else. The
  button prints ⎵; Space and W press it when the plugin's own key would not
  (`is_primary_key`, so end turn's `space` never fires twice), and a click
  within 300 ms of a drop is swallowed (`guarded` over `RecentDrag`). The
  strip's chips are computed by `plugin_ui::strip_chips`, which excludes the
  primary, the secondary, the cancel, the free-table offer and, while a
  prompt is open, every option that names a card — the card is the button.
- **The strip** (`plugin_ui::strip_state`) has five exclusive states in its
  slot and collapses to nothing otherwise: my prompt (the question, the count
  chip — "1 of 1", "2 picked · up to 3", "· optional" — the hollow × cancel
  at the row's left end, the chips); the response window ("claude played
  Back Off — respond or pass" from the top chain row); waiting (a spinner
  ticking seconds, "still thinking…" after 60 s, red **disconnected** when
  the roster says so); the opening roll with its chips; the amber refusal
  row on top of any of them. The hidden-card offer row, the greyed hint and
  activation chips stay on the strip until U6 moves them onto the cards.
  Nothing on the strip grows past three rows; everything else the presenter
  writes is routed: the scoreboard to the plates, held/contested onto the
  battlefield (a 4 pt seat-coloured rule along its near edge, dashed for the
  contester), the combat line into a plate over the contested battlefield
  (the two might totals as seat-coloured chips), the chain line dropped.
  Zone labels on the far side carry the owner's name ("claude's Base").
  Every prompt the engine raises renders through this one path — the
  presenter's `why` is the question and the affordances are the chips — so
  M9's kinds needed no new state: a ransom (`PromptWhy::PayOrLet`, "pay 2
  energy to keep Discipline?") shows the other seat a waiting line and the
  payer two chips whose labels `plugin_ui::answer_wording` rewrites from the
  presenter's `yes`/`no` into the words its `answer_words` promise, "pay 2
  energy" and "let it resolve" (`PromptKind::PayOrLet { cost }`, parsed from
  the question); kai-cli prints the same words in parentheses after
  `action N: yes`. The Repeat, additional-cost, XP and Burn confirms, the
  ambush location, the look-at-top and reveal-and-pick candidates all read
  verbatim. The banish pile is the fourteenth zone of the manifest and lays
  itself out from its decl beside the trash (`zones.rs` pins it in the
  two-seat layout test; hand drops still route to the trash, the first
  discard zone). XP rides on the seat plates from the `xp` seat counter (the
  points line's tail as the fallback) and is a nudge only where
  `counters::nudges_allowed` says so, which is the free table.
- **The tray** (`plugin_ui::TrayItems`, `tray_ui`) draws card faces at
  120×168 in the `banner` slot for prompt options whose card has no face on
  the felt (`faceless_options`: the top N of a deck, a revealed hand — enemy
  cards framed in the owner's colour) and for whatever `deck/battlefield.rs`
  fills in: at the table the battlefield choice after "play again" is a tray
  of wide faces, not a window, and `picked` is read back. "peek at table"
  collapses it to text chips.
- **The chain panel** (`chain.rs`) draws in its slot: rows newest first with
  a 52×72 thumbnail, the name, the controller's swatch, the top three full
  and "and N more", hover into the inspector, the resolve buttons free-table
  only. It is the chain's drop target: `on_drop_on_chain` watches every
  `DragEnd` and, when the release lands inside the panel rect
  (`ChainRects`), emits `interaction::chain_drop` — the same bytes as P and
  the double-click. `ChainRects` also publishes each row's live centre, and
  `arrows::item_anchors` reads those instead of a hard-coded row height.
- **The inspector** (`inspector.rs`) sits in its slot at the left edge and
  never moves: the hovered-or-selected card (or the hovered chain row) at
  the slot width, landscape for battlefields, with a caption of kind · cost ·
  printed → current might (green above, red below) · statuses · "face down
  · name" for my own facedown card.
- **The winner banner** is a panel in the `banner` slot: "{name} wins · N
  points" in the seat colour, play again and leave, a × that keeps looking;
  "select deck" is gone (change deck lives in the table menu).
- **The table menu** (`hud::TableMenu`): the ≡ in the `menu_button` slot or
  Esc opens a left `sheet` — resume · propose / confirm a free table (the
  presenter's offer, drawn nowhere else, so it never sits beside end turn) ·
  change deck (between games) · leave table (a second press confirms; "this
  closes the table" as host, "the table stays open for the others" as a
  client) · settings. `sheet(ctx, id, class, side, title, open, body)` is
  the one helper: one full-screen `Order::Middle` area moved to the top
  each frame that paints the scrim (a click outside the panel closes), a
  380 pt side panel inside it (full screen on phones), one scroll area, a ×.
  The scrim is black at 185/255 premultiplied because bevy_egui blends in
  linear light: 115 read as a 25 % dim on the felt, not the 45 % of §7. Esc at the table closes an open
  settings screen and otherwise toggles the menu; the ⚙ row is drawn off the
  table only. The version badge over the table and the wasm bridge line are
  gone (the build line sits in the settings header until U5's advanced tab;
  the plates' connectivity dot replaces the bridge line).
- **The hand slider** is gone: the wheel over the hand band scrolls the fan
  (`scene::wheel_over_hand`, shift still works anywhere) and a "1–7 of 9"
  chip sits at the band's right end when the hand overflows.

What U4 does not touch: the camera framing still fits the window, not the
stage, so the near band's outer stacks can sit under the primary column on
a wide window and the phone sizes still crop the far side — `framing_for`
against `HudRects::stage` is U7's. The history rail and the phase bar have
their rects reserved and nothing drawn (U9, U11). Concede is not in the menu
until the presenter offers it (U11).

## Responsive — the phone rows, the framing, the drawer, the safe areas

U7 makes the two phone classes real layouts instead of a shrunken desktop,
frames the 3D table against the HUD's `stage` slot, puts the hand in a
drawer, reads the safe area on Android and the web, and adds pinch and
two-finger pan. Everything is a pure function with a test beside it;
nothing here changes the desktop or tablet rows.

- **`hud::layout` dispatches by class.** Desktop and Tablet run U4's row
  untouched; `PhonePortrait` and `PhoneLandscape` go to
  `layout_phone_portrait` / `layout_phone_landscape`, the [ux.md §3.4/§3.5](ux.md)
  tables to the point. `HudRects` grew five phone-only slots — `top_bar`
  (a container, not a placed slot), `opp_strip`, `ticker`, `chain_rail`,
  `bottom_left` — and `drawer_state`. On phones the existing consumers draw
  into the phone rects through the fields they already read: the strip is
  the banner band, the chain panel draws into the chain rail as the
  ribbon of §3.4 — "chain · N", up to four 28×40 thumbnails as
  `chain::rail_rows` fits in the rail's width, "+N" beyond, each bordered
  in its controller's colour and anchoring the arrows; the ribbon is absent
  while the chain is empty and no card is held (`chain::rail_hidden`), and
  a tap toggles `chain::ChainSheet`, the full list through `hud::sheet` —
  the seat plates sit at the right end of the top bar at a compact
  `plate_h`, and `toast` *is* the ticker rect (the toast slot rides the
  ticker line, so `placed()` skips it on phones). Portrait: top bar 56,
  opp strip 64, banner up to 96 (three strip rows), ticker 32, rail 56, the
  stage down to the drawer, primary 152×56 and the rune chips riding the
  drawer's top edge; the stage is 408 pt with everything pending and 504
  with an idle strip. Landscape: a 48 pt top bar (the design says 44; the
  touch floor is 48 and the ≡ lives there), the banner floating centred at
  `min(480, W − 2·176)` over the far seat's inner band, the ticker and rail
  under it, primary 120×48 at 672–792 × 248–296, the stage 48–296. When the
  landscape drawer is raised the ticker collapses and the rail clamps above
  the drawer so nothing floats over the hand.
- **`layout_pending(…, strip_idle)`** is the seven-argument form: with
  `strip_idle` the banner band is zero-height and the rows below move up.
  `refresh_hud` computes idleness from `plugin_ui::strip_state` and the
  host's empty-seat line (`hud::strip_idle`); refusals do not un-idle it —
  on a collapsed strip the amber row draws in the ticker line, so the
  camera never reframes for a 2.5 s notice. In portrait the framing is
  width-fitted with the near edge pinned, so a prompt opening changes no
  camera either way.
- **`scene::framing_for(class, players, window, stage: StageNdc, pitch, zoom, floor_pt) -> Framing`**
  replaces the window fit. `StageNdc::of(window, stage_rect)` turns the
  stage slot into NDC bounds; the solver bisects the distance for the depth
  span (`fit_depth`) or, on portrait, for `QUAD_W_MIN + 2·FRAME_NEAR_INSET`
  units across the stage width (`fit_width`, pitch forced to 72°), then
  pins the near edge to the stage bottom (`pin_near`). `depth_edges(class)`
  chooses what is framed: desktop and tablet keep today's edges (the hand
  fan hangs off the near band by design, so their stage for framing is the
  safe rect, not the slot between the toast and the hand band — framing
  the table into that 412 pt band would float the fan in the middle of the
  screen); landscape frames from the far seat's base cards to my outer
  band's near edge, cropping the far outer band whose counts the top bar
  carries; portrait frames to my outer band too, since the hand lives in the
  drawer. `raise_to_floor` then shrinks the distance until a card lying in
  my base projects at least `min_card_pt(class)` (56 on phones, 44 on
  desktop), re-pinning the near edge each round so the far side is what
  crops. At 800×360 the plain fit gives 51 pt and the floor costs the far
  base card its top ~20 px under the top bar; at 360×800 the width fit
  gives 55 and the raise is a whisker. `Framing::card_pt` carries the
  measurement so the test reads it rather than assuming it. `Extent` now
  owns `pitch_deg` and `card_pt` beside `quad_w`; `apply_camera` always
  poses through `camera_pose_panned` (a zero pan is the old locked pose).
- **`quad_w` as a parameter.** `seat_center_in(seat, count, quad_w)` and
  `zones::anchors_in(zones, players, battlefields, quad_w)` are the pure
  forms; `layout_cards` and `sync.rs` pass `extent.quad_w`. `seat_center`
  and `zones::anchors` remain as wrappers over the `dim::quad_w()` atomic
  for the U6 call sites (`interaction.rs`, `arrows.rs`, `highlight.rs`,
  `ui.rs`); the atomic goes when those four move to the `_in` forms.
- **The hand drawer** (`layout.rs`): `HandDrawer { state }` and
  `DrawerScroll` are the phone hand's state; `drawer_next(state, event)`
  is the machine (`TapDrawer`/`SwipeUp`/`PromptInHand` raise;
  `TapFelt`/`SwipeDown`/`DragOut`/`PromptOnBoard`/`TurnEnded` tuck).
  `hud::drawer_ui` owns the gestures: tucked, the whole band is one
  egui sense (tap raises, an upward swipe past 24 pt raises, a horizontal
  drag scrolls); raised, a 38 pt grab band under the cards tucks on tap or
  a downward swipe and scrolls on a horizontal drag, while the cards
  themselves stay pickable for taps and drags. `tuck_on_drag_out` watches
  `Held` and tucks the moment a hand card is dragged above the drawer's
  top; `on_tap_felt` tucks on a felt tap and resets pan and zoom on a
  double tap; `auto_drawer` fires `prompt_event` once per new prompt
  (candidates in my hand raise, candidates on the board tuck; the mulligan
  is the hand case) and `turn_event` when `acting` falls. Hand cards on
  phones are laid out in screen space: `drawer_card_center` gives each
  card's pixel centre in the band (96 pt wide, 12 pt overlap raised, five
  across tucked with only their tops showing), `card_depth` the distance
  at which a card projects 96 pt wide, and `phone_hand_slot` casts the
  camera ray to that depth. `hand_visible(class, band_width, pitch)`
  replaces the constant on phones (desktop keeps 7). `anim::anchor_of`
  faces each phone hand card at the eye instead of the seat's shared hand
  plane.
- **The opponent strip.** `sync::OppStrip` is refreshed from the table:
  per far seat the hand count, main deck, trash, rune pool ready/total and
  the legend and champion names (`opp_seats`). `hud::opp_strip_ui` draws
  it as 32×45 thumbnails, countable backs and chips on portrait, one chip
  line inside the top bar on landscape. `hud::bottom_left_ui` draws my
  rune pool as "runes ready/total" beside the primary (the bundled font has no ⬢).
- **Touch camera.** `scene::touch_camera` reads egui's `multi_touch()`:
  `pinch_zoom` divides `tuning.zoom` by the zoom delta (through
  `bypass_change_detection`, so a pinch is not persisted, like the wheel),
  `pan_step` moves the pan like the middle-drag does. `camera::Preset`
  (arena 62°, top-down 90°) is the pair Settings › look offers;
  `Preset::apply` sets the pitch and zeroes the pan.
- **The zoom rule is applied through the egui style, not `set_zoom_factor`.**
  `viewport::UiZoom { ui_scale }` × `class_zoom(class)` (1.15 portrait,
  1.10 landscape) scales `touch_style(class, input_kind)` — interact height
  48/40/24, button padding, item spacing, body 16/15/14 pt — in
  `apply_ui_style`, once per class/input-kind/scale change. `set_zoom_factor`
  would divide egui's coordinate space by the factor while thirteen
  `world_to_viewport` sites (rims, arrows, toasts, chips, counters, zone
  labels) keep writing window points into egui, so every overlay would land
  off its card on phones; the style route gives the same text and control
  sizes and the phone rows already carry phone-sized rects.
- **Safe areas.** `viewport::SafeInsets { top, bottom, left, right, ime }`
  is the platform resource; `sync_insets` copies it into `hud::Insets`
  (`bottom = max(bottom, ime)`) and `feed_egui_insets` hands the same
  margins to egui's `RawInput::safe_area_insets`, so `ctx.content_rect()`
  — the rect every sheet fills — shrinks too. On Android `MainActivity`
  installs an `OnApplyWindowInsetsListener` and calls `nativeInsets(top,
  right, bottom, left, ime)` in dp (display cutout + system gestures on
  R+, the cutout and gesture insets separately below; `SafeInsets::from_array`
  clamps negatives and NaN), and `hideSystemBars` moves from
  `setSystemUiVisibility` to `WindowInsetsController` with
  `LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES` (also in the manifest's
  `Theme.Kai` as `windowLayoutInDisplayCutoutMode=shortEdges`). On the web
  `index.html` reads `env(safe-area-inset-*)` through a hidden probe and the
  visual-viewport keyboard height into `window.__kaiInsets`, which
  `viewport::platform` reads each frame. Desktop insets stay zero.
- **The IME.** `os::ime::scroll_focused_above_ime` runs first in the egui
  pass: while the IME inset is up and the focused widget's rect ends below
  `content_rect()`, it asks egui to scroll that widget into view, so a
  field under the keyboard rises inside its sheet's scroll area.
- **The logical back key.** `viewport::BackKey` is set from
  `KeyboardInput.logical_key` (`Key::BrowserBack`, which winit sends for
  the Android back button, or `Key::GoBack`); `back_pressed(keys, back)`
  is Esc, the physical `KeyCode::BrowserBack` or that key, and
  `consume_back` clears all three so the next rung of the ladder stays
  quiet. The four ladder handlers (`help_keys`, `drawer_keys`,
  `escape_ladder` and `menu_keys`) read it and the PreUpdate chain is
  ordered after `read_back_key`.
- **Resume.** `net::rejoin_on_resume` watches `AppLifecycle::WillResume`:
  a session that ended with `Recovery::Rejoin { host }` asks the bridge to
  join that host again (`resume_rejoin` is the pure decision). Keep-awake
  was already tied to hosting and joining.
- **The web page** sets `viewport-fit=cover`, `user-scalable=no`, `100dvh`
  on the body and canvas, and `touch-action: none; overscroll-behavior:
  none; user-select: none` on the canvas.

All of it registers through `hud::ResponsivePlugin` (added in `app.rs`),
so `CardTablePlugin` changed only in one ordering edge: `refresh_hud` now
runs before `camera::fit_table`, which reads the stage from `Hud`.

What needs a device: the JNI insets and the cutout mode, the
`WindowInsetsController` immersion, the IME inset arriving through
`nativeInsets`, pinch and two-finger pan through egui's touch events, and
the drawer's swipe thresholds under a real finger. `cargo ndk check --lib`
proves the Rust side compiles for arm64; the Java side is checked by the
next `android-build`.

## Home, lobby, deck box and settings

U5 replaced the games screen, the lobby, the deck surfaces and the settings
window with the four screens of ux.md §2. `src/menu/` is split by screen:
`mod.rs` holds `Menu` (the screen, the last game, the open sheet), the back
ladder and the shared chrome (`screen_frame`, `chip`, `segmented`, the ‹ and
gear buttons); `home.rs` the resume card and the three fixed tiles in the
order Riftbound · MTG · free-form (`HOME_ORDER`); `lobby.rs` the three
decisions and the primary verb; `opponent.rs` the AI · friends · join group
and the AI seat's start path; `deckbox.rs` the one deck sheet. `src/settings/`
is split by tab: `mod.rs` (the sheet, the tabs, the `PanelMetrics` and the
312 dp walk), `play.rs`, `look.rs`, `you.rs`, `advanced.rs`.

**The back ladder** is `Menu::ladder(table_menu_open) -> Rung`, walked by
`menu_keys` on Esc and the logical back key for every screen: a sheet closes
first, then at the table the table menu toggles (never leaving the table),
then the lobby goes Home, and Home stays. Settings is a sheet, so it is the
first rung whenever it is open; `settings::toggle_on_keys` and F3 are gone.
`Menu::back()` keeps its old meaning — the explicit "back to the lobby" of
the empty table — because `src/table/ui.rs` calls it.

**The lobby** is one `lobby_screen`: a header (‹, the game name, the gear
and, on desktop and tablet, the primary button), one vertical scroll area and,
on phones, a sticky footer holding the primary. `lobby_sections(game,
enforced, role)` is the pure section list: Riftbound gets deck (with the
coverage chip under rules enforced), rules and opponent; MTG the same with
the rules locked to free; free-form no deck card. `primary_verb(&LobbyState)`
is the state machine behind the one button — *play vs AI* · *host table* ·
*join {name}* · *go to the table* — and `action_for` turns the enabled verb
into the net call. Its disabled reasons are inline: "choose a deck first",
"choose a battlefield first", "pick a table or paste a ticket", and whatever
`net::host_block(&choice)` returns — nothing on desktop and android; on the
web, "engine.wasm is still loading" until the engine module is in, or
"rules enforced needs the plugin: …" while the gateway's `modules/riftbound`
is still on its way (the browser hosts too, with the opponent panel warning
that a hidden tab pauses the table). Rules enforced is the Riftbound default: opening a lobby
whose game differs from `TableChoice.game` resets the flag through
`default_enforced(game)` (the `TableChoice::default` itself is untouched
because net's genesis tests read it). The mode presets are a chip row —
*by seats* · the five `MODES` · *house rules…* — with the two steppers under
them and one sentence, `mode_line`, beneath. Playmat and seat colour left the
lobby for Settings › look.

**The deck card** shows the seated deck's legend art (a `Thumbs` local staged
by `deckbox::stage_thumbs`), `history::label`, the battlefield line with its
*change* (or the *choose a battlefield* chip while `needs_choice`), and under
rules enforced the coverage chip: `pool::deck_coverage(deck,
pool::scripted_names())` counts the legend, champion, main deck and
battlefields against the union of card names in every `Scripted: complete`
pool file, and `pool::coverage_chip` prints "scripted" or "N of M scripted —
the rest play as vanilla". `pinned::Source::note` is now one short line and
`pin_decks` writes it, plus the staged count, to `ImportPanel.note`, which
the deck card shows while no import is in flight.

**The deck box** (`deckbox_ui`, drawn through `hud::sheet` on the right) is
the deck **selection** surface and nothing else; the seat it picks for is
implied by the card that opened it (`DeckSeat::Mine` / `DeckSeat::Ai`).
Steps: *your decks* — the saved decks from history as tiles (a tap seats
one on any table, rules enforced included; the AI seat adds *let the AI
pick*; renaming, deleting, editing and importing live in the deck editor,
which the sheet's last link, *open the deck editor*, opens). The pool decks
are no longer offered — every card is scripted — and the battlefield step is
gone: the choice is made at the table (`battlefield::prompt_ui`'s tray, after
the body deal and before the roll). *Battlefield* (historical) —
`battlefield::battlefield_step`, the deck's battlefields as 160×112 tiles,
single choice, the placement note as one line; it is never a spontaneous
window any more: `prompt_ui` draws only the table tray, and off the table the
deck card and the primary's reason say the deck is incomplete. *Sideboard* —
`sideboard::sideboard_step`, the two lists as 56 dp rows with a 40 dp
thumbnail and a *swap* button, "use this list next game" gated by
`reload_allowed`; the bottom-centre toggle and the floating window are gone
and `sideboard_ui` is now only the system that remembers the imported list
for *reset*. Import lives in the deck editor's footer now and reads the
clipboard itself (`menu::editor::start_import` → `os::clipboard`,
`import::begin_import_any` detects the game, sends a first-line link through
`dispatch_url` and anything else through `parses_locally` and
`dispatch_paste`, and records `ImportPanel.source` ("link:{url}" or "paste")
so the history record's transform carries where the deck came from; the
result offers *load into the editor*, *save to your decks* and *share*).
"download full riftbound set" is `import::full_set_controls` under Settings
› advanced.

**Pool labels.** `history::label` asks `pool::label_for(deck)` first, which
compares the deck's `Snapshot::identity()` with the identity of every pool
list (built once and cached, on every platform), so a pool list is labelled
by its file's H1 — "Lillia (house)", "Lillia (Jonnynick)" — wherever the
label is shown: the deck card, the seat rows, the AI seat's status, the soak
record. `pinned::seating_holds` accepts either the saved row's label or the
pool label for a saved copy, since the saved copy of a pool list is labelled
by the pool too.

**The opponent group** is a segmented control. *AI*: the AI's deck card
("let the AI pick" by default; the deck box for the AI seat offers every
pool deck and every saved deck), its battlefield ("the first of its deck",
index 0, the nanosecond pick is gone), a *random · fast · thinking* brain
row (`ai::seat::BRAIN_PRESETS`: the free random brain, then the two model
presets), and the running seat's status with *stop AI*. *play vs AI*
hosts and sets `Opponent.pending_ai`; `opponent::start_pending_ai` starts
the in-process seat (`ai::local`, below) the frame the host role lands.
*friends*:
the roster as rows (swatch, name, the deck label for me or the legend on the
table for others, `(you)` `(host)` `(gone)` tags), *invite* (copies the
ticket and shows the QR inline through `identity_header`), *add AI*, and the
amber recovery card only when `Recovery` is not `Nothing`. *join*: the mesh's
open tables as selectable 64 dp rows (the first is preselected so the
primary reads "join {name}"), and the empty state with a paste field whose
*join* goes through `net::join_by_ticket` — `node::add_peer` then
`bridge::request_join` — so a direct join works while the `tables` listing
bug stands. `net::lobby_section`, `net_section`, `host_controls` and
`recovery_controls` are deleted; net exposes `open_tables`, `host_table`,
`join_table`, `join_by_ticket`, `rejoin`, `rehost` and `host_block` instead.
Hosting with rules enforced and no loadable plugin is refused outright
(`enforced_without_plugin`): the lobby stays, the status names the reason,
and no free table opens in its place.

**Settings** is `hud::sheet` on the right, 380 pt on desktop and tablet and
the whole screen on phones, with the × the helper draws and no save button
("settings save as you go" in the footer). Four tabs, play the default.
*play* is honest: no play preference ships before the automation it drives,
so the tab lists what arrives with U8. *look*: the playmat grid, the seat
colours, the camera presets (`CameraPreset::Arena` 62° / `TopDown` 90° over
`tuning.pitch_deg`), zoom, the hover preview size and a foil checkbox over
`foil_chance`. *you*: the QR and ticket, add a peer, and the peers list
under a *details* disclosure. *advanced*, behind "show developer settings"
(`Settings.developer`, session-scoped until a tuning field exists): the
tuning sliders and the sample hand, "open a tuning table" (the old "open the
table without a session"), the full-set download, the AI model string and
presets (`ai::window::model_controls`), the per-game plugin lines and the
modules section, the node line, telemetry, and the footer "v0.10.0 · wire 5
· modules/riftbound v…" (`advanced::footer_line`). The multiplayer and decks tabs, the game
radios, the identity header on every tab and the version badge on every
screen are gone.

**The AI chat** window opens with C at the table only (`ai::window::CHAT_KEY`,
`chat_opens`), holds the chat lines, the input, the preset row as "switch
model" and *stop AI*, and closes itself off the table; it is the drawer's
chat tab in waiting.

**Tests.** `Menu::ladder` and `Menu::back`; `content_width`; `HOME_ORDER`
and the tile row; `resume_card`; `lobby_sections`; `primary_verb` and
`action_for` for every role, segment and deck state; `mode_line`,
`table_line`, `rules_line`; `deck_state`; `pool::coverage`,
`deck_coverage` and `coverage_chip` over every pool deck; `pool::label_for`
and `history::label` on the two Lillias; the seat tags; the AI's pool deck
and first battlefield; the recovery card; the legends per seat; the deck box
and battlefield tile columns; the sheet metrics; the default tab and the
developer toggle; the camera presets; the footer line; and the 312 dp walk
over Home, every lobby (game × enforced × seated × segment), both deck box
seats and every settings tab with and without developer settings.

**Shots** (taken on the harness, not kept): Home, the lobby in each opponent
segment and settings › play at the four sizes, the deck box for both seats
and every settings tab at 1280×800.

## The verb grammar

U6 makes every card input pass through one classifier and one meaning per
gesture ([ux.md §4](ux.md)): tap selects, a second tap or double-click fires
the card's one default action, long-press or right-click pins the inspector,
a drag starts only past a real distance, and everything a card can do is a
visible, labelled chip on the card itself.

- **The classifier** (`gesture.rs`). `classify(press, now, held_ms,
  moved_max, kind)` is the pure rule: past the slop — 8 dp on touch, 4 dp on
  a mouse — the press is a `DragStart`; still for 500 ms it is a
  `LongPress`; otherwise it is a `Tap`. `Tracker` is the state machine the
  observers drive: `press` from `Pointer<Press>`, `moved` from every
  `Pointer<Drag>` (bevy_picking's own drag starts at zero distance, so its
  `Drag.distance` is measured against the slop and the card lifts only at
  the first `DragStart` the tracker returns), `tick` from an `Update`
  system for the long press, `release` from `DragEnd`, `cancel` from Esc.
  A release after a lift is a `Drop`, after a long press a `Cancel` (so the
  click bevy still sends is swallowed), and a second tap within 300 ms and
  24 dp is a `DoubleTap`. `RecentDrag` is set only after a real lift, so a
  jittery finger's click reaches `on_click_card` as a tap.
- **Chips** (`chips.rs`). `offers(view, rims, table, mirror, me, card)` is
  groups 1–2 of §4.3: the affordances naming the card (a disabled one keeps
  its place with `not available right now` as its second line), a `play`
  chip for a legal Play/React row (to the chain when the row lists it, else
  one per zone), `move › {zone}` per March destination, and the hidden-card
  offers (`hide at … · 1 rune`, `play from hidden`, `reveal`) that used to
  live in the strip's hover-only row; a greyed card with no offer gets a
  disabled `play` chip carrying the old `GREYED_HINT`. `chips()` appends
  `inspect` and, on a free table, `exhaust`/`ready`, `trash` and `recycle`.
  `default_action` is the one enabled offer, or nothing. `card_chips_ui`
  draws the row 6 pt below the hovered-or-selected card (above a hand card)
  in an `Area` constrained to the stage, four chips then `more…`, each
  numbered for the 1–9 keys and Enter; the row survives the pointer moving
  onto it. `Act` is the one performer: an affordance fires through
  `Sender`, `play`/`move`/`hide` write `CardDropped`, `reveal` writes
  `RevealRequested`, `inspect` pins. The strip's chips carry the same digits
  (`chip_label`), and `strip_chips` no longer lists any affordance that
  names a card — those are answered on the card.
- **The click plan** (`interaction::click_plan`) now takes the rims: a card
  an enabled affordance names still answers the prompt on one tap (the
  Answer rim is visible state); otherwise the first tap selects, and the
  second tap fires the default chip, else plays a hand card or my facedown
  card to the chain, else exhausts on a free table, else nothing. Under
  rules enforced no second tap ever exhausts.
- **Drags.** `on_drag` lifts a card only when the tracker says `DragStart`
  and `may_lift` allows it (rimmed, or a free table). While held,
  `Rims::destinations` now carries every legal row's zones (not only a
  march's), `tint_layers` colours them by the card's own rim (Hide zones in
  the hide colour) and `march_tint_ui` dims every other zone by 30 %;
  `draw_arrows` plans one provisional arrow from the card's origin slot to
  the zone the drop would land on (`arrows::provisional`, orange for a
  march, blue for a play). Dropping: `drop_plan` reads the row — play,
  hide, both (the **drop chooser**: `play here` / `hide here` at the drop
  point, no timeout, cancelled by a tap elsewhere or Esc) or refuse — and a
  release on the felt under rules enforced snaps to the nearest lit zone
  within `DROP_REACH` (`snap_zone`) or writes nothing; a free table keeps
  the old board/hand drop. Both drop handlers ignore a card that never
  lifted.
- **GroupMove as a batch.** `plugin_ui::prompt_kind` reads the presenter's
  `move others to {zone N} too?` and `set aside up to N cards to redraw`
  verbatim. For a GroupMove the other ready units already pulse with the
  Answer rim and a tap sends one pick; `march_all_ui` floats an `all · move
  N` chip on the destination zone and `drive_march_all` sends one card
  option per view refresh until only `done` is left, then `done`; the
  primary reads `move N` (`primary_label`, N = the first unit plus the
  picks). The mulligan gets its overlay in the `banner` slot: the hand as
  faces, a corner × on each card set aside (`mulligan_marks`: the options
  that vanished, plus `LastIntent` once the prompt is full and no option
  names the rest), and the primary reads `keep · N set aside`.
- **Pins, Esc and the keys.** `Pinned(Option<Entity>)` is set by a long
  press, a right-click, the `inspect` chip or I; the inspector prefers it
  over hover and selection; a tap elsewhere or Esc clears it.
  `escape_ladder` runs in `PreUpdate` after the input systems and walks the
  drop chooser → a drag in progress → the prompt's cancel → the selection,
  clearing the key's just-pressed state when it consumes it so the table
  menu toggle (settings.rs) sees nothing; with nothing to cancel the key
  passes through. Tab / Shift+Tab cycle `Selected` through the highlighted
  and rimmed cards (`cycle_order`, `step`), ← → walk my hand. `ClaimedKeys`
  records the key the plugin fired this frame (`plugin_hotkeys` claims,
  `claims` is the pure rule); every kai handler asks `kai_key` first, so a
  key never fires twice, and `key_of` no longer maps the letters kai owns
  (I H P R L C K D T E) while it gains the digits. H/P/R/E/T act on the
  focused card only when the matching chip exists (`chips::by_key`).
- **Touch.** `settle_hand_hover` stands down under `InputKind::Touch` and
  `mirror_touch_selection` keeps `Hovered` on the selected card, so the
  raised hand card is the selected one until the selection clears — the
  hover-driven fan in `layout.rs` needs no change.

Seams left for the milestones around this one: `primary.rs` calls
`plugin_ui::primary_label` for the `move N` / `keep · N set aside` words
(one line); `mod.rs` registers the two modules, the resources and the
systems; the opponent plate's `mulliganing…` and the drawer auto-raise on a
hand prompt are U11/U7 items; the M0 seat-colour affordance rim still draws
under the Answer rim until U9's rim pass.

## Automation

U8 makes the client stop asking about what the player cannot influence
([ux.md §4.6](ux.md)). Everything is a pure decision in `auto.rs` over the
`PluginView` and the play preferences; one Update system (`auto_pilot`)
turns the decision into the same `Sender::fire` a button press makes.

- **`decide(view, me, prefs, stops, hold, through) -> Decision`.** A prompt
  for another seat waits (`Theirs`). A prompt for me is the auto-answer
  case below. Otherwise the pass affordance (`primary::pass_index`) is the
  only thing automation ever presses — end turn, the roll and a prompt's
  done/keep are never auto-pressed, so a hand of zero-cost cards cannot leak
  through a green button. With pass-through armed the pass fires regardless
  of a response; else auto-pass off waits (`Off`), a hold waits (`Hold`), a
  stop set on this phase for this side of the table waits (`Stop`), and any
  move at all waits (`Response`): `has_move` counts every legal row of any
  kind — Play, March, Activate, React, Answer and Hide — and every enabled
  offer besides the pass, the free-table offers and the roll's reveal. That
  is stricter than the primary's `plays_remain` (which ignores March and
  Activate because they do not colour the button), and it is exactly the
  soak's manual seat: `random::options` is `[pass]` if and only if
  `has_move` is false.
- **The AI seat asks the same question.** `decide`'s "anything to do" is
  `auto::offer`, which the bot's pilot ([bot.md](bot.md)) runs before it
  wakes the model: the bot passes, ends an empty turn, answers forced picks
  and sends a lone roll on its own, and the model's `hold` tool sleeps
  through a stretch until a named condition. The player's rules here are
  unchanged.
- **The delay.** Every firing carries `after_ms = AUTO_DELAY_MS` (600) — the
  same number whether the pass is automatic or a pass-through over a live
  response, so timing leaks nothing [MTGA-8]. `Timer::tick(view, after_ms,
  now)` arms on a view, fires once when the same view has stood for the
  delay, and never fires twice for one view: a refused or stale press
  leaves the view unchanged and the timer stays quiet until the next view.
  Views compare by equality, so any status change re-arms the delay.
- **`HoldFocus`.** `Off`, `ThisPhase(PhaseKey { turn, seat, phase })` or
  `Held`. Ctrl-click on the primary toggles this phase, Ctrl+Shift-click
  toggles held (`primary::click_of`); `settled(view)` clears a phase hold
  the moment the presenter's turn line names another turn, seat or phase,
  and `chip()` is the word the turn plate carries in amber beside the mode
  chip ("holding this phase" / "holding").
- **`Stops`.** A `BTreeSet<Stop { phase, mine }>` on `Tuning`, so it
  persists with everything else in `tuning.json`; `set(phase, mine)` is
  the lookup `decide` makes against `phase_of(view)`. Settings › play lists
  every presenter phase but setup with a my-turn and a their-turn box
  (`settings/play.rs`); the phase bar's click-to-stop on pointer classes is
  U11's, when the view carries the phase list. `PHASES` is the vocabulary
  and a test holds it equal to the engine's `Phase::ALL` labels.
- **Pass-through.** Shift+Space or Shift-click arms `PassThrough(Some(Mark
  { turn, items, prompt }))` while a pass is offered; the primary reads
  "passing…" (`primary::shown_label`). It passes through every window,
  response or not, until `Mark::something_new`: a chain item id it has not
  seen, a prompt opening, or the turn number changing — and any key press,
  mouse button or touch disarms it (the arming Shift+Space excepted).
  Comparing item ids rather than `chain.len()` means the chain shrinking as
  items resolve does not count as news.
- **Auto-answer.** With "ask me anyway" off, a prompt for me is answered
  when its enabled options are exactly the remaining forced picks: `min ==
  max`, not optional, no cancel/skip/no on the strip (`cancel_index`), no
  enabled non-card option (a zone, a seat, yes/no), and as many enabled card
  options as `max - picked`. Picks go one per delay, in the presenter's
  order, and the view's `picked` count re-arms the timer between them.
  "order my triggers myself" and "assign combat damage myself" opt the
  `order your triggers…` and `assign N damage: who takes lethal next?`
  prompts out by their verbatim phrasing (`ORDER_TRIGGERS_WHY`,
  `ASSIGN_WHY`/`ASSIGN_WHY_TAIL`). The engine already auto-answers a
  single-option prompt (`prompts::auto_answer`), so this mostly catches
  forced multi-picks such as discarding your whole hand.
- **The end-turn confirm.** Off by default (a per-turn tax); on, an amber
  end turn under rules enforced (`needs_confirm`) needs a second press
  within three seconds (`confirm_step`), and the button reads "press
  again" meanwhile. `plugin_hotkeys` holds back the plugin's own Space
  while `primary::confirm_guards` says the confirm applies and
  `is_primary_key` takes Space instead, so the key, W and the button all
  honour the confirm. A second, always-on reason shares that same
  mechanic: `primary::TurnActivity` tracks whether the acting seat has
  fired anything other than pass or end turn since the current
  `(turn.number, turn.seat)` began (`observe_turn` resets it, `note_action`
  sets it, hooked into `hud::Sender::fire` and into `plugin_hotkeys`'
  direct write so every action path is covered). `needs_confirm` asks for
  the second press when `confirm_end_turn` says there are still plays
  *or* `TurnActivity` says nothing was done this turn, regardless of the
  setting — ending a turn cold, with nothing played, moved or activated,
  always gets the one-click safety net even when the "still have plays"
  preference is off.
- **Settings › play** (`settings/play.rs`) is the first play tab with
  preferences behind it: `TOGGLES` binds each label to a `Tuning` field —
  `auto_pass` (on), `ask_anyway`, `order_triggers`, `assign_damage`,
  `confirm_end_turn`, `fast_anim`, `hand_left` — then the UI scale slider
  (0.8–1.5, copied into `viewport::UiZoom` by `auto::sync_ui_scale` so the
  style route applies it) and the stops list. `fast_anim` gates the beats
  (`anim.rs`), `colour_blind` and `theme` reach `theme.rs`, and `hand_left`
  is `Fit::hand_left` in `hud::layout`: on phones it mirrors the primary,
  secondary and `bottom_left` slots to the other edge
  (`hand_on_the_left_mirrors_the_primary_row_on_phones`); desktop and tablet
  ignore it, since their primary sits under the column.
  An old `tuning.json` loads with every new field at its default
  (`the_play_preferences_default_on_an_old_file_and_round_trip_with_the_stops`).

**The soak proof.** `kai-cli soak --brain-a auto` plays the desktop seat:
`soak::auto_choice` runs `auto::offer` over the seat's view before the
random pick: a pass, an empty turn's end turn, a forced pick and a lone roll
are sent first (the desktop's `decide` never presses end turn or a roll for
the player, see [bot.md](bot.md)). When it fires, the soak asserts the
manual seat's option list is exactly that one action — if the auto-pass
ever fired with a play open the game ends `Stuck` and the soak fails — and
then sends it through the same `random::pick` draw the manual seat would
have made, so the seed's stream stays aligned and the two seats play the
same game move for move. `Table.auto_fired` counts the forced passes. The
test `the_auto_passing_seat_plays_the_same_game_as_the_manual_seat_on_the_same_seed`
runs two games each way and asserts identical records and host logs with at
least one auto-pass; a four-game soak at seed 4242 (`kai-cli soak --seed 4242
--brain-a auto`, rerunnable) matched on every game with 50 auto-passes.

## History rail, drawer, badges and rims

U9 lands the four parts of [ux.md §3.13–3.14 and §7](ux.md) that read the
game back to the player: what just happened, the log and chat and tokens in
one drawer, the status glyphs under a unit, and a rim system where colour is
never the only signal.

**The history rail** (`history.rs`). The presenter carries its narration as a
sliding twelve-line window inside `status` (`NARRATION_LINES` in the
riftbound-turns blob), mixed with the turn line, the scoreboard and the rest.
`refresh_history` filters the window through `classify_status` (only
`StatusLine::Narration` lines count) and `History::absorb` appends what is
new: `fresh(previous, window)` finds the longest suffix of the last window
that is a prefix of the new one, and everything past it is fresh — so a
sliding window adds only its new lines, a repeated line ("{seat 1} passes"
twice) stays distinct, and an unrelated window (a new game) is all new.
Every line becomes an `Event { class, seat, card, zone, text, at }`:
`classify` reads the first token and the verb — `{seat N} plays` → Play,
`{card N} dies` → Death, `conquers` / `holds` / `keeps` / `wins the combat at
{zone}` → Conquer, `is attached` / `detaches` / `is recalled` → Attach,
`draws` / `burns` / `channels` → Draw, `hides` → Hide, `reveals` / `looks` →
Reveal, `triggers` / `activates` → Trigger, `passes` → Pass, `wins with N
points` → Win, anything else → Note. The tokens give the event its seat,
card and zone. The rail draws `History::rail(tiles)` — the newest board
events first, passes and notes skipped — into the `history` slot at one
tile per `HISTORY_TILE_H` of pitch: the card's thumbnail (or a seat swatch
when the line names no card), a border in the acting seat's colour, and a
painted glyph for the class in the corner (a triangle for play, a cross for
death, a flag for conquest, two rings for attach, a card outline for draw, a
filled card for hide, an eye for reveal, a bolt for trigger, a star for the
win). Hover shows the sentence expanded through `plugin_ui::expand` beside
the tile and writes `HistoryHover` for the inspector; a click selects the
card on the table. Refusal toasts join the same list as `EventClass::Toast`
(from `Refusals.current`, keyed by its `at`), and `History::trim` keeps the
last five of them beside up to `KEEP` (200) narration lines, so the log tab
reads the last five refusals without them evicting the narration. Leaving
the session clears the history. The structured `narration` list of U11
replaces the window merge; the classifier stays.

**The drawer** (`drawer.rs`). `hud::Drawer(DrawerState)` — the resource the
layout already reads for the desktop and tablet column — is now driven:
`DrawerPanel { tab }` names the tab (log · chat · tokens), `toggle(open,
current, wanted, free)` is the one rule (the same key again closes, another
key switches, tokens need a free table), and `set` writes both resources.
`drawer_keys` runs in `PreUpdate` before `escape_ladder`: L and C toggle
their tabs unless a text field has focus or a sheet, the settings or the
table menu is above; Escape and the back key walk `ladder_rung` — a token
placement in progress is cancelled first, then an open drawer closes — and
clear the key so the interaction ladder and `menu_keys` never see it. K stays
with `ui::hotkeys` (it is a free-table verb and inert under rules enforced):
`sync_tokens_tab` mirrors `TokenPanel.open` both ways, so K opens the drawer
on the tokens tab, K again closes it, and closing the drawer or leaving the
tab clears the flag. `drawer_tabs_ui` draws the three 40×28 tabs under the
primary (`drawer_tabs` slot; the tokens tab only on a free table) and
`drawer_ui` fills the `drawer` slot (a right-side sheet on phones, which have
no tab slot) with a tab chip row, a ×, and the body: the **log** is every
history event newest at the bottom, each sentence split by `segments` into
plain text, seat names in the seat colour, zone labels and card names as
tappable links that select the card for the inspector, toasts in amber with
the refusal glyph; the **chat** is the AI seat's chat from the old
`src/ai/window.rs` (deleted): the status line, the `you:` / `bot:` lines,
the draft, `switch model` over `PRESETS` and `stop AI` (`model_controls` for
Settings › advanced lives here too; on wasm the tab says the AI runs in the
desktop and Android builds); the **tokens** tab is below. Nothing floats but
the strip and the drawer.

**Tokens as a placement mode** (`tokens.rs`). The window is gone. The tab
lists `choices` — the manifest's tokens plus the custom name and might — as
48 dp buttons; pressing one sets `TokenPanel.placing` and tucks the drawer
so the board shows. `placement_ui` then lights every candidate zone
(`spawn_targets`: the battlefields and my own base, through
`highlight::tinted`) in the Answer colour with the prompt pulse, draws a
"tap a lit zone to place X · esc cancels" plate at the top of the stage with
a cancel, and on the first release inside a lit quad (`inside`, a convex
point test over the projected corners; the release that pressed the button
is ignored) emits `TokenSpawn` through `spawn_for` — a shared zone spawns
for seat 0, my base for me — and leaves the mode. `leave_placement` drops
the mode when the table stops being free or the player leaves.

**Badges** (`counters.rs`). `dress(mirror, card, printed_might, seat_rgb)`
turns a unit's counters and statuses into `Dress { badges, frames }`: a
might chip reading `3›5` in green above the printed value and `3›2` in red
below (`3` in the counter's own colour when unchanged), a damage chip `−2`
in red, glyph chips for the statuses — a sword in the attacker's seat colour,
a shield for the defender, a gear for equipped, an hourglass for temporary —
and frames instead of chips for stunned (frost) and empowered (gold); other
counters keep their label. Exhausted stays the tilt, plus a 60 % dim
(`Rims.dim` from `exhausted_cards`, `EXHAUST_LEVEL`; an unaffordable card's
45 % grey wins). Chips are 20 pt pills stacked rightward along the card's
own bottom edge: `stack(widths, more_w, card_w)` shows at most three, drops
the tail behind a `+N` chip, and shrinks the shown count until the row fits
the card's projected width — so a dense zone never has chips crossing the
card below. Glyphs are painted, not typed, so they do not depend on font
coverage (U10's asset set replaces the paint).

**Rims** (`highlight.rs`). `RimKind::LEGALITY` is the five of ux.md §7 and
`rim_style` pairs each with a stroke: Play solid 2, March solid with a
chevron on the top edge, Activate solid with a filled corner tag, React
dashed, Answer solid 3 pulsing; Hide is the Play colour dotted (`same_family`),
Enemy is the ring outside. `rim_shapes` turns a projected ring into those
shapes (`top_edge` picks the edge nearest the top of the screen for the
chevron and tag). `Palette::of(&tuning)` picks the colour set: the standard
one, or the colour-blind one where Play/Hide turn blue, Enemy turns orange
and Activate yellow so the pairs stay apart; `rim_rgb_in(kind, palette)` is
the table, `rim_color`/`rim_rgb` remain the standard shorthand. The tests
assert the five legality hues pairwise ≥ 60 apart and ≥ 60 from the enemy
ring in both palettes, every kind at least 55 from every seat colour and 50
from every arrow tint in both palettes, no two kinds sharing a stroke
pattern, Hide and Play sharing the hue and differing in stroke, the
precedence order Answer › React › Play › Hide › Activate › March, and a
faded rim staying nearer its own hue than any other family's. The M0
seat-colour affordance rim is gone: a prompt candidate is an Answer card. `colors.rs` adds the seat glyphs of the
colour-blind palette — circle, square, triangle, diamond, star for the five
pickable colours, ring, bar and cross for the reserved tail — as painted
shapes (`paint_glyph`, `glyph_of_rgb`) and `swatch(ui, rgb, size,
colour_blind)` draws a swatch with its glyph beside it; the history rail, the
log, the seat plates, the chain rows and the lobby's seat rows all draw
through it, so the glyph appears everywhere a seat colour does once
Settings › look ticks `Tuning.colour_blind`.

## Discoverability and the visual pass

U10 lands [ux.md §6 and §7](ux.md): the three coach marks, the idle hint,
the rim legend tags, the help sheet, touch tooltips, the two token sets, the
glyph set and the chain-resolution beat.

**The coach** (`table/coach.rs`). `Seen { marks, hints }` is the per-device
memory, persisted as `Tuning.coach` so it rides tuning.json and "show hints
again" (Settings › play) is `Seen::reset`. `next_mark(view, me, rims, seen)`
picks one mark at a time in the order the player meets them: `Prompt` when a
prompt is mine, then `Deal` when a hand card is lifted as playable
(`Rims.lift`), then `Pass` when the primary is enabled on my action. A mark
is dismissed by doing the thing — `done(mark, dragged, any_fired,
primary_fired)`: a `Held.card` or a `CardDropped` for the deal, any
`PluginActionRequested` for the prompt, one whose bytes match the primary's
affordance for the pass — or by **got it**; either way it is seen for good.
`coach_ui` draws the 240 pt callout with `place_callout(anchor, side, size,
safe, stage)`: above the first lit hand card, above the first pulsing card (or
under the strip when none has a face), left of the primary on desktop and
above it on phones, clamped into the safe rect and pushed out of
`keep_out(stage)` — the middle half of the stage — so it never sits on the
felt's centre. An amber connector runs from the callout to the anchor.

**The idle hint.** `idle_hint(view) -> Option<Hint>` answers only when
`acting(view)` and no prompt is open: the strongest rim kind in `view.legal`
(the `RimKind` order) with the primary's label, or the primary alone when
nothing is lit. `Hint::text(palette)` chooses the colour word from the live
palette ("drag a green card to play it, or press end turn" becomes "drag a
blue card…" under the colour-blind palette). `refresh_coach` times it: six
seconds after the view last changed with nothing held and no mark showing,
the hint shows for eight, is counted in `Seen.hints` (`HINT_TIMES` = 2 per
hint per device), and is not re-evaluated until the view changes again. It
is drawn in the strip's rect on desktop and tablet when `hud::strip_idle`
says the strip is empty and no refusal is on it, and in the ticker on phones.

**Rim legend tags.** The first time each `RimKind` appears in a session
(`Coach.tagged`) a 12 pt tag with `RimKind::legend()` in the rim's colour
sits beside the card's top-right corner for three seconds. The session's
memory resets when the table session ends.

**Touch tooltips.** `coach::touch_tip(response, label)` is `on_hover_text`
on a pointer and, after `Response::long_touched`, the same label shown for
1.5 s; the ≡ button uses it. The drawer tabs and the sheet × are the next
takers.

**The help sheet** (`help.rs`). `?` (the logical `Key::Character("?")`, so
any layout) toggles `HelpSheet.open`; the table menu's **help** entry opens
it; Escape closes it in `help_keys` before the drawer and the ladder see the
key. The body is `VERBS` (the §4.2 table as fixed two-column rows — action and
key on the left, mouse and touch on the right, mouse omitted on phones),
`HOTKEYS` (§4.7), the rim legend with a painted sample of every stroke through
`highlight::rim_shapes` in the live palette, the status chips with glyphs from
the icon set, and the three coach-mark sentences.

**Themes** (`theme.rs`). `Tokens` holds the nine colours of §7 plus the
hairline and a `dark_mode` flag; `DARK` and `LIGHT` are the two sets and
`Tokens::of(Resolved)` picks one. `resolve(Theme, system)` maps the setting
to a `Resolved` theme — `System` follows `WindowThemeChanged` and is dark
until the window reports one. `ThemePlugin` keeps `ActiveTheme` current,
publishes the tokens into egui memory (`theme::tokens(ctx)`, the accessor the
hud constants will move to), sets `ClearColor` and the ambient brightness,
and `tint_felt` multiplies every `SeatDecor` material's original base colour
by `felt_tint` — identity in the dark theme, a warm lift in the light one —
remembering the original so the tint is exact when the theme flips back.
`contrast` is WCAG relative luminance; `best_ink(fill)` picks the readable
ink for a fill, and the test holds every chip pair at ≥ 4.5 : 1 in both sets
(the light green is `#147644`, darker than the design's `#1B8F4E`, which
read 4.45 : 1 under white ink). `Tokens::visuals()` builds the egui visuals
for a set, ready for the frame swap. Settings › look grows the theme control
and the colour-blind checkbox. The table HUD stays on the dark constants by
design; Home, the lobby, the deck box, the settings and help sheets and
`hud::sheet` itself read `theme::tokens(ui.ctx())` for their ink and
surfaces and call `theme::dress(ui)`, which swaps that `Ui`'s egui visuals to
`Tokens::visuals()` when the published set differs from the style's, so a
light theme lightens the menus and every sheet along with the room while
the HUD's plates, strip and chips stay dark. The sheet panel is filled with
`surface_opaque()` so nothing beneath it ghosts through.

**The glyph set** (`assets/icons/*.svg`, embedded through `theme::ICONS`).
Twenty-one hand-authored 24-unit SVGs (‹ › × ⎵ ● ◉ ◐ ⬢ ⚡ ≡ ? ✓, the sword,
shield, gear and hourglass, an eye, a hand, a star, a drag arrow and a
spinner) parsed by a small reader of `path` (M L H V Z), `circle`, `polygon`
and `polyline` into `Glyph { marks }` and painted with `paint_glyph` at any
size in any colour, so no icon depends on the font's emoji coverage.
`theme::icon` and `theme::icon_button` are the widgets. The idle hint and the
help sheet use them; the badge painter in `counters.rs` and the `⎵` on the
primary are the next takers.

**The beat** (`table/anim.rs`). `Beats` remembers the last chain and, when a
row's item leaves it between two views (`resolved_between`), pushes a `Beat`
for the row's card — staggered by `BEAT_STAGGER` when several resolve at once,
none when `fast_anim` is set. For `BEAT_SECS` the card lifts by `beat_lift`
and scales by `beat_scale` inside `animate_cards`, and `beat_rings_ui` draws a
ring in the controller's seat colour expanding from the card and fading.
`interrupt_beats` clears them on any key, button or touch press, so feedback
never costs the player their turn.

## Structured view fields, the phase bar and stops

U11 lands [ux.md §10 U11](ux.md): the presenter says in fields what it used to
say only in lines, kai prefers the fields with the parser as the fallback, and
the phase bar puts the U8 stops on the desktop and tablet screens.

**The fields** (agni `sim/src/wire.rs`, `plugins/sdk/src/view.rs`). `PluginView`
gains six serde-defaulted, skip-when-empty members, so `WIRE_VERSION` stays at
5 and a view without them decodes exactly as before: `turn: Option<TurnInfo>`
(number, seat, phase, the nine phase labels, mode), `seats: Vec<SeatInfo>`
(points, victory, xp, hand, deck, ready and total runes), `waiting: Option<Waiting>`
(the seat or none for "every seat", and what for), `narration: Vec<String>` (the
blob's log), `primary: Option<u16>` (the affordance the primary button fires)
and `hidden: Vec<u16>` (affordances that live only in the table menu).
`PluginView::shown()` iterates the affordances that are not hidden. The SDK
builders are `turn`, `seat` (kept sorted by seat), `waiting`, `narrate`,
`primary`, `primary_last` and `offer_hidden`; the encoder writes the six keys
after `chain`, each only when present, and the wasm boundary is proven in
`net/tests/riftbound_turns.rs` and the plugin crate's ABI test.

**The presenter** (`games/riftbound-turns/src/present.rs`). `free`, `enforced`
and `lobby` all fill the fields; `seat_info` counts the hand, the main deck and
the rune pool from the snapshot per seat. The primary is `end turn`, `pass`,
`roll`/`reveal` and a prompt's `Answer::Done` option (done, keep); who goes
first is a choice between chips and marks none. Every old status line keeps
being emitted for one release, so `classify_status` still works against a
newer plugin and an older one still drives the plate.

**Hidden affordances.** `concede` is offered to every seated player while the
game is on (`TurnEvent::Concede`, byte 14): the blob records the seat in
`conceded`, `Ctx::winner` names the last seat standing once every other seat
has conceded, and `win_check` narrates "wins by concession" instead of the
points line. `confirm free table` and the new `withdraw free table` are hidden
too. kai keeps hidden affordances off the strip (`plugin_ui::strip_chips`,
`ordered`, `highlighted`), out of `plays_remain` and `auto::has_move` (so a
concede never keeps the primary amber or holds auto-pass), and out of kai-cli's
numbered list and fallbacks (`shown()` in `describe`, `actionable` and
`fallback`), which is how "confirm free table" leaves the AI's list. The table
menu finds them by label as before.

**The free-table proposal** expires with the proposer's turn (`expire_free_table`
runs when the turn advances, in both the engine and the free path) and is
withdrawn by the proposer sending `FreeTable` again; `Refusal::ConfirmPending`
became `AlreadyConceded`. The menu reads "withdraw the free table proposal"
while it stands.

**kai reads the fields first.** `plate::turn_of`, `waiting_of` and `mode_of`
return the field when the view carries it and the parsed line otherwise;
`turn_plate` and `auto::phase_of` go through them. `seat_plates` takes points,
victory, xp, hand and deck from `seats`, then the mirror's counters, then the
points line, and shows "runes ready/total" on the desktop plate when the field
is present. `history::narration` returns the field when non-empty and the
classified lines otherwise, so the rail, the ticker and the log tab take the
presenter's own list. `strip_state`'s waiting row reads `waiting_of`.

**The phase bar** (`plate::phase_bar`, `phase_bar_ui`). Only when the view
carries the phase list, only in the `phase_bar` slot (desktop and tablet) and
only on a pointer (`phase_bar_shown`): nine dots under the turn plate, the
current phase filled in the turn player's colour, a bar above a dot for a stop
on my turn (my seat colour) and below it for a stop on their turn (amber).
A click toggles the stop for the turn being shown and a right-click the other
one (`chip_clicked`), the hover text spells both out, and the stops are the
same `Tuning.stops` the Settings › play list edits, so `save_tuning` persists
them and `auto::decide` honours them on the next window.

## The phase-3b review

The review of U5–U11 ran the harness at the four sizes against a kai-cli
seat and filed its findings against the landed code; this is what changed
for them, by mechanism.

**Visibility is `layout_cards`' alone.** `hide_viewed_hand` used to force
every `CardView` to `Inherited` each frame, so a card `layout_cards` had
left unplaced (a chain card, a hand card scrolled out of the drawer)
rendered face-up at its deal origin — the opponent's spell face-up in its
hand fan, my own spell on the chain *and* on my fan. `hide_viewed_hand` now
touches only the opponent-hand backs and `ZoneDecor`; `layout_cards` takes
the menu and writes `card_visibility(at_table, placed)`. With that, a prompt
whose only candidates are chain items opens the tray (the candidate is no
longer "shown"), a chain row wearing an Answer candidate is itself the
button (`chain::answer_for`, drawn with the Answer rim), `arrows::item_anchors`
prefers the panel row over a card on the felt, and the tray stays closed
during a mulligan, whose overlay already lists every hand card.

**The press always closes.** bevy_picking emits `Click` before `Release`
and `DragEnd` only after a drag, so a still tap left its `Press` in the
`Tracker` and `tick_gestures` turned it into a long-press 500 ms later.
`on_release` (`Tracker::settle`, a global `Pointer<Release>` observer) closes
any press that was not lifted; `on_cancel` drops a lifted one and puts the
cards back. On phones the tucked drawer owns the tap: `sync_hand_pickable`
marks camera-facing cards `Pickable::IGNORE` while the drawer is tucked and
nothing is held, and the drawer's egui area shrinks to its grab strip while
raised, so a raised card is the 3D entity under the finger. A horizontal
`DragStart` on a raised card is the swipe (`swipe_starts`, `DrawerSwipe`
scrolls `DrawerScroll` from the drag distance); a vertical one lifts. This
is also what made the 800×360 drag inert: the raised drawer's full-band egui
area captured the pointer.

**The stage ends at the primary row; the frame does not move.** `HudRects`
gains `frame`, the stage computed with the drawer tucked; `camera::fit_table`
frames phones from it, so raising the drawer never re-solves the camera and
the battlefields stay in view while the hand is open (the drawer covers the
near band instead). `stage` itself now ends above the primary row, so chips
constrained to it never cover the button, and coach marks carry an avoid
list (`callout_avoids`: primary, secondary, bottom_left, hand) that
`lifted_off` walks. In landscape `raise_to_floor` pins the far edge
(`pin_far`, `crops_near`) so the card floor crops my outer band, not the far
seat's base. The chain rail collapses to zero while the chain is empty
(`rail_height`) and floats over the stage top only while a card is held
(`chain::rail_rect`); the banner sizes to the rows its state needs
(`StripFit::Rows`) and carries a painted chevron that collapses it to 28 dp
(`hud::Banner`). The primary ignores a press within 300 ms of a drawer state
change (`drawer_guarded`) the way `RecentDrag` guards drops, drops the key
hint on phones and steps its label size down to fit (`label_pt`);
`primary_for(view, me)` reads "choose" for my own pick instead of "waiting".

**Rims on drawer cards are screen-space.** Fanned cards overlap by 12 dp and
the 0.12 outset ring of each crossed its neighbours' faces, which read as
"thin vertical line pairs". `drawer_rim_rects` clips each rim to the visible
slice of its card and insets it 2 dp; felt cards keep the projected ring.

**Sheets keep the touch style at the table.** `apply_ui_style` stores the
menu style in the egui context and `viewport::dress_sheet` applies it inside
`hud::sheet`, so the settings sheet's rows are 48 dp at the table too. The
turn plate replaces the mode chip with the hold chip (`plate_chips`) and
takes a long-press for the hold (`hold this phase`, then `hold`);
`TURN_PLATE_W` is 200. The refusal toast stacks above the chip row of the
same card (`chips::ChipRow`, `toast_top`). `plugin_hotkeys` leaves Shift+Space
and Shift+W to kai (`plugin_takes`), `key_of` refuses every key in `KAI_KEYS`,
and `escape_ladder` shares `interaction::Covers` with `drawer_keys` so an open
sheet, help, the table menu or the chain sheet owns the key.

**Elsewhere.** Hosting refuses an enforced lobby whose plugin failed to load
(`net::enforced_without_plugin`), `seed_one` re-publishes a bundle whose held
version is not trusted, `pinned::dealt_here` gates "already on the table" on
a live session, `pool::label_for` also matches by (zone, name, count), the
mode switch of the opening (`switch to free table`) is a table-menu row with
a confirm, `advanced::newest_plugin` reports the newest held version,
kai-cli `spawn` joins a multi-word token name, the lobby's deck links are
48 dp `menu::link` rows and battlefield tiles size from the sheet
(`battlefield::tile_fit`), and `hand_left` mirrors the primary row on
phones. In agni, Fizz's replay filters by affordability, an uncancellable
play whose cost no candidate can pay is taken back with a narration instead
of a prompt with only refusable answers, and `Cost::label` groups identical
power needs.

## The AI seat runs in-process

The AI opponent used to be a spawned `kai-cli --ai` child beside the desktop
binary, so Android — which ships no sidecar — had no AI at all. It is now a
seat inside the app on every native target, and the machinery it runs on is
shared with `kai-cli` rather than copied from it.

**One driver, two links** (`src/ai/driver.rs`). Everything that used to be
`kai_cli.rs`'s seat — the `Seat` replica (`ClientSession` built through
`modules::prepare_join` with the same pin checks the desktop client makes),
`describe` and the text state, the command set (`run_command`: do, move,
play, hide, reveal, draw, trash, recycle, spawn, exhaust, counter, playmat,
deck, battlefield, deal), the chat file tail (`Chat`), the LLM decision loop
(`think`, keyed by seq and the offered labels, with the idle `fallback`
press) and the reveal automation — lives in the library as `Driver::tick`.
`Driver::steer` runs the pilot first ([bot.md](bot.md)); `think` is only
called on `Step::Model` and receives the hold's note as `Situation.held`.
The transport is a `Link` trait with two implementations: `BridgeLink` is
`agni_net::bridge` (kai-cli over the mesh) and `ai::local::ChannelLink` is a
pair of std channels to the host in the same process. `Mind` is `Random`,
`Auto` (random with the desktop auto-pilot's forced passes sent first, the
soak's `auto_choice`) or `Llm(Brain)`; `kai-cli --brain random|auto|nanogpt`
picks one and `--ai` still means nanogpt. A random or auto mind sends one
intent and then waits until that intent has folded (its own entries are
counted by `Seat.own_folds`) or been refused (`Seat.refusals`) before it
decides again, with `FOLD_WAIT` as the safety net — deciding on every fold
re-pressed *roll* while the first press was still in flight. A commit is
idempotent until revealed (`RollSecrets::commit` keeps the secret it holds),
so a repeated press cannot break the reveal. The random mind logs only its
picks and the host's notices; the LLM keeps the full state dumps in the log.

**Admitted as a network joiner would be** (`src/ai/local.rs`,
`src/net/mod.rs`). The local seat is a peer connection with a synthetic conn
id (`CONN_BASE = 1 << 62` and up, never a transport id): `local::start`
spawns the `kai-ai-seat` thread, which sends `ClientMsg::Join` down its
channel, and `net::drain_net` extends the bridge's events every frame with
`local::events()` — `PeerJoined` once, then every `ClientMsg` the seat sent
as a `PeerFrame`, and `PeerLeft` when the thread finishes. From there the
host does exactly what it does for a peer over iroh: `HostState::peer_message`
(the body of the old `handle_peer`, now a method so a test can drive it)
seats it with `join_as("local:ai", "bot")`, broadcasts the join entry and
the roster, sends `Welcome` with the log and the faces it is owed, folds its
intents, deals its deck and answers refusals with a notice. Outbound, every
`HostMsg` goes through `net::send_to`, which hands a local conn's message to
`local::deliver` (the seat's channel) and everything else to
`bridge::send_to`. `Conns::deliver` — the entries broadcast plus
`owed_faces` routed per seat — therefore reaches the AI's replica through the
same path as any joiner's private faces; there is no third path. The host's
own actions go through `HostState::own_intent`/`own_deal`, which the routes
share.

**Lifecycle.** `start` replaces any live seat, truncates `seat.log` and
`chat.log` under `os::paths::config_dir()/ai/` (the notes at `ai-notes.md`
beside them; that directory exists on Android), and the seat picks its deck
from the lobby's choice or, for a free brain with "let the AI pick", the
pool deck that is not mine (`opponent::free_brain_deck`). `stop` (the lobby
and the chat tab), `HostClosed`, `HostLost` and `leave_session` all call
`local::end`, which raises the stop flag, drops the channel and queues a
`PeerLeft` so the host marks the seat gone at once. The thread notices
between ticks (the link reports `Dropped`), and a decision in flight is cut
short rather than run out: the same flag is the driver's halt, so `exec`
answers `DECISION_OVER` without running the command, `settle` stops
polling, and `Brain::decide_until` refuses the next model call — the one
HTTP request already on the wire is the only thing that still has to
return. A `bot:` reply therefore cannot land in a successor seat's
`chat.log`. The loop runs under `catch_unwind`, so a panic on the seat
thread becomes the exit reason (`the seat thread panicked: …`) instead of
a bare "the seat thread stopped".

**A new game and a reclaimed seat.** The host's new game (the winner
banner, the table menu) appends `LogAction::Reset`, which the desktop client
answers through `DealGeneration` and `redeal_after_new_game`; the seat's
replica does the same in `handle_event` — the reset clears `Seat.dealt`,
raises `Seat.new_game`, and the next `tick` re-arms the auto deal, forgets
the LLM's game memory and the last decision key, and deals the held deck
again. When the AI is stopped and added again mid-table the host reseats
`local:ai` into its old seat (`join_as`) with the deck still there, so
`deal` checks `Seat.deck_on_table` first and resumes with that deck instead
of asking for a second one (the lobby and the chat tab then say *bot resumed
its seat with the deck already on the table* rather than naming the deck
that was picked). The model's failures are not silent either: the brain's
log lines (`ai thinks`, `ai says`, `ai: nanogpt 401 …`) go to `seat.log`,
and the last `ai: …` error of a decision sits in `Seat.fault` →
`Status.fault` until a decision succeeds, which the chat tab and the lobby
show as *the model is failing — …*. `status()` reads the thread's
liveness, the mind kind, the stop reason, the fault and the resume for the
lobby and the chat tab; a chat line sent at a random seat, or a model
switch while one is live, is answered with words (`FREE_BRAIN_DEAF`,
`switch_live_model`) rather than a `you:`/`model:` line nobody reads.

**Tests** (`ai::local::tests`, headless): a random-brain seat joins a hosted
table built the way the desktop hosts one (`session_engine` plus the wasm
riftbound plugin, the enforced genesis options), the host's seat 0 plays
random moves through `own_intent`, and the game runs to a winner inside the
turn cap (three seeds tried, as random play does not always finish); the
seat stops when the table closes, with `PeerLeft` heard by the host and a
network conn still falling through `deliver`; the seat deals its deck again
after the host's reset (`HostState::new_game`, two `DealDeck` frames, the
deck back on the table); a seat started again after a stop reclaims its
place and resumes with the deck already there, without a second deal; the
chat file round-trips (`ai::seat::tests`: `you:` lines reach
`Chat.messages`, `model:` lines switch the brain, `bot:` replies land in the
file, and a `you:` line written while the bot was deciding is still fresh
after its reply); `RollSecrets` repeats a commitment until it is revealed.

## Open

- **U12** — the second release: portrait unlocked in the Android manifest,
  the share sheet for the ticket, the dark-mode hook, the half-resolution
  render target, the emote row and concede in the table menu (ux.md §10).
- **The `tables` listing bug** — a hosted table is not always listed for a
  meshed peer; `join_by_ticket` is the workaround.
- **Sequencer election** — the host is the only sequencer; a lost host is a
  rehost, never a takeover.
- **Two rows, two facings.** Seats sit in two straight rows, so yaw is only
  ever 0 or π; a round table with per-seat angles would reuse the same spin
  everywhere the yaw already flows.
- **Device runs.** The Android pieces of U7 (insets, IME scroll, keep-awake)
  are verified only on the harness until a phone run before U12.

## Web demo

The table runs in the browser (Bevy wasm, WebGL2): tailnet at
`https://agni.rae.blue`, public at `https://omashu.dragon-pierce.ts.net:8443` via
Tailscale Funnel. Build and deploy:

```
web-build                                            # wasm + bindgen + wasm-opt + assets
rsync -az --delete web/dist/ root@omashu:/var/lib/agni-demo/
```

Serving is `infra/modules/services/agni-demo.nix` — a static nginx vhost,
`default_server` so the funnel's ts.net Host header lands on it. The funnel
proxies the TLS listener (`tailscale funnel --https=8443 --bg https+insecure://127.0.0.1:443`,
run once on omashu; plain :80 only answers with the forceSSL redirect, which is
useless publicly). Funnel cannot carry custom domains — public on
`agni.rae.blue` itself would need a Cloudflare Tunnel.

Native/web split: native deals from the spirit store (bytes, hash-verified);
web deals from the gateway bridge's live manifest, fetching each dealt card's
art blob lazily by hash. No card art is committed or bundled — a page that
reaches no gateway deals tinted placeholders. The wasm window sets
`fit_canvas_to_parent`, so the canvas fills the page and tracks resizes. wasm
has no clock (`web-time` shims it) and no filesystem (tuning does not persist).

**Funnel port trap (2026-08-28):** `tailscale serve/funnel --https=443`
intercepts ALL of :443 on the node's tailnet interface, not just ts.net
traffic — every nginx vhost on omashu (uvs, mail, agni) answered TLS alert 80
until the funnel moved to `--https=8443`. Funnel may only use 443, 8443, or
10000; never give it 443 on a host that serves tailnet vhosts.
