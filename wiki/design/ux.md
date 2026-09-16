# kai — The UX Overhaul

> Status: designed 2026-09-11, nothing built. This page is the synthesis of
> the three competing designs from the UX phase (mobile-first, information
> architecture first, continuity-first) after three judges scored them from
> three chairs: a Hearthstone player on a 360×800 phone, a competitive
> desktop player, and the engineer who has to split the work across agents
> and prove it headlessly. The mobile-first design won the phone, the
> IA-first design won the other two chairs; this document starts from the
> IA-first skeleton, takes the phone layout, the sheet system and the
> gesture grammar from mobile-first, and takes the harness, the status-line
> classifier, the prompt tray and the tools gate from continuity-first.
> Every contradiction between the three is resolved in §11 with the reason.

Citations: `[HS-n]`, `[MTGA-n]` and `[MOB-n]` are the three research sets
the UX phase produced (Hearthstone, MTG Arena, mobile card games).
`file:line` cites the phase-1 inventory of kai's `src/` unless another crate
is named; `INDEX defect n` cites the screenshot index taken at 1280×800,
1024×768, 800×360 and 360×800. The rules engine this UI serves is described
in [agni's rules-engine.md](../../../agni/wiki/design/rules-engine.md);
its two-sentence player contract — *drag a card or press a button; if the
table refuses it tells you why* — is the sentence this whole design exists to
make true.

## 0. What is wrong today, in five lines

1. The refusal reason is written only to `SessionInfo.status`
   (`src/net/mod.rs:332-345`, `904-925`) and read only by the lobby and the
   multiplayer tab (`:1639`, `:1751-1752`). At the table a refused drag
   snaps back in silence. Every free-table tool stays armed under rules
   enforced and produces those silent refusals.
2. Eleven egui areas each choose their own anchor: score window and AI
   window both at RIGHT_BOTTOM (`src/table/counters.rs:157`,
   `src/ai/window.rs:26`); chain HUD and seat buttons at RIGHT_TOP
   (`src/table/ui.rs:184`, `:410`); tokens over the prompt strip at
   CENTER_TOP (`src/table/tokens.rs:46`, `src/table/plugin_ui.rs:463`); hand
   slider and sideboard toggle over the fan at CENTER_BOTTOM
   (`src/table/ui.rs:383`, `src/deck/sideboard.rs:135`); chooser and winner
   both centred (`src/deck/battlefield.rs:135`, `src/table/winner.rs:34`).
3. The same control lives in two or three places: deck import in the lobby,
   Settings › decks and a clipboard button; host/join in the lobby and
   Settings › multiplayer; playmat and seat colour in the lobby and Settings ›
   table; the battlefield chooser opens from four sites.
4. A single unwrapped label pushes a whole window off the screen (INDEX
   defect 1): with rules enforced ticked the lobby is blank at 360×800 and
   half gone at 1024×768.
5. Touch has nothing: hover previews, hidden-card offers, counters and every
   hovered hotkey die on a finger; any tap jitter becomes a drag; the
   Android back key is dead; the table renders 42 pt cards at 800×360 and a
   3.6-unit sliver at 360×800.

## 1. The principles kai adopts

Each principle names the research it comes from and what it changes here.

- **The primary button is a state machine the player reads without thinking**
  — amber when plays remain, green when it is the only thing left, grey when
  it is not your move — and its label says what pressing it does right now
  [HS-3, MTGA-5]. It is the only place the game moves on from. It is never
  auto-pressed on green (a hand of zero-cost cards would leak) and it is
  never adjacent to a second turn-ending button [MTGA-25].
- **Whose move is the loudest thing on screen** [MTGA-13]: a turn plate with
  the seat's colour that pulses while it is you, and a "waiting for …" line
  with a ticking spinner and a connectivity dot when it is not [MTGA-15].
- **Auto-pass by default, stops and hold as the only reasons to wait**
  [MTGA-1, MTGA-2, MTGA-3, MTGA-4], with a constant delay so timing leaks
  nothing [MTGA-8]. The client never asks about what the player cannot
  influence and never asks twice [MTGA-6].
- **Refusals are shown where the player is looking** and in the engine's own
  words. The rules-engine contract is met at the last metre, not in a
  settings tab.
- **Free-table tools are disabled, not refused, under rules enforced.** A
  control is drawn only if the plugin could accept it (the spec's "the strip
  is never a lie"). What remains refusable is what only the engine can
  judge.
- **One meaning per gesture, the same on mouse and touch** [MOB-5, MOB-6,
  MOB-12]: tap selects, a second tap or double-click does the one default
  action, long-press inspects, drag moves; a real distance threshold
  separates tap from drag. Nothing important is hover-only; hover is an
  accelerator [MTGA-17].
- **Drag is the primary verb, the drop target is generous, and releasing on
  nothing is the undo** [HS-7, MTGA-10]. Legal targets are lit by the engine
  before the commit, never guessed by the client [MTGA-9].
- **Mandatory choices are answered on the objects themselves with one
  confirm and a visible count** [HS-13]; candidates with no face on the felt
  get faces in a tray the player can tuck away to look at the board
  [HS-12].
- **The stack is always visible, newest on top, hoverable, with its target
  drawn as an arrow** [MTGA-7], summarised to one line on a phone
  [MTGA-22].
- **Hidden information is shown as objects you can count** [HS-5, HS-20]:
  the opponent's hand as backs, deck and trash counts on their seat plate,
  never a hover away on a phone.
- **A history of what just happened without a log** [HS-10], plus a
  persistent log phrased as sentences with hoverable names [MTGA-16].
- **On phones the hand is a drawer, the primary button is thumb-sized, the
  opponent's side is a summary strip, and every tappable thing is 48 dp**
  [HS-16, MOB-1, MOB-7, MOB-8, MOB-16, MTGA-21, MTGA-22]. Phone and tablet
  get different layouts chosen by physical size [HS-17], read through egui's
  zoom factor, not the scale factor override [MOB-10].
- **Back navigates one level and never dismisses the table by surprise**
  [MOB-3]. The safe area is read and every HUD anchor pads by it [MOB-2].
- **No modal dialogs; the client is one physical object** [HS-21]. Windows
  become sheets over the felt; the winner is a banner across it.
- **Colour is never the only signal** [MTGA-24]: every rim pairs a hue with a
  stroke pattern; a colour-blind palette exists; every action has a keyboard
  path.
- **Animation explains transitions, the static display explains state, and
  feedback never costs the player their turn** [HS-11, HS-18]: every
  animation is interruptible and there is a fast-animations setting.
- **Hide what the rules do not require and be honest about the gaps**
  [HS-20]: a deck that is only partly scripted says so before the deal.

## 2. Information architecture

### 2.1 Four screens, sheets over them, one back ladder

Four screens survive: **Home**, **Lobby**, **Table**, **Settings**, plus
(since 0.15) the two **deck editor** screens, **Decks** (the library) and
**DeckEditor**, which are full screens reached from Home, the lobby's deck
card or the deck box and which return to whatever opened them. Settings is a
sheet over whatever is beneath it, never a screen that hides the table (today
`menu_ui` returns while `settings.open`, `src/menu.rs:157`, so opening
settings at the table blanks the game). Everything else — the deck box, the
battlefield step, the opponent panel, the table menu, the card sheet, the
drawer, the winner banner — is a sheet or a panel inside one of the screens.

```
Home ──▶ Lobby(game) ──▶ Table
  │          ├ deck box (sheet: pick a saved deck · sideboard · "open the deck editor")
  │          ├ opponent panel (sheet, from an empty seat)
  │          └ join sheet
  │                          ├ table menu (sheet: resume · concede · free table · leave · settings · help)
  │                          ├ card sheet (inspect + chips; phones)
  │                          ├ drawer (log · chat · tokens)
  │                          └ winner banner
  ├ Decks (screen: draft in progress · your decks) ──▶ DeckEditor (screen: … · import from the clipboard · share)
  │      ↑ from Home, the lobby deck card or the deck box; back returns there
  └ Settings (sheet: play · look · you · advanced), reachable from every screen
```

A sheet is drawn by one helper, `sheet(ctx, id, class, Side, |ui| …)`: on
phones a full-height panel or a 60 % bottom sheet with a scrim, no drag, no
resize, no collapse, one vertical scroll area and an × top-right [MOB-11];
on tablet and desktop a 380–480 pt panel on the chosen side. At most one
sheet and the drawer are open at once; opening a second closes the first.
No sheet uses `Order::Foreground` to fight another — the battlefield chooser
drawn under the lobby window (INDEX defect 3) is what that rule prevents.

The **back ladder** is one system, `back_ladder`, that Esc, the Android back
gesture (read from the logical `Key::BrowserBack`; the physical
`KeyCode::BrowserBack` polled at `src/menu.rs:135` and `src/settings.rs:111`
never fires on Android) and browser back all walk [MOB-3]:

1. a sheet or the drawer is open → close it; in the deck editor, an open
   card detail, then the cards pane, then the editor (to the library), then
   the library (to the screen that opened it);
2. a drag or a targeting prompt is in progress → cancel it (the prompt's
   cancel/skip when offered, else the drag eases back);
3. a card is selected → deselect;
4. at the table with nothing open → open the table menu (never leave; leave
   is a menu button with a confirm);
5. in the lobby → Home; Home → the system may minimise.

Esc therefore no longer means "settings" at the table and "back" in the
lobby (`src/settings.rs:111-129`, `src/menu.rs:132-143`).

### 2.2 Home (replaces the Games screen, `src/menu.rs:191-245`)

Centred, max content width 720 pt, one column on phones:

- If a session is live: one full-width primary card "Return to your table ·
  Riftbound · turn 7 · your action" with a seat-coloured rule. It is first
  because a live game is the most likely reason to be here; today it sits
  under the debug text.
- Three game rows (phones) or three 220×140 pt tiles (tablet, desktop) in the
  order **Riftbound, MTG, free-form** — the enforced engine first, not
  `TableGame::ALL` order (`src/net/mod.rs:115`). Each has a fixed size; the
  label and tagline are laid out with `ui.set_max_width` and `Label::wrap`
  inside the tile so the button never grows to the text (INDEX defect 6,
  `src/menu.rs:208`).
- A footer row with a gear (Settings) at the right. Nothing else: the module
  store lines with hashes, the "serving N blobs · peers" line
  (`src/menu.rs:68-87`) and the version/wire badge (`src/settings.rs:166-180`)
  move to Settings › advanced. The clipboard quick-import button
  (`src/deck/import.rs:808-828`) is removed from every screen and becomes the
  deck box's paste button.

No hero tile with legend art: a first screen is navigation, not marketing.

### 2.3 Lobby — three decisions and one verb

The lobby is where a table is set up; nothing about setup remains on the
table. It is one scroll area, one column on phones, two on tablet and
desktop (deck and rules left, opponent right). Every label wraps (§5.5).
Header: a back button whose glyph the bundled font has ("‹", not U+2190
which renders as tofu, `src/menu.rs:263`, INDEX defect 4), the game name,
nothing else — no plugin-store line (`src/menu.rs:262-283`).

The three decisions, in the order a player makes them:

**Deck** — one card: legend art thumbnail (`src/deck/thumbs.rs`), the deck
label, a line "battlefield: Rockfall Path · change", and under rules enforced
a coverage chip: "scripted" for a pool deck, "N of 40 scripted — the rest
play as vanilla" for an imported one. Coverage is computed from the pool md
files kai already parses (`src/ai/cards.rs` `from_pool`) against the deck's
names. Partly scripted decks are allowed at an enforced table; the chip is
the honesty, not a lock. The card carries exactly two links: "switch decks"
opens the deck box (§2.4) and "deck editor" opens the deck screens; editing,
renaming and sharing live in the editor, not on the card. With no deck chosen
the card reads "switch decks" and is itself the button.

**Your name** — a single-line field under the opponent segments, capped at
24 characters, with the platform default ("browser" on the web, the login
name on the desktop) as its hint. It is saved as it is typed (`name.txt` in
the config dir, localStorage on the web) and is the name the roster shows
for the next table hosted or joined; the same field sits in Settings › you.

**Rules** — a segmented control **rules enforced · free table**, default
*enforced* for Riftbound (`TableChoice::default` has `enforced: false`
today, `src/menu.rs:511-518`). One short line beneath: "the table refuses
illegal plays and runs the turn" / "anything goes; you move the cards". Then
the mode presets as one chip row (1v1 Duel · 1v1 Match · FFA3 · FFA4 · 2v2 ·
house rules…) with the two steppers (first to N · N battlefields) under the
last chip. The multi-clause mode note (`src/menu.rs:484-491`) becomes one
sentence: "first to 8 · 2 battlefields · first player by roll". MTG and
free-form lobbies show the control locked to free.

**Opponent** — a segmented control **AI · friends · join**:

- *AI*: the AI's deck card (same shape as mine, "let the AI pick" by
  default), a "fast · thinking" control mapping to the two shipped model
  presets (`src/ai/window.rs:5-128` lists them), and nothing else. The model
  string, vendor presets and log path (`src/menu.rs:332-450`) go to Settings ›
  advanced. The AI's battlefield is the first of its deck, not wall-clock
  nanoseconds (`src/menu.rs:422-427`), shown as "battlefield: … · change".
- *friends*: the seat roster as rows — swatch, name, deck tile or "no
  deck", "(you)" / "(host)" / "(gone)" — never plain text
  (`src/net/mod.rs:1607-1667`). An empty row offers **invite** (copies the
  ticket, shows the QR inline; on Android the share sheet) and **add AI**.
  Recovery ("reconnect", "re-host") is one amber card at the top of this
  group only when applicable.
- *join*: the open tables in the mesh as 64 dp rows "{host} · Riftbound ·
  1/2 seats" with a join button, and the empty state "no tables yet — ask a
  friend to host, or paste their ticket" with a paste field. The empty state
  matters because the `tables` listing bug (§9) makes the list empty more
  often than it should be; direct join by ticket works regardless.

The **primary button** is pinned to the bottom (a sticky 72 dp footer on
phones, right-aligned in the header on desktop), 56 dp tall, and its label
is the verb for the current state: **play vs AI** · **host table** · **join
{name}** · **go to the table** (a seated client or a host whose seats are
dealt). Disabled with its reason inline: "choose a deck first", "choose a
battlefield first". "open the table without a session" (`src/menu.rs:262-283`)
leaves; the tuning workflow it served is Settings › advanced › "open a tuning
table". The lobby never renders `info.status` as a log line; session events
become toasts (§3.7) and the seat rows carry state.

Playmat and seat colour are not in the lobby. They are a look preference,
they were duplicated from Settings › table (`src/menu.rs:452-593`,
`src/settings.rs:259-269`), and a player wants to change them at the table
too — so they live once, in Settings › look, reachable from everywhere.

### 2.4 The deck box — the one deck surface

A sheet opened from the deck card (mine or the AI's — the seat is implied by
which card opened it, so "seat this deck (player N)" wording goes), from the
table menu between games, and from the winner banner. Five steps top to
bottom, one scroll area:

1. **Your decks** — tiles (2 columns at 360 dp, a row on desktop). Under
   rules enforced the scripted pool decks come first with a "scripted" chip:
   Lillia - Bashful Bloom, Irelia - Blade Dancer, and the four new pool decks
   once their pool files land. Then "decks you have played" from history
   (`src/deck/history.rs:429-479`), each with a … menu (rename · sideboard ·
   forget, forget asking once). This replaces the two-label `Side` selector
   and its paragraph (`src/deck/pinned.rs:249-309`, the single label that
   triggers INDEX defect 1) and makes `pinned::Side` an open list of pool
   decks keyed by legend name rather than a two-variant enum matched by
   label prefix (`src/deck/pinned.rs:9-49`). Tapping a tile seats it;
   seating is the only side effect, exactly as `pin_decks` does today.
2. **Battlefield** — the chooser (`src/deck/battlefield.rs:97-202`) as a step
   inside the sheet: the deck's battlefields as 160×112 dp landscape tiles,
   single choice, the placement note as one line. It is never a spontaneous
   window: the four openers (`battlefield.rs:52-85`, `src/menu.rs:463-465`,
   `pinned.rs:297-299`, `import.rs:1110-1112`) collapse to "the deck card is
   incomplete until a battlefield is chosen" and the lobby's primary button
   says so. After "play again" the choice arrives as a prompt-tray question
   at the table (§3.8), not a second window over the winner.
3. **Sideboard** (Riftbound, between games) — the two-list swap from
   `src/deck/sideboard.rs` inside the sheet, rows 56 dp with a 40 dp
   thumbnail. The bottom-centre toggle leaves every screen
   (`sideboard.rs:134-145`). "reload deck" is offered only while no game is
   in progress and reads "use this list next game".
4. **Import** — one multiline box with the placeholder "paste a deck list, a
   deck code, or a riftdecks / piltover link", a **paste** button (the one
   clipboard entry point, `src/os/clipboard.rs`) and **import**. `parse_any`
   already classifies text and `link::classify` recognises a URL on the
   first line, so the second URL box and its buttons go. Progress and errors
   are one status row under the box ("resolving 40 cards…", "3 cards not
   found: …"), not `ImportPanel.note` overwriting itself. "download full
   riftbound set" moves to Settings › advanced.
5. **Edit** — the deck editor sheet over the box: the **new deck** tile in
   the first row of *your decks*, `edit a copy` under a pool tile, `edit` in
   a history tile's menu, *edit the whole deck* under the sideboard and
   `edit` on an import result all open it; it builds a deck from the
   catalog, checks it against rules 103.1–103.4 and shares it as a text
   list, code or link — [deck-editor.md](deck-editor.md). Closing it
   returns to the box it came from.

### 2.5 The AI opponent

Lives in exactly two places: the lobby's opponent › AI segment (deck and
speed) and the drawer's **chat** tab at the table (§3.11), where the "you:" /
"bot:" lines, "switch model" (the presets) and "stop AI" live. The B hotkey
becomes C and opens the chat tab at the table only (today the window overlaps
the lobby, `src/ai/window.rs:13-16`). The AI's seat is a normal seat
everywhere else: a plate, a colour, a name.

### 2.6 The table menu

Esc, the back gesture or the ≡ button top-left open a sheet with exactly:
**resume** · **concede** (second press confirms [MTGA-19]; needs the hidden
concede affordance from the presenter, until then "leave table" is the only
way out) · **propose a free table** / **confirm free table** (the presenter's
offer at `present.rs:444-448`, moved here so it never sits beside end turn)
· **change deck** (between games) · **leave table** (with "the table stays
open for the others" / "this closes the table" as host) · **settings** ·
**help**. The winner banner (§3.10) offers "play again" and "leave".

### 2.7 Settings — a sheet with four tabs

A right-side sheet 380 pt wide on desktop and tablet, full screen on phones,
with a close × (there is none today, INDEX defect 5) and no "save" (which
only closes, `src/settings.rs:305-366`; the sheet saves as you go and says
so once in the footer). It never hides the table. The identity QR block
leaves the header (380 px on every tab, `src/settings.rs:12`). Tabs:

- **play** — auto-pass when I have no response (on); ask me anyway for
  forced choices (off); order my triggers myself (off); assign combat damage
  myself (off); confirm "end turn" while I still have plays (off); fast
  animations (off); hand on the left (off); UI scale (0.8–1.5); show hints
  again.
- **look** — playmat grid (wrapping fixed-size tiles), seat colour swatches,
  camera preset (arena · top-down), zoom, camera lock, hover preview size,
  foil on/off, theme (system · dark · light), colour-blind palette. These
  are the player-facing entries among the nineteen sliders
  (`src/table/ui.rs:62-110`).
- **you** — the QR, "you are {short id}", copy ticket, copy node id, a
  display name field (the roster name), add a peer (paste, scan on Android),
  and the peers list (`src/net/peers.rs:180-240`) under a "details"
  disclosure with wire bytes and refs.
- **advanced** — behind a "show developer settings" toggle persisted with
  tuning: the hand/foil/easing sliders (± steppers on phones so they do not
  fight drag-to-scroll), "deal a sample hand", "open a tuning table",
  modules (`src/engine/modules.rs:1195-1235`), telemetry and the log viewer
  (`src/telemetry.rs:597-668`), "download full riftbound set", the AI model
  string, presets and log path, and the footer "v0.9.1 · wire 5 ·
  modules/riftbound v0.2.1".

The multiplayer and decks tabs are gone (host/join only in the lobby, decks
only in the deck box); the game radio buttons go with them (the lobby's game
is the game). F3 no longer exists. The default tab is play.

### 2.8 Naming

A seat is shown as its roster name with its colour swatch; the colour word
appears only when the roster has no name. A pure
`seat_label(roster, colors, me, seat) -> (String, [u8; 3], is_me)` in
`src/table/colors.rs` feeds the strip's `{seat N}` expansion
(`src/table/plugin_ui.rs:128-162`), the seat plates, the winner banner and
the lobby rows, ending the two naming schemes the inventory found. The
fallback name is "Player 2", not "red". The mode is written "rules enforced"
and "free table" everywhere, the presenter's own words; no third word.

## 3. The table HUD

### 3.1 One layout function, one owner per region

`src/table/hud.rs` owns

```
pub fn layout(class: ViewportClass, screen: Rect, insets: Insets, drawer: DrawerState, chain_len: usize) -> HudRects
```

returning a named rect per slot. Every egui area under `src/table` draws
into its slot with `Area::new(id).fixed_pos(rect.min)` and
`ui.set_max_size(rect.size())`; nothing may call `.anchor()`. The arrows
module reads chain-row anchors from the chain panel's live rects instead of
assuming a 104 pt row and a 30 pt header (`src/table/arrows.rs:188-212`), and
the seat plates no longer compute their offset from the preview height
(`src/table/ui.rs:408-413`).

Slots are of two kinds. **Placed** slots may not overlap each other or the
safe-rect edge. **Floating** slots (`banner` on landscape phones,
`card_chips`, `toast`, `drop_chooser`) are anchored to something live — the
stage's top band, a card's screen rect, the drop point — clamped inside the
stage, and may overlap the stage but not each other or any placed slot. The
`hand` band on desktop and tablet is reserved, not drawn: the layout subtracts
it so nothing can cover the fan. The unit test runs at 1280×800, 1024×768,
800×360 and 360×800 for chain lengths 0, 1 and 5, drawer tucked and raised,
and asserts: no two placed slots intersect; every slot is inside the safe
rect; every touch slot is ≥ 48 dp on its short side; the stage is ≥ 50 % of
the safe height on phones.

### 3.2 Desktop (width ≥ 1100, height ≥ 480; reference 1280×800)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ≡  ┌turn plate──┐    ┌──────────── strip ────────────┐      ┌ chain · 2 ──────┐│
│    │ turn 7     │    │ Vi: choose a unit to stun      │      │ ▣ Defy   → Poro ││
│    │ rae · action│   │ 1 of 1   [your base] [cancel ×]│      │ ▣ Back Off      ││
│    └────────────┘    └───────────────────────────────┘      └─────────────────┘│
│    ◐ ● ● ◉ ● ● ● ● ●   (phase bar, pointer classes)          ┌ seats ──────────┐│
│                       (toast: not enough runes: 3 needed…)  │ ● claude  ○○●●●●││
│ ┌history┐                                                    │   xp 0 · hand 5 ││
│ │ ▣ ▣ ▣ │                                                    │   deck 31       ││
│ │ ▣ ▣ ▣ │                        (felt)                      │ ● rae ●●●○○○○○  ││
│ └───────┘                                                    │   xp 2   (you)  ││
│ ┌inspector─┐                                                 └─────────────────┘│
│ │          │                                                 ┌ end turn ───────┐│
│ │  card    │                                                 └─────────────────┘│
│ │  caption │             ╭──────── hand fan ────────╮        ┌ PASS        ⎵ ──┐│
│ └──────────┘             ╰──────────────────────────╯        └─────────────────┘│
│                                                              [log] [chat] [tok] │
└──────────────────────────────────────────────────────────────────────────────┘
```

| slot | position (pt, from the inset edge, 12 pt gutters) | size |
|---|---|---|
| `menu_button` | left-top | 40×40 |
| `turn_plate` | right of the menu button | 150×48 |
| `phase_bar` | under the turn plate (pointer classes only) | 150×20 |
| `strip` | centre-top, y 12 | w = min(560, W − 2·(220 + 24)), h ≤ 3 rows |
| `toast` | under the strip, 8 pt gap | strip width × 32 |
| `chain` | right-top | 220 × (3 full rows + "and N more") |
| `seats` | under the chain | 220 × 64 per seat |
| `history` | left, under the phase bar | 40 × up to 8 tiles of 56 |
| `inspector` | left, between history and the hand band | w = min(198·s, 0.18·W), h ≤ 0.42·H |
| `secondary` | above the primary, 8 pt gap | 168×36 |
| `primary` | right-bottom, above the drawer tabs | 168×56 |
| `drawer_tabs` | under the primary | 3 × 40×28 |
| `drawer` | right side, over the seats when open | 300 × H |
| `hand` | centre-bottom, reserved | (W − 2·192) × 0.28·H |
| `banner` | centre of the stage | winner, mulligan, prompt tray |

### 3.3 Tablet (width 600–1099 with height ≥ 480; reference 1024×768)

The desktop layout with the same slots and 12 pt gutters, the inspector
capped at 0.16·W, the seats column 200 wide, the history rail four tiles,
and the drawer sliding over the seats. A tablet keeps the hand fan (always
raised); under touch the fan is driven by selection instead of hover, which
needs no drawer. A tablet with a mouse is a desktop for hover purposes; the
input kind is a separate axis from the class.

### 3.4 Phone portrait (width < 600, height ≥ width; reference 360×800)

```
┌──────────────────────────────────────┐  y
│ ≡   Your action · turn 3      R2 Y0  │  0–56    top_bar
│ ▣▣  ▮▮▮▮▮ (hand 5)  deck 24 · ⬢ 4/6  │  56–120  opp_strip
│ ┌──────────────────────────────────┐ │
│ │ Choose a target for Cleave       │ │  120–216 banner (0 when idle,
│ │ 1 of up to 2  [your base] [× ]   │ │          collapsible to 32)
│ └──────────────────────────────────┘ │
│ › claude played Back Off             │  216–240 ticker
│ [▣1][▣2]  +1                          │  240–296 chain_rail (when non-empty)
│                                      │
│         ┌────┐  ┌────┐               │  296–720 stage
│         │ BF1│  │ BF2│               │          (≥ 400 dp always)
│         └────┘  └────┘               │
│   ┌──┐┌──┐┌──┐┌──┐                   │
│   │  ││  ││  ││  │  (my base)        │
│   └──┘└──┘└──┘└──┘                   │
│ ⬢ 3/5 ⚡2                 ┌ END TURN ┐│  656–712 bottom_left · primary
│                           └──────────┘│
│ ╭──╮╭──╮╭──╮╭──╮╭──╮                 │  728–800 drawer, tucked (72)
└──────────────────────────────────────┘          raised: 612–800 (188)
```

| slot | y (dp, insets 0) | holds |
|---|---|---|
| `top_bar` | 0–56 | ≡ (48), turn plate (two lines), score plate |
| `opp_strip` | 56–120 | legend/champion tiles 32×45, the far hand as countable backs, deck · runes · trash chips |
| `banner` | 120–216 max | prompt / response window / waiting / opening / refusal; chevron collapses to 32 |
| `ticker` | 216–240 | newest narration line for 4 s, else the idle hint |
| `chain_rail` | 240–296 when non-empty | 40×56 thumbnails newest-left, "+N" beyond four; tap expands a sheet |
| `stage` | to the primary row | the 3D table; drives the camera framing |
| `bottom_left` | 48 tall, above the drawer | rune/energy chips ⬢ 3/5 ⚡ 2 |
| `secondary` | 40 tall, 8 above the primary | end turn when it must be separate; cancel |
| `primary` | 56 × ≥ 120, right, above the drawer | the primary button |
| `drawer` | 72 tucked / 188 raised | the hand (§5.4) |
| `card_chips` / `toast` / `drop_chooser` | floating | §4.3, §3.7, §4.4 |

The primary, secondary and bottom_left slots ride on the drawer's top edge
with an 8 dp gap, so they rise with it. There is no inspector slot; the card
sheet (long-press) replaces it. Handedness (Settings › play) mirrors
`bottom_left` and `primary`. With the banner at its maximum and a chain
showing, the stage is 424 dp; with nothing pending it is 570.

### 3.5 Phone landscape (height < 480; reference 800×360)

```
┌────────────────────────────────────────────────────────────────────────┐ y
│ ≡  Your action · turn 3 · action        hand 5 · deck 24 · ⬢4/6   R2 Y0│ 0–44  top_bar
│           ┌───────── banner (floating, ≤ 56) ─────────┐                │ 48–104
│           │ Choose a target · 1 of 1  [your base] [×] │                │
│           └───────────────────────────────────────────┘                │
│           [▣1][▣2] +1   (chain ribbon, floating, 32)                    │ 108–140
│                                                                        │
│                 ┌────┐   ┌────┐          (stage 44–304, 260 dp)         │
│                 │ BF1│   │ BF2│                                        │
│                 └────┘   └────┘                                        │
│      ┌──┐┌──┐┌──┐┌──┐                                                  │
│ ⬢3/5 ⚡2                                              ┌ END TURN  ⎵ ┐  │ 248–296
│                                                       └─────────────┘  │
│ ╭──╮╭──╮╭──╮╭──╮╭──╮╭──╮╭──╮                                           │ 304–360 drawer (56)
└────────────────────────────────────────────────────────────────────────┘        raised: 192–360 (168)
```

| slot | rect (dp) | note |
|---|---|---|
| `top_bar` | full width, 0–44 | ≡ 44, turn plate one line, the far seat's counts as chips (there is no opp_strip: the far outer band is cropped and its counts live here), score plate |
| `banner` | floating, centred, w = min(480, W − 2·176), 48–104 | ≤ 56 dp, two rows max, collapsible to 28 |
| `chain_rail` | floating, under the banner, 108–140 | one-line ribbon when non-empty |
| `stage` | 44–304 | the far seat's inner band lies under the floating banner; the banner is collapsible for exactly that reason |
| `bottom_left` | 8–168 × 248–296 | rune/energy chips |
| `primary` | 672–792 × 248–296 | 48×120 in the thumb zone |
| `secondary` | above the primary, 8 gap | 40×96 |
| `drawer` | 304–360 tucked / 192–360 raised | the hand |

The previous designs promised this row and did not write it; this is the
row `hud::layout` implements and the non-overlap test runs.

### 3.6 Turn plate, phase bar and seat plates

The **turn plate** answers "whose move" in words, loudest of everything
[MTGA-13]. Line 1 (18 pt desktop, 20 pt phone) in the acting seat's colour:
"your action" / "waiting for claude" / "respond or pass" / "showdown at
Battlefield 2" / "setup · mulligans". Line 2 (13 pt): "turn 7 · action
phase" and a mode chip (**enforced** neutral, **free** amber). While
`acting(view)` is true (`src/table/highlight.rs:113`) the plate border and
the primary button pulse in my seat colour on a 2 s ease.

Its source until the structured view fields land (§10 U11) is
`classify_status(line) -> StatusLine`, a pure function over the prefixes the
presenter writes today — "turn N · {seat} · {phase} phase · rules enforced"
(`present.rs:342-349`), "points ·", "chain:", "held by", "contested by",
"waiting for", "roll for first player", "combat at" — with a test table that
quotes those strings verbatim, so a presenter wording change fails a kai
test rather than the player. `PluginView.status` stays `Vec<String>`; no
wire change.

The **phase bar** — nine chips for `Phase::ALL`, the current one filled,
click toggles a stop on that phase in two colours (my turn / their turn)
[MTGA-3] — appears only on pointer classes (desktop and tablet with a
mouse) and only once the view carries the phase list (U11). Sub-48 dp chips
that silently change auto-pass are a mis-tap trap on a phone, so phones set
stops in Settings › play as a per-phase list. Stops are a client-side
`Stops: BTreeSet<(Phase, MyTurn)>` persisted with tuning.

**Seat plates** (desktop: right edge under the chain, mine last; phones: the
score plate in the top bar, tapping it opens a popover with the same rows):
a swatch and the roster name; points as pips filled to `points` of
`victory_score` with the numeral as annotation [HS-1]; "xp 2" as a chip once
XP reaches the blob; and for the *other* seats "hand 5 · deck 31" in grey —
the counts of what I cannot see, on screen because a phone has no hover
[HS-5, MOB-5]. A connectivity dot in the plate header replaces the wasm
bridge line (`src/net/gateway.rs:195-222`). The ± of the score window
(`src/table/counters.rs:96-142`) render only on a free table. In FFA and free
tables a plate is clickable to look from that seat, replacing the seat
buttons (`src/table/ui.rs:399-432`); the viewed seat's plate carries an eye
glyph so the spectator camera stops being invisible state.

### 3.7 The strip on desktop, the banner on phones — one element

The prompt strip (`src/table/plugin_ui.rs:421-580`) is today a paragraph of
up to twenty lines that grows mid-game and pushes its buttons. It becomes a
fixed element with five exclusive states, each at most three rows (56–96 dp
on phones), sitting directly above the cards being asked about:

1. **Prompt** (`view.prompt` is mine): the question with the source card's
   thumbnail at the left when `PromptWhy::item()` names one ("Vi - Piltover
   Enforcer: choose a unit to stun"), the count as a chip ("1 of 1", "2
   picked · up to 3", "optional"; `prompt_line`, `plugin_ui.rs:164-185`
   already builds these words), and one row of chips for options that are
   *not* on the table (seats, zones, "your base", yes/no, "recycle a rune").
   Options that name a card are not chips: the card itself is the button
   [HS-13]. A hollow **cancel** / **skip** / **no** with × printed on it sits
   at the row's left end when the prompt allows it.
2. **Response window** (`view.chain` non-empty and I hold priority): "claude
   played Back Off — respond or pass", the top chain item's thumbnail at the
   left. The primary reads **resolve**.
3. **Waiting**: "waiting for claude · her action phase" with a spinner that
   ticks off seconds; "still thinking…" after 60 s; red **disconnected** when
   `net::peers` liveness says so [MTGA-15]. A hosted table with an empty seat:
   "waiting for a player · share ticket".
4. **Opening**: the roll and the first-player choice ("you won the roll — who
   goes first?" with **go first** / **claude first** chips), then "dealing…".
   Today this is the four-line prompt that covers the gear row
   (`1280x800-table-rolled-default.png`).
5. **Refusal**: the amber row of §3.8 for 2.5 s, then the previous state.

When nothing is pending the strip collapses to zero height. Everything else
the presenter puts in `status` is routed by `classify_status`: the scoreboard
to the seat plates; "held by / contested by" onto the battlefield zone itself
(a 4 dp seat-coloured rule along the zone's near edge for the holder, dashed
for the contester); the combat line into a combat plate over the contested
battlefield (attackers' total might vs defenders' as two seat-coloured
chips); "chain: A → B (top)" dropped (the chain panel shows it); narration
to the ticker, the history rail and the log. The strip never grows and never
pushes a button.

### 3.8 Refusals — shown where the player is looking

One resource, three renderings, no agni change:

```
pub struct Refusal   { text: String, card: Option<u32>, at: f64 }
pub struct LastIntent { card: Option<u32>, sent_at: f64 }
```

`net::session_failed`, `refusal_notice` and the `HostMsg::Notice` handler
(`src/net/mod.rs:332-345`, `904-925`) write `Refusal` beside `info.status`
(the lobby keeps its line). Every site that sends a card intent —
`on_drop_on_zone`, `on_drop_on_surface`, the chain drop, the chip actions —
writes `LastIntent`. A refusal arriving within two seconds of a `LastIntent`
that carried a card is anchored to that card; otherwise it is a strip
refusal. The text is the engine's `Refusal::label` verbatim
(`riftbound-turns/src/lib.rs:61-104`: "not enough runes to pay for that: 3
needed, 1 ready", "units are played to your base, then attack from there",
"answer the open question first") with `{seat N}` expanded through
`seat_label`. Renderings:

1. the strip/banner's refusal state, amber, 2.5 s;
2. a toast bubble 8 pt above the refused card's resting rect (it has snapped
   back, so it is exactly where the player is looking), 1.5 s;
3. a 120 ms shake of the card before it eases back, and a one-shot amber rim
   pulse.

The last five refusals stay readable in the log tab. Session events ("player
2 dealt a deck", "claude asked for a new game", "host ended the session") use
the same toast in the strip position. kai-cli already prints the Notice, so
the AI seat and the human finally see the same sentence.

Refusals should be rare, because under rules enforced the free-table verbs
are **disabled, not refused**. A `Tools { free: bool }` resource set each
frame from `enforced(view)` (`src/table/plugin_ui.rs:10-15`) gates, with one
`if tools.free` at each draw site: single-click exhaust
(`src/table/interaction.rs:334-344`), right-click recycle (`:259-287`), drags
of cards with no legal row (a card without a rim does not lift; a drag starts
only for rimmed cards or on a free table), the D/T/E hotkeys
(`src/table/ui.rs:522-647`), the chain resolve buttons (`ui.rs:203-226`), the
score ±, the counters popup, the tokens tab and K, sideboard reload mid-game
and "deal a sample hand". A fixture test over a `PluginView` whose
`status[0]` ends "rules enforced" checks that every free verb is inert. What
remains refusable is what the engine must judge: a rimmed drag to a wrong
zone, a play the pool cannot pay, a stale prompt press.

### 3.9 Prompts whose candidates have no face on the felt — the tray

Most prompts are answered on the felt. Some candidates have no face there:
the top N of a deck (Stacked Deck), an opponent's revealed hand (Sabotage,
Decree of Strength — the critic's revealed-hand gap), a battlefield to choose
before a deal, PayWith's gold. For these the strip grows a **tray** in the
`banner` rect: card faces at 120×168 (landscape 168×120), each a button with
the option's label, framed in the owning seat's colour for revealed enemy
cards, and a **peek at table** toggle that collapses the tray to the text
chips [HS-12]. The tray is the same option list (`ordered`, group 0) rendered
with art, so the hotkeys and the AI's view are unchanged. The table-side
battlefield choice after "play again" is drawn by `deck/battlefield.rs` into
the tray through a `TrayItems` resource, so the strip stays game-agnostic and
the chooser and the winner never share the centre.

The forms for the prompt kinds the four new decks need, so the strip is
designed once:

| prompt | strip row | candidates | primary |
|---|---|---|---|
| look at top N, pick one | "look at the top 3 · pick one" | tray of 3 faces (mine only; the other seat sees "rae is looking at 3 cards") | — |
| reveal a hand and pick | "claude reveals 5 cards · pick one" | tray of faces framed in claude's colour | — |
| pay or let resolve (Hard Bargain) | "pay 2 energy to stop {card}?" | `yes` / `no` chips, neither primary [MTGA-25] | — |
| each player chooses, in order | "{seat} chooses first · waiting" then mine | as the underlying kind | — |
| optional cost (Burn, Accelerate) | "pay … to …?" | `yes` / `no` | — |
| XP spend confirm | "spend 1 XP to …?" | `yes` / `no` | — |
| extra turn notice | turn plate line 2 reads "extra turn" | — | end turn |
| GroupMove (the March batch, §4.5) | "move others to Battlefield 2 too? · 2 picked" | the other ready units pulse on the felt; an **all** chip on the destination | `done` reads "move 3" |
| mulligan | "set aside up to 2 to redraw · 1 picked" | hand cards pulse, a corner × on the marked ones; the drawer auto-raises | `keep` reads "keep · 2 set aside" |

### 3.10 The chain panel

Desktop and tablet: top-right, 220 pt, header "chain · 2", rows newest first
with a 52×72 thumbnail, the name, the controller's swatch and a target arrow
(`arrows.rs` `Origin::Item`); the top three full size, the rest "and N
more" [MTGA-7]. Hover or tap drives the inspector (`ChainHover`,
`src/table/ui.rs:241`). Phones: the one-line ribbon of §3.4/§3.5, tapped to
expand into a sheet [MTGA-22]. "resolve → base / trash" render only on a
free table.

The panel is the chain's **drop target**. The chain zone has no `DropZone`
because it is `ZonePlace::Offstage` (`games/riftbound/src/lib.rs:157-166`),
which is the first cause of the facedown-play blocker; dropping any card on
the panel's rect emits `CardDropped { to: stack_zone }` — the same bytes the P
hotkey sends (`src/table/ui.rs:586-597`). On phones, while the response
window is open, dropping a React-rimmed card anywhere on the stage plays it
(that window has one destination).

### 3.11 Inspector and card sheet

Desktop and tablet: the inspector at the left edge, a card face at 2.2×
(landscape for battlefields) with a caption block — kind · cost · printed →
current might (green above, red below [HS-4]) · active statuses with their
source ("stunned · Vi's ability, until your next turn") · for my facedown
card, "face down · Smoke and Mirrors" [HS-9]. It reads `Hovered.or(Selected)`
so it works on touch, and it never moves. Pinned by right-click or
long-press, unpinned by Esc or a tap elsewhere [MTGA-17].

Phones: no inspector slot. Long-press (or the **inspect** chip) opens the
**card sheet**, a bottom sheet with the art at 60 % of the width, the same
caption, and the card's chips repeated at the bottom; dismissed by the scrim,
back, or a drag down. Peeked faces (a look the plugin granted) show with the
"you may look" caption, as `preview_hud` does today.

### 3.12 Winner banner

A banner across the felt at the `banner` rect, not a window [HS-21]: "rae
wins · 8 points" in the seat colour, with **play again** (host redeals, a
client asks the host — the existing `new game` path) and **leave**; a small ×
keeps looking. "select deck" goes (it only reopened the lobby); "play again"
asks for a missing battlefield through the tray after the banner closes, so
nothing is centred twice.

### 3.13 The drawer — log, chat, tokens

A right-side drawer (300 pt desktop, 320 dp phones, full height) opened by
the three tabs under the primary button or by L, C, K: **log** (every
narration line and every toast as sentences, newest at the bottom, card
names tappable to the inspector [MTGA-16]); **chat** (the AI seat's chat and,
later, human chat with a six-emote row and a per-seat mute [HS-15, MTGA-20] —
the emote wire message is a later ask, so the tab ships with the AI chat
only); **tokens** (free table only: the spawn grid from
`src/table/tokens.rs:29-113` with real buttons, entering a placement mode
that lights the candidate zones — the same targeting grammar as a prompt).
The drawer is the only floating panel besides the strip; the AI window
(`src/ai/window.rs`), the tokens window and the sideboard window no longer
float.

### 3.14 History rail

Desktop and tablet only: the left edge under the phase bar, up to eight
40×56 tiles newest at the top, built from the narration lines the presenter
already emits ("{card 12} dies", "{seat 0} conquers {zone 3}") [HS-10]: the
source card's thumbnail (or a seat swatch) with a border in the acting
seat's colour and a glyph for the event class (play, death, conquer, attach,
draw). Hover or tap expands the sentence in the inspector caption. Lines are
classified by their first token until the structured `narration` list lands
(U11). This is how the AI's turn becomes legible after the fact. Phones keep
the ticker and the log.

### 3.15 Rules enforced vs free table — what is drawn

| control | rules enforced | free table |
|---|---|---|
| drag to a zone | only `view.legal` destinations light; a drop elsewhere eases back with no bytes | every zone |
| tap to exhaust | never | chip **exhaust**, double-tap |
| recycle · trash · draw · to deck | only what `view.legal` lists | chips and D/T/E |
| score ± · card ± · toggles | read-only | steppers in the card sheet and the seat plate |
| tokens tab and K | absent | present |
| chain resolve buttons | absent | present |
| propose / confirm free table | table menu only | — |
| sideboard reload | between games | any time |
| seat-view switch | spectators and FFA plates | plates |

The rule is mechanical: a control is drawn only if the plugin could accept
it, and the check is `view.legal` / `view.affordances` for enforced tables.
The mode is read from the view, so a table that confirms "free table"
mid-game flips every tool on in the frame the mode chip changes; the HUD
frame itself does not change, so a player learns one layout.

## 4. The verb grammar

### 4.1 Selection replaces hover as the source of truth

`Selected(Option<Entity>)` sits beside `Hovered`. Mouse hover sets a
transient selection; a tap or click sets a sticky one; a tap on the felt
clears it. The inspector, the chips, the counters popup (free table) and the
hovered hotkeys all read `Hovered.or(Selected)`, so selection survives the
pointer moving to a chip — the third leg of the facedown blocker (the offer
row vanished on `Pointer<Out>`, `src/table/plugin_ui.rs:494-517`).
`settle_hand_hover` (`src/table/interaction.rs:50-87`) is gated on an
`InputKind` resource (mouse, or last-input-was-touch from
`Touches::any_just_pressed`, cleared by the first mouse move) so on touch the
raised hand card is the selected one and stays raised until the selection
clears [MOB-5, MOB-12].

All card input passes through one pure classifier in `src/table/gesture.rs`
before anything touches `Held` or emits a message:

```
classify(press: Vec2, now: Vec2, held_ms: u32, moved_max: f32, kind: InputKind) -> Gesture
Gesture = Tap | DoubleTap | LongPress | DragStart | Drag | Drop | Cancel
```

Slop 8 dp on touch (Android's touch slop), 4 dp on a mouse; long-press ≥
500 ms with movement under the slop; double-tap = a second tap within 300 ms
and 24 dp. `DragStart` is emitted only past the slop, which fixes
bevy_picking's zero-threshold `DragStart` turning every finger jitter into a
drag that swallows the tap (`bevy_picking-0.19.1 events.rs:1053-1082`,
`interaction.rs:107-127`) [MOB-6]. `RecentDrag` keeps suppressing the click
after a real drag.

### 4.2 The verbs

| action | mouse | touch | hotkey |
|---|---|---|---|
| inspect (show in the inspector; show its chips) | hover | tap | Tab / Shift+Tab cycle `highlighted(view)` |
| pin the inspector / open the card sheet | right-click | long-press | I |
| select (lift it, show its chips) | click | tap | Tab |
| the default action of the selected card (its one enabled chip) | double-click | second tap | Enter |
| act with a specific chip | click the chip | tap the chip | 1–9 |
| play · move · hide · march | drag to a lit zone | drag to a lit zone | select + chip |
| play to the chain (hand card, my facedown card, react) | drag onto the chain panel, or double-click | drag onto the ribbon, or second tap | P (when the chip exists) |
| answer a prompt with a card | click the pulsing card | tap the pulsing card | 1–9 in banner order |
| answer a prompt with a non-card option | strip chip | banner chip | 1–9 |
| the primary action (pass · resolve · end turn · done · keep) | click the button | tap the button | Space (W is an alias) |
| cancel / skip / decline a prompt | the hollow chip, or Esc | the hollow chip, or back | X, Esc |
| cancel a drag | release on nothing | release on nothing | Esc |
| deselect | click the felt | tap the felt | Esc |
| scroll the hand | wheel over the fan | horizontal swipe on the drawer | ← → move the selection |
| camera | wheel zoom, middle-drag pan | pinch, two-finger pan, double-tap the felt resets | Home resets |
| hold this phase / hold | Ctrl-click the primary / Ctrl+Shift-click | long-press the turn plate | — |
| pass through (keep passing until the chain changes) | Shift-click the primary | — | Shift+Space |
| free table only: exhaust · trash · recycle · draw | chip | chip | E · T · (chip) · D |

Single tap has one meaning, select — except that a tap on a card wearing the
Answer rim while a prompt is open is the prompt's pick, because that rim is
visible state, not hidden state. Today single-click has three meanings
depending on affordance count and zone (`src/table/interaction.rs:289-361`).
Double-click never exhausts under rules enforced.

### 4.3 Chips — the touch-visible affordance list

When a card is selected, a row of chips (40 dp tall, ≥ 48 dp wide, at most
four, "more…" beyond) appears 6 pt below its screen rect (above it for the
hand), in this order:

1. every enabled affordance whose `card` is this one ("play a Sprite · 2⚡";
   an orange corner tag marks an available activation, greyed and reading
   "used this turn" once spent [HS-14]);
2. the hidden-card offers for my own cards from `hidden_actions` ("hide at
   Battlefield 2 · 1 rune", "play from hidden", "reveal") — the H/P/R offers
   unreachable on touch today and the desktop's only working facedown path;
3. **inspect** (always); on a free table **exhaust**, **trash**, **to deck**,
   **±**.

A disabled offer renders greyed with its reason as the chip's second line
("needs 3 energy · you have 1"), which is the greyed hint (`GREYED_HINT`,
`plugin_ui.rs:87`) moved to where the player is looking. If groups 1–2 hold
exactly one chip, it is the default action. Chips are numbered 1–9 for the
keyboard and stay until the selection changes.

### 4.4 Drag preview, the drop chooser, the chain drop

While a rimmed card is held, every zone in `Rims::destinations(card)`
(`src/table/highlight.rs:189`) tints in the rim's colour, everything else
dims to 70 %, and a provisional arrow runs from the card's origin slot to the
zone under the pointer through `arrows::plan` (`arrows.rs:266`) [HS-7,
MTGA-10]. The drawer tucks so the board is visible. Zone hit rects grow by
24 dp on every side; on phones the nearest lit zone within 48 dp of the
release wins. Releasing over nothing eases the card back and writes no
bytes.

When both a face-up play and a hide are legal at the dropped battlefield, a
**drop chooser** appears at the drop point: two 48 dp chips **play here** /
**hide here**. It waits until one is tapped or the player taps elsewhere
(cancel); there is no timeout — a timed choice punishes a hesitating new
player. Hidden destinations come from `Legal.hidden`, never guessed.

Dropping on the chain panel or ribbon is §3.10. Double-click on a hand card
*or my facedown board card* plays to the chain: the arm at
`interaction.rs:334-344` that routes any non-hand card to an exhaust toggle
checks `lies_facedown` first.

### 4.5 Targeting prompts, the mulligan, the March batch

A prompt whose options name cards is answered on the cards: candidates wear
the Answer rim and pulse, everything else dims to 45 %, a tap toggles (a
corner check for multi-pick), the strip counts "1 of up to 2", the primary
reads **done** once `picked ≥ min` [HS-13]. For a single-target play the
drag grammar also works: drag the source to the target and release. An arrow
follows the pointer from the source item to the hovered candidate. Esc/X or
the hollow chip cancels when the prompt allows [MTGA-11]. While a prompt is
open the drawer auto-tucks if the candidates are on the board and
auto-raises if they are in my hand.

The **mulligan** gets its own overlay in the `banner` rect: the hand in one
large row, tap marks a card with a corner ×, the primary reads "keep · 2 set
aside", the opponent's plate reads "mulliganing…" [MTGA-18].

The **March** is a batch inside the engine's own prompt, not a client-side
pre-selection: the first drag of a ready unit to a battlefield is the whole
gesture for one unit; the engine then opens `GroupMove` ("move others to
{zone} too?", `riftbound-turns/src/engine/march.rs:292-318`), one option per
other ready unit and `done`. kai renders that prompt as the batch: the other
ready units at the origin wear the Answer rim, a tap sends one (the prompt is
max 1 per round, so each pick is sent as made), an **all** chip on the
destination sends the rest in order, the primary reads "move 3" [MTGA-12].
A `Marching` set that emits moves in selection order would be refused after
the first ("answer the open question first"), which is why it is not built.

### 4.6 Automation — client-side, over the existing pass affordance

`src/table/auto.rs` holds pure decisions over `PluginView` plus the settings
[MTGA-1, MTGA-2, MTGA-4, MTGA-6]:

- **auto-pass** (Settings › play, on): when the primary is pass/resolve and
  `view.legal` has no `React` / `Answer` / `Activate` row for me, no stop is
  set on this phase and hold is off, kai sends the pass after a fixed
  600 ms. The delay is constant whether or not I hold a response, so timing
  leaks nothing [MTGA-8].
- **hold**: `HoldFocus { Off, ThisPhase, Held }` — Ctrl-click the primary (or
  long-press the turn plate) for this phase, Ctrl+Shift for held; a "holding"
  chip on the turn plate; cleared when the phase changes.
- **pass through**: Shift+Space (or Shift-click) arms "keep passing until
  something new lands on the chain or a prompt opens" (compare
  `view.chain.len()` and `view.prompt` between refreshes); the primary reads
  "passing…"; any click or key disarms.
- **auto-answer** (Settings › play "ask me anyway", off — so auto-answer is
  on): a prompt for me with exactly `min == max` enabled card options and no
  cancel is answered after the same delay; "order my triggers myself" and
  "assign damage myself" opt the `OrderTriggers` / `Assign` cases out.
- Two triggers of the same untargeted ability of the same card from one
  event are never asked at all: the engine places them in queue order
  ([rules-engine.md](../../../agni/wiki/design/rules-engine.md),
  the Triggers ruling), so "order my triggers myself" only matters for a
  real choice.

The "is there anything to do" question `decide` asks is `auto::offer`, and
it is the same function the AI seat's pilot asks before it wakes the model:
the bot passes, ends an empty turn, answers forced picks and sends a lone
roll without a model call, and the model's `hold` tool lets it sleep through
a stretch on purpose — see [bot.md](bot.md). The player's side is unchanged:
`decide` never presses end turn and never answers behind another seat's
prompt.

Auto-pass ships with `auto::decide` table tests and a kai-cli soak that shows
the AI seat's game unchanged with the desktop seat auto-passing; it is on by
default because it only fires when the player literally has no move, and
because every priority window in a Riftbound game otherwise needs a manual
pass. The end-turn confirm ("you still have plays — press again") is off by
default: it is a per-turn tax on the button a competitive player presses
most, and `RecentDrag` already refuses a click within 300 ms of a drop.

### 4.7 Hotkeys, consolidated

| key | effect |
|---|---|
| Space (W alias) | the primary button |
| Shift+Space | pass through |
| Enter | the default action of the selected card |
| Esc | cancel/skip when offered; else the back ladder |
| X | the prompt's cancel/skip/no (the presenter's key) |
| 1–9 | the nth strip chip when a prompt is open, else the nth chip of the selected card |
| Tab / Shift+Tab | cycle the selection through `highlighted(view)` |
| I | pin the inspector / open the card sheet |
| F · P · R | hide facedown / play from hidden / reveal on the selected card, only when the chip exists |
| L · C · K | log · chat · tokens (free table) |
| H · ? | help sheet |
| hold Ctrl | full control: every priority window stops while the key is down, nothing persists |
| Home | reset the camera |
| Ctrl-click, Ctrl+Shift-click the primary | hold this phase / hold |
| free table only: D · T · E | draw · trash · exhaust on the selected card |

Removed: B (chat is C, at the table only), L for camera lock (a setting),
F3, Shift+wheel (the plain wheel over the fan scrolls it), middle-drag on
phones, edge drift. The plugin's `key_of` table (`plugin_ui.rs:240-268`)
keeps `space`, `enter`, `x`, the digits and letters not in the table above;
a `ClaimedKeys` resource records the keys the plugin fired this frame so
kai's own handler skips them, closing the latent double fire. The presenter
tags only `w`, `space` and `x` today (`present.rs:10-12`); kai treats `w` as
Space rather than asking the presenter to retire it.

## 5. Responsive rules

### 5.1 Viewport classes

`src/viewport.rs`: `viewport_class(logical: Vec2) -> ViewportClass`, pure,
computed once per frame from the window's logical size in points (one point
is one dp on Android, one CSS px on web, one logical px on desktop):

| class | rule (points, before egui zoom) | reference | shots |
|---|---|---|---|
| **PhonePortrait** | width < 600 and height ≥ width | 360×800 | `360x800-*` |
| **PhoneLandscape** | height < 480 | 800×360 | `800x360-*` |
| **Tablet** | otherwise, width < 1100 | 1024×768 | `1024x768-*` |
| **Desktop** | width ≥ 1100 and height ≥ 480 | 1280×800, 1920×1080 | `1280x800-*` |

1280×800 is Desktop, 1024×768 is Tablet. `InputKind { Touch, Pointer }` is
a separate axis set by the last input event; phone classes assume touch,
tablet and desktop can be either. The class picks the layout; the input kind
picks the affordance style.

### 5.2 The egui zoom rule

UI zoom is applied with `ctx.set_zoom_factor` on top of the native scale,
never `scale_factor_override` [MOB-10]: PhonePortrait 1.15, PhoneLandscape
1.10, Tablet and Desktop 1.0, multiplied by the Settings › play "UI scale"
slider (0.8–1.5, persisted with tuning: tuning.json on desktop, localStorage
on web, app storage on Android). 3D picking uses logical coordinates and is
unaffected. On class or input-kind change one system rewrites the egui
style once [MOB-1]:

| | phone / touch | tablet | desktop |
|---|---|---|---|
| `interact_size.y` | 48 | 40 | 24 |
| `button_padding` | (16, 12) | (12, 8) | (8, 4) |
| `item_spacing` | (8, 8) | (8, 6) | (6, 4) |
| body text | 16 pt | 15 pt | 14 pt |

Every stock button in kai is 18 pt tall on a phone today.

### 5.3 Camera framing and the minimum card size

`scene::framing` fits the 12-unit depth to the viewport height
(`src/table/scene.rs:59-104`), which yields 42 pt cards at 800×360 and a
3.56-unit sliver at 360×800. It becomes
`framing_for(class, players, aspect, pitch, zoom, stage: Rect) -> Framing`,
solved against the `stage` slot, not the window, so the top bar and drawer
never cover cards:

- **Desktop / Tablet**: today's arena framing (pitch 62°, far edge to near
  band), solved for the stage rect.
- **PhoneLandscape**: pitch 62°; frame from the far seat's *inner* band
  (legend, champion, base) to my outer band; the far seat's outer band
  (decks, trash, runes) is cropped and its counts live in the top bar.
  Depth framed ≈ 7.2 units; a 260 dp stage gives ~40 dp per unit, so a
  board card lands at 40×56 dp — exactly on the floor below, which
  `min_card_pt` tests rather than assumes.
- **PhonePortrait**: pitch 72°; fit *width*: `quad_w + 2·FRAME_NEAR_INSET`
  units across the stage width, so at 360 dp nine units → 40 dp per unit;
  the two battlefields sit side by side at ~150 dp and my base spans the
  width. The far seat's outer band runs off under the `opp_strip`, which
  summarises it.

`min_card_pt(class)` is the validator: a board card must render at least
**56 pt tall** (≈ 40 wide at the 1 : 1.4 card ratio) on phones and 44 on
desktop; the solver raises zoom, cropping the far side first since the near
edge is pinned, until the near band's card meets the floor; the seat plate or
top bar carries what was cropped [MOB-8]. Hit boxes are padded to 48 dp by
picking the nearest card within 8 dp of the ray hit, as a second line of
defence. A raised hand card on a phone is ≥ 96 dp wide. `quad_w` stops being
a process-global atomic (`src/table/mod.rs:26-34`); `Extent` carries it and
`zones::anchors` takes it as a parameter, so the layout is a pure function
of the viewport description. `HAND_VISIBLE = 7` (`mod.rs:44`) becomes
`hand_visible(class, stage_width, spacing)`.

Camera on touch: pinch from `ctx.multi_touch().zoom_delta` (bevy's
`PinchGesture` is macOS/iOS only) into `tuning.zoom` within
`ZOOM_MIN..ZOOM_MAX`, two-finger translation into `pan_x/pan_z` within
`PAN_LIMIT`, double-tap on the felt resets [MOB-9]. The wheel keeps working
on desktop but `zoom_camera` gains the `egui_wants_pointer_input` guard that
`pan_camera` already has (INDEX defect 2, `scene.rs:189`). Zoom from the wheel
no longer persists; only the slider does. Two presets in Settings › look
(arena 62°, top-down 90°) replace the tilt slider; camera lock, edge drift
and their keys go (`src/table/camera.rs:99-160`).

### 5.4 The hand drawer (phones)

Tucked (72 dp portrait, 56 landscape): the cards' tops show as a tight row
with the cost pip and name, five across, overflow scrolls by swipe; a
playable card keeps its rim in the tucked state so "I have plays" reads
without raising. Tap raises it (188 / 168 dp): cards fan at 96×134 dp with
12 dp overlap, swipe scrolls (`HandScroll` from the swipe delta, replacing
the 420 pt slider, `src/table/ui.rs:358-397`), the rune chips stay visible in
`bottom_left` so cost and pool are read together [HS-16], and the primary
rises with the drawer. A `DragStart` on a raised card past the drawer's top
edge tucks the drawer and becomes a table drag; a drag from the tucked state
does nothing until the hand is raised [MOB-6]. Tap the felt or drag the
drawer down to tuck. Auto: a prompt with board candidates tucks; a prompt
with hand candidates raises; the drawer tucks when my turn ends. Desktop and
tablet keep the fan (`hand_slot`, `src/table/layout.rs:125`), hover lifts one
card, the wheel over the fan scrolls it, and a hand larger than what fits
shows a "1–7 of 9" chip at the fan's right end.

### 5.5 Sheets, wrapping and text entry on small screens

Every remaining `egui::Window` (`settings.rs:322`, `menu.rs:163`,
`sideboard.rs:153`, `counters.rs:156`, `battlefield.rs:134`, `winner.rs:33`,
`tokens.rs:45`, `ai/window.rs:25`) becomes a sheet through the helper of
§2.1. The **wrap rule**: `settings::boxed` (`src/settings.rs:142`) gains
`ui.set_max_width(inner_w)` and `style.wrap_mode = Some(TextWrapMode::Wrap)`,
so no label — the pinned note (`src/deck/pinned.rs:258`), the mode note
(`src/menu.rs:484-491`), the playmat row (`src/table/playmat.rs:397`), the
telemetry path — can widen a window past the screen; the playmat and seat
colour rows become `horizontal_wrapped` of fixed-size *buttons*, since
`horizontal_wrapped` of `vertical` groups does not wrap (INDEX defect 1).
`panel_metrics(class, box)` returns every width the panels use, replacing
`HEADER_W 380`, `desired_width 320/200/140`, `PANEL_W 560`, `LIST_H 340`,
`CHOICE_W 180`. `ui.columns(2)` is used only on Tablet and Desktop. A test
walks every lobby and settings section at 312 dp width and asserts no child
exceeds it. Sliders live only in advanced, and that scroll area sets
`DragScroll::Never` so the slider wins the drag.

Text entry on Android: while a `TextEdit` has focus, the containing scroll
area scrolls the focused rect above the IME inset (from the insets callback
below) [MOB-13]; on web the paste button remains the only reliable path, and
every field has one.

### 5.6 Safe areas, Android and the web

A `SafeInsets { top, bottom, left, right }` resource, zero on desktop. On
Android a JNI hop in `MainActivity` (`getRootWindowInsets` → display cutout
+ system-gesture insets + IME inset → `nativeInsets`, the same shape as
`nativeClipboard`) because winit 0.30 drops `InsetsChanged` [MOB-2]; on web
`env(safe-area-inset-*)` read through a small shim into the same resource.
`hud::layout` shrinks the screen rect by the insets, so the drawer's drop
threshold sits above the bottom gesture inset and a drop never summons the
nav bar; in landscape the cutout side gets the extra inset. Android moves
from `setSystemUiVisibility` to `WindowInsetsController` with
`layoutInDisplayCutoutMode=shortEdges` so the stage runs under the cutout
and the HUD does not; `enableOnBackInvokedCallback` stays off. `set_keep_awake`
is on while a session is active; on resume from Suspended with
`Recovery::Rejoin` the rejoin fires itself [MOB-14].

Orientation: the manifest stays `landscape` (`AndroidManifest.xml:13`) until
the portrait layout has shipped on web and been seen on a device; then it
moves to `fullUser` with a "lock orientation" toggle in Settings › play
(§10 U12). PhonePortrait is a full class from the start — it is the class a
phone browser and a narrow desktop window hit — so nothing about it waits on
the manifest.

`web/index.html`: `touch-action: none; overscroll-behavior: none;
user-select: none` on the canvas, `viewport-fit=cover`, `100dvh` body,
`user-scalable=no` [MOB-15]. Hosting stays desktop-only; joining from a
phone browser shows "locking the screen ends the table" in the waiting
banner.

## 6. Discoverability

A new player learns the grammar from the table, not from a manual:

- **Chips instead of tooltips.** Every offer is a visible, labelled chip on
  its card with its cost and hotkey digit; nothing important is hover-only.
  This replaces `on_hover_text` (`plugin_ui.rs:565`) and the strip's greyed
  hint.
- **The primary button says what it does** and prints its key; the turn
  plate says whose move it is in words. A player who reads nothing else
  still knows the two facts that matter.
- **Three coach marks**, once per device, persisted with tuning
  (`src/table/coach.rs`): on the first deal, "lit cards can be played — drag
  one to a lit zone" anchored to the first rimmed hand card; on the first
  prompt, "pulsing cards answer the question — tap one"; on the first pass,
  "this button always says what it does" on the primary. Each is a 240 pt
  callout dismissed by doing the thing or by "got it"; never more than one,
  never over the felt's centre; "show hints again" in Settings › play.
- **The idle hint**: when it is my action, no prompt is open and nothing has
  happened for 6 s, the ticker (phones) or the strip (desktop) reads one
  sentence from `idle_hint(view)` keyed to the strongest rim present: "drag
  a green card to play it, or press end turn" / "respond with a violet card,
  or pass". Shown at most twice per hint per device.
- **Rim legend**: the first time each rim appears in a session a 12 pt tag
  sits beside the card for 3 s ("playable", "attack", "ability", "respond",
  "choose", "hide"); the permanent legend is in the help sheet.
- **Help sheet** (?, or "help" in the table menu): the verb table of §4.2 in
  two columns (mouse / touch), the hotkeys, and the legend of rims and
  status chips. There is no legend anywhere today.
- **Empty states carry the next action**: the deck box with no deck says
  "paste a list or pick a scripted deck"; the seats group with one seat says
  "invite or add AI"; the join list says "paste their ticket". The empty
  table label ("create a table or import a deck to begin",
  `src/table/ui.rs:46-60`) is replaced by a centred card with one button,
  "back to the lobby".
- **Refusals teach**: the toast is the engine's own sentence, the disabled
  chip says why, and a refusal repeated three times adds "see ? for the
  verbs". A player never sees a card snap back in silence.
- **Touch tooltips**: every icon button shows its label on long-press; every
  chip has a text label; no emoji-only buttons (the gear/🃏 row today).

## 7. Visual system

**Spacing** on a 4 pt grid: 4 inside a chip, 8 between siblings, 12 HUD
gutter, 16 panel padding, 24 between groups; screen margins 16 (phones) /
24 (desktop). Radius 8 for panels, 12 for sheets, pill for chips and the
primary, 4 for small chips. Panels are 92 % opaque on the felt with a 1 px
hairline. Touch rows 56–72 dp; icon buttons 48 dp with a 24 dp glyph.

**Type**, one family (the bundled egui font plus a glyph set covering ‹ › ×
⎵ ● ◉ ◐ ⬢ ⚡ and tabular digits so pips and counts do not jitter):

| role | phone | desktop |
|---|---|---|
| display (Home title, winner) | 28 | 32 |
| title (turn plate line 1, sheet titles) | 20 | 18 |
| body (strip, chips, rows) | 16 | 14 |
| label (turn plate line 2, captions) | 14 | 12 |
| meta (counts, hotkey glyphs) | 12 | 11 |

Semibold for titles and the primary button; "weak" only for captions, never
for instructions.

**Colour**: two token sets in `src/theme.rs`, chosen by Settings › look
(system default). The table HUD is always dark because it sits on a felt;
Home, lobby and settings follow the theme.

| token | dark | light |
|---|---|---|
| surface | `#1C1C22` at 92 % | `#F6F5F2` at 94 % |
| surface-2 (chips, rows) | `#2A2A33` | `#E9E7E1` |
| ink | `#F2F2F5` | `#1A1A1F` |
| ink-weak | `#A9A9B5` | `#5C5C66` |
| primary green (nothing else to do) | `#38A05C` | `#1B8F4E` |
| amber (plays remain; refusal fill at 20 %) | `#F0B232` | `#C77800` |
| grey (waiting) | `#46464C` | `#B8B8C0` |
| danger (refusal text, disconnected, concede) | `#FF6B6B` | `#B3261E` |
| scrim | black 45 % | black 30 % |

Seat colours stay the five pickable `SEAT_COLORS` (`src/table/colors.rs:7-16`)
as identity — the swatch, the chip fill on the score plate, the rule on a
held battlefield — but a colour is never a name (§2.8). The primary button's
fills are not seat colours: it is labelled, and seats are identified by
swatch and name.

**The rims.** Seven rim kinds today with no legend (`src/table/highlight.rs:17-25`,
`47-57`) is too many hues to learn [HS-2]. Five legality rims remain, each
pairing a hue with a stroke so colour is never the only signal [MTGA-24]:

| rim | meaning | colour | stroke |
|---|---|---|---|
| Play | you may play this | green `#8CF060` | solid 2 dp |
| March | you may attack with this | cyan `#28C8FF` | solid 2 dp with a chevron at the top edge |
| Activate | you may use this ability | orange `#FF9600` | solid 2 dp with the corner tag |
| React | you may respond with this now | violet `#D650FF` | dashed 2 dp |
| Answer | the game is asking about this | pale `#BEC6D6` | solid 3 dp, pulsing |

Hide is a way of playing the card, so it is the Play colour dotted; Enemy
(`#FF2882`) is not a legality kind but a threat ring drawn outside the legal
rim only while an enemy item aims at the card; Refused is a one-shot amber
pulse. The M0 dashed affordance rim in the seat colour (`plugin_ui.rs:310-366`)
is dropped: a card that is a click target for a prompt option is an Answer
card. Play, March and Activate stay distinct rather than folding into one
"act" colour because a march is an attack and a player reads that difference
at a glance. The test at `highlight.rs:616` that no rim is confusable with a
seat colour or an arrow stays; a new one asserts the five hues are pairwise
apart in both themes. The colour-blind palette swaps Play/Enemy to blue/orange
and adds a seat glyph (● ■ ▲ ◆ ★) beside every swatch.

**Status chips and badges** under a unit: a might badge (printed → current,
green above, red below), a damage badge (red), and statuses as frames or
glyphs rather than words [HS-4]: stunned = frost frame; exhausted = the tilt
plus a 60 % dim; attacker = sword glyph in the attacker's seat colour;
defender = shield; equipped = gear glyph; empowered = gold frame; temporary =
hourglass — drawn from a small glyph set in `assets/` so they do not depend
on emoji coverage. Chips are 20 pt, stack to the right, truncate to "+2"
beyond three, and clip at the card's top so a dense zone never covers the
card below (`src/table/counters.rs:393-496`). Points are pips; XP a numeral
chip; runes a row of ready/spent chips in `bottom_left` on phones and in my
seat plate on desktop [HS-1].

## 8. Removed, merged and re-homed — every inventory item

| today | where | verdict |
|---|---|---|
| Version / wire badge on every screen | `src/settings.rs:166-180`, chained at `:92-101` | Settings › advanced footer only |
| Gear / 🃏 quick-import / menu row | `src/settings.rs:182-211`, `src/deck/import.rs:808-828` | gear on Home and lobby; ≡ top-left at the table opens the table menu; 🃏 removed, its clipboard path is the deck box paste button |
| Games screen with module hashes and node line | `src/menu.rs:191-245`, `:68-87` | Home, §2.2; lines to advanced |
| Lobby header: tofu back glyph, plugin line, "open the table without a session" | `src/menu.rs:262-283`, `:263` | ‹ back, name only; the sandbox is advanced › "open a tuning table" |
| Lobby deck column, free tables (import panel + history) | `src/menu.rs:294-307` → `src/settings.rs:368-395` → `src/deck/import.rs:931-1152`, `src/deck/history.rs:429-479` | the deck box, §2.4, the only instance |
| Lobby deck column, pinned decks and paragraph | `src/deck/pinned.rs:249-309`, selection `src/menu.rs:297-304` | scripted tiles in the deck box; `Side` becomes an open list (`pinned.rs:9-49`) |
| Lobby play column: host/join, session state, `info.status` log line, recovery | `src/net/mod.rs:1607-1667`, `1669-1755`, `1554-1604` | opponent group (friends · join), seat rows, one amber recovery card; `info.status` never rendered as a line |
| Lobby game settings: enforced checkbox off by default, mode note, playmat, seat colour | `src/menu.rs:452-593`, `484-491`, `511-518`, `595-641` | rules group with enforced default; one-line note; playmat and colour to Settings › look |
| Lobby AI section: model box, presets, log path, nanosecond battlefield | `src/menu.rs:332-450`, `422-427` | opponent › AI (deck + fast/thinking); the rest to advanced; first battlefield |
| Settings shell: five tabs, 380 px identity header, no close, "save" | `src/settings.rs:305-366`, `220-246`, `src/net/identity.rs:68-128` | a sheet, ×, four tabs, identity on "you" |
| Settings › table: nineteen sliders, sample hand, playmat, colour | `src/table/ui.rs:62-110`, `src/settings.rs:397-422`, `src/table/playmat.rs:387-478`, `src/table/colors.rs:118-176` | six player entries to look; sliders and sample hand to advanced |
| Settings › multiplayer: host/join, game radios, compact history, identity, peers | `src/settings.rs:270-298`, `src/net/mod.rs:1473-1519`, `1669-1755` | removed; host/join in the lobby; identity and peers on "you"; radios gone |
| Settings › decks | `src/settings.rs:299`, `368-395` | removed; the deck box |
| Settings › modules | `src/engine/modules.rs:1195-1235` | advanced |
| Settings › telemetry | `src/telemetry.rs:597-668` | advanced |
| Battlefield chooser window, four openers, default order under the lobby | `src/deck/battlefield.rs:97-202`, `52-85`, `:134`; `src/menu.rs:463-465`; `src/deck/pinned.rs:297-299`; `src/deck/import.rs:1110-1112` | a step in the deck box; at the table only as a tray prompt after "play again" |
| Sideboard toggle and window, registered unconditionally | `src/deck/sideboard.rs:112-266`, `src/table/mod.rs:159-161` | a step in the deck box; the bottom-centre badge leaves every screen; reload only between games |
| Empty-table label | `src/table/ui.rs:46-60` | a centred card with "back to the lobby" |
| Plugin prompt strip: status paragraph, greyed hint, hover-only offers, wrapped button row | `src/table/plugin_ui.rs:421-580`, `87`, `391-419`, `489-556` | turn plate, strip/banner, chips, primary button, seat plates, ticker, history, log |
| Seat-colour dashed affordance rim | `src/table/plugin_ui.rs:310-366` | dropped; Answer rim marks prompt candidates |
| Seven legal rims, no legend | `src/table/highlight.rs:404-465`, `42-57` | five rims + Hide dotted + Enemy ring + Refused pulse, with a legend |
| March tint | `src/table/highlight.rs:466-560` | kept; also live during a drag |
| Target arrows with hard-coded chain row anchors | `src/table/arrows.rs:188-212` | kept; anchors from the chain panel's live rects |
| Chain HUD at RIGHT_TOP over the seat buttons; textual chain line | `src/table/ui.rs:126-239`, `:184`; `present.rs:286` | chain panel in its slot, drop target; text line dropped; resolve buttons free only |
| Card preview HUD at LEFT_TOP under the gear | `src/table/ui.rs:244-340`, `:326` | inspector in its slot, driven by hover or selection; card sheet on phones |
| Seat buttons | `src/table/ui.rs:399-432` | removed; seat plates are clickable in FFA and free tables |
| Zone labels without owner; card name labels | `src/table/ui.rs:434-482`, `3-44`, `src/table/zones.rs:286-292` | kept; owner prefix on the far side; held/contested rule on battlefields |
| Hand scroll slider, 420 pt at CENTER_BOTTOM | `src/table/ui.rs:358-397`, `:383` | removed; wheel over the fan, swipe on the drawer, a "1–7 of 9" chip |
| Score window at RIGHT_BOTTOM, always open | `src/table/counters.rs:144-190`, `:157`, `15-24` | seat plates; ± free only |
| Hovered-card counters popup | `src/table/counters.rs:287-391` | free table only, driven by selection; read-only in the inspector caption otherwise |
| Counter badges and status chips stacking over the card below | `src/table/counters.rs:393-496` | restyled glyphs, rightward stacking, "+2" |
| Tokens window over the strip, K only | `src/table/tokens.rs:29-113`, `:46`, `src/table/ui.rs:544-547` | drawer tokens tab, free only, with a button and K |
| Winner dialog centred with the chooser; "select deck" | `src/table/winner.rs:16-70`, `:34` | banner across the felt; play again · leave |
| AI player window at RIGHT_BOTTOM, B everywhere | `src/ai/window.rs:5-128`, `:26`, `:13-16` | drawer chat tab; C at the table only; presets to advanced |
| wasm bridge status line at CENTER_TOP | `src/net/gateway.rs:195-222`, `:217` | a connectivity dot in the seat plate |
| Session status and refusals written to `info.status` only | `src/net/mod.rs:332-345`, `904-925`; reads at `1639`, `1751-1752` | `Refusal` + `LastIntent`, the toast at the card |
| Mouse gestures: three-meaning click, undocumented double-click, right-click recycle | `src/table/interaction.rs:289-361`, `259-287` | the grammar of §4 |
| Camera: wheel without the egui guard, L lock, edge drift, middle-drag on touch | `src/table/scene.rs:189-245`, `src/table/camera.rs:99-160` | guarded wheel, presets, pinch/pan; lock and drift removed |
| Sixteen hotkeys with no on-screen twin; plugin key double fire | `src/table/ui.rs:504-648`, `src/table/plugin_ui.rs:240-308`, `src/menu.rs:126-144`, `src/settings.rs:105-130` | the table of §4.7; `ClaimedKeys` |
| Android: dead back key, no insets, immersive flags, landscape lock | `src/menu.rs:135`, `src/settings.rs:111`, `MainActivity.java:125-138`, `AndroidManifest.xml:12-14` | logical back key, `SafeInsets`, `WindowInsetsController`; the lock stays until U12 |
| Overlay collision map | `settings.rs:169,192; ui.rs:184,326,383,410; plugin_ui.rs:463; gateway.rs:217; tokens.rs:46; battlefield.rs:135; winner.rs:34; counters.rs:157; ai/window.rs:26; sideboard.rs:135` | `hud::layout`; nothing anchors itself |

Developer controls, all of them, behind Settings › advanced › "show
developer settings": the version and wire badge; module store rows and
reload; node/peer summary, refs and wire bytes; the hand/foil/easing sliders;
"deal a sample hand"; "open a tuning table"; "download full riftbound set";
telemetry shipping, token and the log viewer; the AI model string, vendor
presets and log path; the seat-view switcher outside FFA and free tables.
None is reachable from Home, the lobby or the table.

## 9. The bug list, and where each lands

| bug (phase 1 severity) | where it lands |
|---|---|
| BLOCKER · a facedown card cannot be played from a battlefield: double-click routes non-hand cards to exhaust (`interaction.rs:334-344`), the chain is Offstage so there is no drop target (`riftbound/src/lib.rs:157-166`), the hover-only offer row vanishes on `Pointer<Out>` (`plugin_ui.rs:494-517`) | U3 (double-click checks `lies_facedown`; `Selected` keeps the offers), U4 (the chain panel is a drop target), U6 (chips) |
| HIGH · refusals invisible at the table | U3 (`Refusal`, `LastIntent`, the toast) |
| HIGH · `tables` never lists the hosted table: both spirit stores persist the other side's id in `forgotten`, `TableBook::learn` drops adverts for unknown hosts, `admits_introduction` refuses re-introduction (`spirit/node/src/tables.rs:66-118`, `mesh.rs:118-124`, `639-660`) | not a kai UI change; a spirit-node fix tracked beside this plan. The lobby's join empty state offers paste-a-ticket (U5) so direct join works meanwhile |
| INDEX defects 1–8: unwrapped labels, unguarded wheel, chooser under the lobby, tofu back glyph, no settings close, tile growth, overlapping badges, the stale deal behind the lobby | U2 (all eight, before any redesign) |
| touch: taps become drags (no threshold) | U6 (the gesture classifier) |
| Android back key dead; no safe areas; IME covers the field | U7 |
| web canvas: no `touch-action`, browser pinch, pull-to-refresh | U7 |
| MEDIUM · wasm gas trap has no graceful refusal | agni engine work (an iteration cap in `cleanup::run` and `triggers::collect` returning `Refusal`), outside this plan |
| MEDIUM · free-table proposal hangs in the status; the AI must blacklist "confirm free table" | the UI half in U4: the offer lives only in the table menu, never beside end turn; withdraw/expire and hiding it from the AI's list are the agni asks in U11 |
| MEDIUM · roadmap tails (spectators, rewind, host migration, foreground service, art by hash, relay) | out of scope; `hud::layout` reserves a rect for a rope and a spectator plate so they land without moving anything |
| LOW · AI seat follow-ups (card_text prompt line, reasoning flag, plan seed) | out of scope for the UI; unchanged |
| LOW · floating energy pool for manual rune taps | closed: refused under enforcement, moot; not built for the free table |
| LOW · Blade Dancer trigger, Rockfall Path ruling | verified not bugs; no change |
| the agni status page saying the lobby toggle is "next" | fixed in the docs pass that accompanies this page |

## 10. The implementation plan

*Status (2026-09-11): U1–U4 landed as v0.9.2–v0.10.0 (phase 3a, commit
`00ea79a5` together with the engine's M9); U5–U11 landed together as phase
3b in kai v0.11.0 with the riftbound plugin at 0.4.0, each milestone's
paragraph below carrying its own landed note and its section in `table.md`.
The integration pass closed the seams the parallel milestones left each
other: the four Esc ladders read `viewport::back_pressed` (the logical back
key) and consume it through `consume_back`; the plugin's own Space honours
the end-turn confirm (`primary::confirm_guards`); the turn plate carries the
`holding` chip; the seat plates, chain rows and lobby seat rows draw
`colors::swatch` with the colour-blind glyph; Home, the lobby, the deck box
and every sheet read `theme::tokens` and `theme::dress` so the light theme
reaches them; `hud::sheet` fills with `surface_opaque()`; the phone chain
rail is a ribbon of thumbnails that opens a sheet and is absent while the
chain is empty; the phone lobby footer reserves the reason line at the touch
style; the id-scoped art ingest lives in agni's `ingest-riftbound`
(`--audit`, `--only`, `--ids`, `--pool`) with `scripts/ingest-pool-art.sh`
as its wrapper. The shots were taken on the U1 harness in the session's
scratchpad and are not kept in the tree. The review of 3b landed as v0.11.1;
what it changed is [table.md — The phase-3b review](table.md#the-phase-3b-review).
U12 remains.*

Twelve milestones. U1 and U2 are sequential and small; U3 and U4 are the
table's structural change; after U4 the pairs (U5, U6, U7) own disjoint
files and run in parallel; U8–U10 close out; U11 is the only milestone that
touches `agni/agni` and deploys from main; U12 is a second release. Each
milestone is one branch, one version bump (patch unless noted), tests in
the files named, no comments in code, and a prose section in this page or
`table.md` when it lands. Every milestone from U2 on ends with the four
screenshot sizes re-captured through the U1 harness and compared against
INDEX; a milestone is done when none of its named defects reproduce at any
size and its tests pass headlessly.

### U1 — the harness

Scope: `KAI_WINDOW=WxH` sets the primary window size; `KAI_SHOT=path` (or
F12) writes a screenshot through bevy's screenshot API on the next frame;
`viewport_class` and `InputKind` as resources. Files: `src/app.rs`,
`src/viewport.rs` (new), `src/table/mod.rs` (registration). Tests:
`viewport_class` at the four sizes plus the boundaries (599×800 portrait,
1100×480 desktop, 1024×479 landscape). Done when the Xvfb/lavapipe run in
the phase-1 procedure produces the four `*-table-default.png` from one
command. Half a day. *Landed in v0.9.2; the procedure and the resources are
described in [table.md — The screenshot harness](table.md#the-screenshot-harness).*

### U2 — stop the bleeding

Scope: the eight INDEX defects with no redesign. The wrap rule in
`settings::boxed` and `panel_metrics`; tiles at a fixed size with wrapped
text; ‹ back; × close on settings; the wheel guard in `zoom_camera` and
zoom no longer persisted from the wheel; the chooser at `Order::Foreground`
until the deck box replaces it; the sideboard toggle gated to the lobby;
cards hidden while `menu.screen != Table` in `src/table/sync.rs`. Files:
`src/settings.rs`, `src/menu.rs`, `src/table/{scene,playmat,colors,sync}.rs`,
`src/deck/{pinned,battlefield,sideboard,import}.rs` (text widths only).
Tests: `panel_metrics` at four sizes; the tile row fits three at 1024 and
one at 360; the 312 dp width walk over every lobby and settings section.
Done when the lobby with rules enforced ticked shows both the back button
and the host button at all four sizes, settings › table at 1024 shows its
title, and no cards show behind the lobby after "close table". *Landed in
v0.9.3; the wrap rule, `panel_metrics` and the 312 dp walk are described in
[table.md — Stopping the bleeding](table.md#stopping-the-bleeding).*

### U3 — refusals, the tools gate, selection, the facedown double-click

Scope: `Refusal`, `LastIntent` and the three renderings (strip row, toast at
the card, shake plus rim pulse); `Tools { free }` from `enforced(view)` with
one `if tools.free` at every draw site of §3.8; `Selected` declared and read
by the preview, the offer row, the counters popup and the hotkeys through
`Hovered.or(Selected)`; the double-click arm checks `lies_facedown` before
exhaust. Files: `src/net/mod.rs` (writes only), `src/table/{mod,toast,plugin_ui,counters,tokens,ui,interaction,anim}.rs`
(`toast.rs` new), `src/deck/sideboard.rs`. Tests: `Tools::from(view)` on the
"rules enforced" fixture makes every free verb inert; a `Notice` within two
seconds of a `LastIntent` anchors to the card; a double-click on my facedown
card in the M6 fixture emits `CardDropped { to: stack }`; the log keeps the
last five refusals. kai-cli observation: a refused move on the AI seat prints
the Notice while the desktop shows the toast. Done when the harness clicks a
board card under rules enforced and the shot shows the amber row and no
`ExhaustToggled` on the wire. *Landed in v0.9.4; the queue, the three
renderings, the gate sites and the click plan are described in
[table.md — Refusals, the tools gate and selection](table.md#refusals-the-tools-gate-and-selection).*

### U4 — the HUD frame

Scope: everything visible in §3 except the history rail, the drawer
contents, the phone rects and the phase bar. `hud::layout` with the desktop
and tablet rows and the non-overlap test; `classify_status` with its test
table; `seat_label`; the turn plate and seat plates (`plate.rs`); the
primary and secondary buttons as `primary_of(view) -> Option<Primary { affordance, label, tone }>`
with the hotkey glyph and the `RecentDrag` guard (`primary.rs`); the strip
reduced to its five states plus the tray (`TrayItems`); the chain panel in
its slot as a drop target (`chain.rs`); the inspector in its slot
(`inspector.rs`); the winner banner; the zone labels with owner and control;
the combat plate; the hand slider replaced by wheel-over-fan and the count
chip; arrows reading live anchors; the table menu sheet with the free-table
offer moved into it; the `sheet()` helper. Files: `src/table/{hud,plate,primary,chain,inspector}.rs`
(new), `src/table/{plugin_ui,ui,counters,winner,arrows,scene,mod,colors}.rs`,
`src/deck/battlefield.rs` (tray items), `src/net/gateway.rs` (line removed),
`src/settings.rs` (badge removed). Tests: `hud::layout` non-overlap at four
sizes × chain 0/1/5 × drawer states; `primary_of` over the M1/M2/M3 fixtures
(pass → "pass", chain non-empty → "resolve", prompt → "done", other seat →
disabled); `classify_status` over the presenter's strings; `seat_label`;
the primary is never one of the strip's chips. Shots: table-default at four
sizes with the hand uncovered and the plates and chain not overlapping after
three chain items (kai-cli's AI seat casts them). Version: minor (0.10.0) —
the table's whole surface changes. *Landed in v0.10.0; the layout, the
parser, the plates, the primary, the strip's states, the tray, the chain
drop target, the inspector, the banner and the table menu are described in
[table.md — The HUD frame](table.md#the-hud-frame).*

### U5 — Home, lobby, deck box, opponent panel, Settings sheet

Runs in parallel with U6 and U7 (no table files). Scope: §2 in full.
`src/menu.rs` splits into `src/menu/{mod,home,lobby,deckbox,opponent}.rs`
(`Menu::back` implements the ladder); `src/settings.rs` into
`src/settings/{mod,play,look,you,advanced}.rs`; `pinned::Side` becomes the
open pool-deck list; the import box merges; the coverage chip; enforced by
default; the AI's first battlefield; `net_section` and `host_controls`
deleted, `lobby_section` becomes the opponent group; identity and peers on
"you"; modules and telemetry under advanced; the chat body moves toward the
drawer (reachable from the table menu until U9). Files: `src/menu/*`,
`src/settings/*`, `src/deck/{import,history,pinned,battlefield,sideboard}.rs`,
`src/net/{mod,identity,peers}.rs` (UI functions only), `src/ai/window.rs`,
`src/engine/modules.rs` and `src/telemetry.rs` (render entry points).
Tests: `Menu::back` ladder (table → menu sheet → closed; lobby → home);
`coverage(deck, pool) -> (n, total)`; a pure `lobby_sections(game, enforced, role) -> Vec<Section>`
per mode; the settings default tab is play; the developer toggle hides
advanced; the primary verb per lobby state. Shots: lobby at four sizes in
each opponent segment; settings › play at 360×800 with the × visible. *Landed: the screens, the ladder, the deck box, the opponent group, the
settings sheet, the pool labels and the coverage chip are described in
[table.md — Home, lobby, deck box and settings](table.md#home-lobby-deck-box-and-settings);
shipped in v0.11.0 with U6–U11.*

### U6 — the verb grammar

Runs in parallel with U5 and U7. Scope: §4 minus automation. The gesture
classifier (`gesture.rs`); chips (`chips.rs`) with disabled reasons and the
1–9 keys; `on_click_card` → select, second tap/double-click → the default
action, prompt candidates pick on tap; right-click and long-press pin;
`on_drag_start` honours the slop and refuses unrimmed cards under rules;
the drag preview tint and provisional arrow; the drop chooser without a
timeout; `GroupMove` rendered as the batch with the **all** chip; the
mulligan overlay; Tab cycling; `ClaimedKeys`; the hotkey table of §4.7; the
hover-only offer row deleted. Files: `src/table/{gesture,chips}.rs` (new),
`src/table/{interaction,highlight,arrows,plugin_ui,ui,inspector}.rs`.
Tests: the classifier table (7 pt = tap, 9 pt = drag, 450 ms still =
long-press on touch; 3 pt / 5 pt on a mouse); chips for the M6 hidden
fixture list "play" and "reveal" for my facedown card and nothing for the
opponent's; a March fixture shows the Answer rim on the other ready units and
"move 2" as the primary; key claims never double-fire. kai-cli observation:
the AI seat's log shows the GroupMove picks after a desktop drag and taps.
*Landed: the classifier, the chips, the click plan, the drag preview and
chooser, the March batch, the mulligan overlay, the pins, the Esc ladder and
`ClaimedKeys` are described in
[table.md — The verb grammar](table.md#the-verb-grammar).*

### U7 — responsive: phone rects, framing, drawer, safe areas, back, web

Runs in parallel with U5 and U6. Scope: §5. `hud::layout`'s phone rows
(appended, not edited); `framing_for` with the portrait width fit and the
landscape inner-to-outer crop; `min_card_pt`; `Extent` owns `quad_w`;
`hand_visible`; the drawer states, swipe scroll, drag-out tuck, auto
tuck/raise; the opponent strip's data as a resource from `sync.rs`; pinch
and two-finger pan; presets; the egui zoom rule and touch style; sheets on
phones with `DragScroll::Never` for sliders; `SafeInsets` from the JNI hop
and the CSS shim; the logical back key; keep-awake and auto-rejoin;
`WindowInsetsController`; IME scroll; the web canvas CSS. Files:
`src/viewport.rs`, `src/app.rs`, `src/table/{scene,camera,layout,anim,zones,sync,hud}.rs`
(hud: phone functions only), `src/os/{android,ime}.rs`,
`android/app/src/main/java/blue/rae/kai/MainActivity.java`,
`android/app/src/main/AndroidManifest.xml` (cutout mode only),
`web/index.html`, `src/menu/mod.rs` and `src/settings/mod.rs` (key handling
only), `src/net/mod.rs` (resume path). Tests: `min_card_pt` ≥ 56 at 800×360
and 360×800; `hand_visible`; the drawer state machine (raise, tuck, drag-out,
auto on `Mulligan` and `Target` fixtures); inset subtraction; `hud::layout`
non-overlap re-run with the phone rows. Shots: 800×360 and 360×800
table-default with a board card ≥ 56 pt, the primary in the corner, nothing
under the drawer, the opp strip readable. The Android pieces cannot be
proven headlessly; they are verified on a device before U12. *Landed: the phone rows, `framing_for`, the drawer, the opponent strip, the touch camera, `SafeInsets`, the back key, the IME scroll, the web CSS and the Android insets are described in [table.md — Responsive](table.md#responsive--the-phone-rows-the-framing-the-drawer-the-safe-areas); the style route replaces `set_zoom_factor` for the reason given there.*

### U8 — automation

After U4 and U6. Scope: `src/table/auto.rs` with `HoldFocus`, `Stops`, the
auto-pass timer, pass-through and auto-answer as pure decisions; the
Settings › play entries in `src/table/tuning.rs` (`auto_pass`, `ask_anyway`,
`order_triggers`, `assign_damage`, `confirm_end_turn`, `fast_anim`,
`hand_left`, `ui_scale`, `colour_blind`, `theme`). Files:
`src/table/{auto,tuning,primary}.rs`, `src/settings/play.rs`. Tests:
`auto::decide` table — no React row → pass after the delay; a stop on this
phase → hold; Held → never; a `min == max` prompt → answered;
`OrderTriggers` with "order my triggers myself" on → not answered (with it
off, the default, the order is answered in presenter order like any other
forced pick); the delay is identical
with and without a React row. Done when a kai-cli soak with the desktop seat
auto-passing produces the same game the manual seat did on the same seed. *Landed: `auto.rs`'s decisions, the timer, hold, stops, pass-through, the auto-answer,
the end-turn confirm, the play tab and the soak's auto seat are described in
[table.md — Automation](table.md#automation).*

### U9 — history rail, drawer, badges, rims

After U4 and U6. Scope: `history.rs` (tiles from the classified lines);
`drawer.rs` (log with toasts, chat from `src/ai/window.rs` which is then
deleted, tokens as a placement mode on free tables); the badge restyle; the
rim system of §7 with stroke patterns and the family test; the colour-blind
palette. Files: `src/table/{history,drawer}.rs` (new),
`src/table/{counters,highlight,tokens,colors}.rs`, `src/ai/window.rs`.
Tests: rim precedence and pairwise distance in both themes; badge stacking
never exceeds the card width; the log keeps the last five toasts; the drawer
opens on L/C/K and closes on the ladder. *Landed: the narration merge and the
classifier, the rail, the drawer with its three tabs, the token placement
mode, the badge dress and stacking, the stroke patterns, the two palettes and
the seat glyphs are described in
[table.md — History rail, drawer, badges and rims](table.md#history-rail-drawer-badges-and-rims).*

### U10 — discoverability and the visual pass

After U9. Scope: `coach.rs` with the persisted seen set; `idle_hint(view)`;
the help sheet (`src/help.rs`); rim legend tags; touch tooltips;
`src/theme.rs` with both token sets and the felt tint per theme; the glyph
set in `assets/`; animation beats on chain resolution, interruptible, gated
by fast animations. Files: `src/table/{coach,anim}.rs`, `src/help.rs`,
`src/theme.rs`, `src/settings/look.rs`, `assets/icons/*`. Tests:
`idle_hint(view)` per rim kind; the seen-state round trip; chip ink
contrast ≥ 4.5 : 1 in both themes. Shots: a first-run table shows mark 1;
after a drag it is gone; one table shot per theme. *Landed: the coach marks,
the idle hint, the legend tags, the help sheet, the token sets, the glyph set
and the beat are described in
[table.md — Discoverability and the visual pass](table.md#discoverability-and-the-visual-pass);
the light theme reaches the room, the menus and the sheets through
`theme::tokens` and `theme::dress`.*

### U11 — structured view fields, the phase bar, stops (the agni milestone)

The only milestone touching `agni/agni`; additive and serde-defaulted so
`WIRE_VERSION` stays at 5, but the riftbound plugin needs a rebuild and
republish, so it deploys from main together with the kai release (the
wire-version deploy rule). Scope: `PluginView` (`sim/src/wire.rs:392`) gains
`#[serde(default)]` fields — `turn: Option<TurnInfo { number, seat, phase, phases: Vec<String>, mode }>`,
`seats: Vec<SeatInfo { seat, points, victory, xp, hand, deck, runes_ready, runes_total }>`,
`waiting: Option<Waiting { seat, what }>`, `narration: Vec<String>`,
`primary: Option<u16>` — filled by the presenter's `free`, `enforced` and
`lobby` while the old status lines keep being emitted for one release; SDK
builders `turn()`, `seat()`, `waiting()`, `narrate()`, `primary()`. kai's
`plate.rs` and `history.rs` prefer the fields with `classify_status` as the
fallback; the phase bar with stops appears on pointer classes; the strip
drops narration when `narration` is present. Also filed here as asks with
graceful degradation: the hidden concede affordance; withdraw/expire for the
free-table proposal and hiding "confirm free table" from the AI's list; XP
as a seat counter. Files: agni `sim/src/wire.rs`, `plugins/sdk/src/view.rs`,
`games/riftbound-turns/src/present.rs` (+ tests); kai
`src/table/{plate,history,auto,plugin_ui}.rs`. Tests: presenter goldens for
the new fields on the existing fixtures; `plate` prefers the fields and
falls back to the parser on a view without them; stops toggle on the chip
and persist. *Landed: the six fields, the presenter, the hidden concede and
free-table verbs, the proposal's withdraw and expiry, the field-first plate,
history and strip, and the phase bar are described in
[table.md — Structured view fields, the phase bar and stops](table.md#structured-view-fields-the-phase-bar-and-stops);
`WIRE_VERSION` stayed at 5. The primary button still derives its label from
the affordances (`primary_of`) and only checks against the `primary` field in
tests; XP was already a seat counter.*

### U12 — the second release: portrait on Android, emotes, concede

After U7 has been seen on a device. Scope: `AndroidManifest.xml` to
`fullUser` with the "lock orientation" toggle; the share sheet for the
ticket; the dark-mode hook; the half-resolution render target and low foil
chance on Android measured with telemetry frame timing first; the emote row
with the per-seat mute once `ClientMsg::Emote(u8)` exists in agni-net (a
wire bump, so from main); concede in the table menu wired to the U11
affordance. Files: `android/*`, `src/os/android.rs`, `src/table/drawer.rs`,
`src/table/scene.rs` (Android branch). Done when a phone rotates without a
black frame and the 360×800 shot matches the portrait row of §3.4.

### Ordering

```
U1 ─▶ U2 ─▶ U3 ─▶ U4 ─┬▶ U5 (menu · settings · deck)      ─┐
                      ├▶ U6 (gesture · chips · verbs)  ─┤
                      └▶ U7 (viewport · camera · os · web)┴▶ U8 ─▶ U9 ─▶ U10 ─▶ U11 ─▶ U12
```

U5, U6 and U7 own disjoint files: U5 `menu/*`, `settings/*`, `deck/*` and
the net UI functions; U6 `gesture`, `chips`, `interaction`, `highlight`,
`arrows`, `plugin_ui`'s prompt renderers; U7 `viewport`, `app`, `scene`,
`camera`, `layout`, `anim`, `zones`, `sync`, `os`, `web`, and `hud.rs`'s
phone functions only. The two shared points — `hud.rs` (U4 creates it, U7
appends) and `plugin_ui.rs` (U6 touches the prompt renderers, U11 the
narration branch) — are separate functions. Version bumps: U1–U3 patches; U4
the 0.10.0 minor; U5 a second minor if it lands after U4 ships; U7 a minor;
everything else a patch.

## 11. Contradictions between the three designs, resolved

| question | mobile-first | IA-first | continuity-first | decision and reason |
|---|---|---|---|---|
| portrait phones | reference layout; Android unlocked | "turn your phone" card; Android locked | web-only FitWidth; Android locked | portrait is a full class from U7 because a phone browser hits it regardless; the manifest unlocks in U12 after a device run, since the Android pieces cannot be verified headlessly |
| the March | client-side `Marching` set emitting moves in order | the engine's `GroupMove` prompt as a batch | the same | the prompt; a pre-selection would be refused after the first move (`march.rs` opens a max-1 prompt) |
| single click on a card with one offer | select; second tap fires | click fires | tap fires for prompt candidates only | tap always selects; a tap on an Answer-rimmed card during a prompt is the pick because that rim is visible state |
| refusal anchored to the card | an agni ask (card id in `Notice`) | client-side `LastIntent` | card on the resource | `LastIntent`; no agni change and it works on the first release |
| rims | five, Play/March/Activate folded into Act | five, the same fold | the M5 five kept, Hide dotted, Enemy a ring | the M5 five kept: a march is an attack and the palette and tests already exist; the seat-colour affordance rim goes |
| end-turn confirm | hold-to-confirm default on | setting, off | not built | off by default; `RecentDrag` guards the slip |
| auto-pass | on | on | a toggle that drives nothing | on, but only in U8 with `auto::decide` tests and a soak; no setting ships before its automation |
| Enter | default action; pass-through | pass-through | fire the single affordance | Enter = the selected card's default action; pass-through is Shift+Space — one meaning per key |
| Esc at the table | the drawer | the table menu sheet | Settings › game tab | the table menu sheet; config and play are not mixed |
| playmat and seat colour | Settings › look | the lobby's look group | Settings › look | Settings › look, once; a look preference must be changeable at the table |
| partly scripted decks under rules | locked out | allowed with a coverage chip | allowed with a badge | allowed; the vanilla fallback is what the pipeline supports and the chip is the honesty |
| the drop chooser | 3 s timeout | — | two buttons, no timeout | no timeout |
| phase bar | when the presenter carries phases | nine chips, click toggles a stop | deferred | pointer classes only, after U11; phones set stops in Settings |
| harness | U8, parallel | none | U0, first | first (U1); every later milestone's screenshot claim depends on it |
| structured view fields | six agni asks | U2, additive | never (parser) | the parser ships first (U4) and the additive fields are the last milestone (U11), deploying from main |
| 1280×800 class | Desktop | Tablet | Wide | Desktop; the thresholds of §5.1 |
| the deck history | one place | one place | lobby and Settings › decks | one place, the deck box |
| Home | rows, resume card, recent strip | hero tile with legend art | tiles | resume card and three fixed tiles; no hero |
| the tablet hand | drawer under touch | fan | fan | fan; selection replaces hover so a drawer is not needed |
| the minimum board card | 40×56 dp | 56 pt tall | 60 pt tall on phones | 56 pt tall (≈ 40 wide) — the three numbers describe nearly the same card; the 48 dp hit padding is the second line of defence |
| the AI chat | drawer tab | drawer tab | floating window | drawer tab; nothing floats but the strip and the drawer |
| scope of the first release | eight milestones, four implementers, six agni asks, Android orientation, light theme, icons, emotes | seven milestones, one agni milestone | eight, no agni | twelve, one agni milestone at the end, the Android unlock and the wire-bumping asks in a second release |
