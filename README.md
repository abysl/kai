# kai

> An Agni Kai needs an arena.

The 3D card table for [agni](../agni/README.md), in Bevy 0.19. Your
hand fans at the near edge (scrollable however big it gets), hover to preview,
drag onto any zone to play. N-player quadrant table, prismatic foils, tunable
everything. kai is the client half of the constitution: UX, rendering, QR
pairing, platform glue — the engine (log, sessions, transport, importers)
lives in agni, and no card content ships with the app.

## Run

```
direnv allow
dev
```

`dev` and `run` first run `modules-build`, which builds the engine and plugin
wasm into `assets/engine` and `assets/plugins` when they are missing. Without
them the store never gets `modules/riftbound` or `modules/mtg` seeded and
hosting a game table fails with "no module ref … in store"; the modules tab
says which bundle is missing.

Each module name is a collection of versions in the store, not a single
pointer. Seeding appends this build's engine and plugins as versions signed by
the store's own key; a version installed from the mesh sits beside them, signed
by whoever published it. What actually loads is the newest version whose
signature comes from a key you trust at cache level or above — so a bundle
never erases a mesh install, and a newer mesh version still wins over an older
bundle. The modules tab lists every version with its signer. Tables pin
module hashes in genesis, so a joiner that lacks the exact module fetches it
from the mesh; the browser fetches it from a gateway instead, which is why
dev1 and dev2 replicate `modules/*` refs from every desktop that advertises
them. A table hosted from a `dev` build pins locally built modules, so a
browser can join it only once a gateway has pulled that build's blobs.

kai boots to an **empty table** on every platform — "create a table or import
a deck to begin". Nothing is dealt until you host a game table and import a
deck, or (free-form, desktop only) click *deal a sample hand* in the tuning
window, which lays out whatever set your spirit store holds under `refs/hob`.
`AGNI_HAND=all` sizes that sample hand to the whole set, `AGNI_PLAYERS=8 dev`
for a crowded table. A wasm build of the same app is deployed at
[kai.rae.blue](https://kai.rae.blue) (`web-build` + `web-serve` locally).

### Home, lobby and settings

kai opens on **Home**: the title, three fixed tiles (Riftbound, MTG,
free-form) in one row on desktop and tablet, one per row on a phone, a
"return to your table" card while a table is live, and the gear as the
footer. Nothing else — module hashes, node lines and version badges live in
Settings › advanced behind the developer toggle.

Under the tiles a full-width **deck editor** row opens the deck screens
(below). A tile opens that game's **lobby**, which is three decisions and one
verb. The deck card (legend thumbnail, label, *switch decks*, *deck editor*)
opens the **deck box** on *switch decks*: a selection-only sheet with your saved decks as tiles (a tap seats one), the
sideboard between games, and one link, *open the deck editor*. There are no
preconstructed lists to pick from any more — every card is scripted, so a
rules-enforced table takes any legal deck you import or build. The
rules group is a segmented *rules enforced · free table* (enforced by
default for Riftbound, locked for MTG and free-form, fixed while a session
is live) with the mode chips (by seats · 1v1 · FFA3 · FFA4 · 2v2 · house
rules…) under it. The opponent group is three segments: **AI** (the AI's
deck card, its first battlefield, random · fast · thinking), **friends** (the seat
rows with swatch, name, deck and tags, invite with an inline QR, add AI) and
**join** (the open tables in the mesh or paste a ticket). The primary verb —
*play vs AI*, *host table*, *join*, *go to the table* — sits in the header on
desktop and tablet and in a sticky footer on phones, disabled with its
reason ("choose a battlefield first") when it cannot run. ‹ goes home, Esc
and the platform back key walk the same ladder everywhere (sheet → table
menu → home).

**Settings** is a sheet on the right (full screen on phones) with four tabs
that save as you go and close on ×, Esc or a tap outside: **play**
(auto-pass, ask me anyway, order my triggers, assign combat damage, the
end-turn confirm, fast animations, hand on the left, UI scale, the
stop-and-wait phases, show hints again), **look** (playmat, seat colour,
camera preset, zoom, hover preview size, foil, theme system · dark · light,
the colour-blind palette), **you** (your identity QR, ticket and node id,
add a peer, the peers list) and **advanced** (the developer toggle; behind it
the tuning sliders and sample hand, a tuning table, the full-set art
download, the AI model, the module rows, the node line and telemetry, and
always the footer "v… · wire … · modules/riftbound v…"). The light theme
reaches Home, the lobby and every sheet; the table HUD stays dark on its
felt.

### The table view

The camera is fixed and tilted the way Hearthstone and MTG Arena frame a
match: your side nearest and largest, the opponent's receding at the top of
the screen, the whole board in view, your hand hanging off the bottom edge
and rising to meet you when you hover it. The frame is solved for the window
(`scene::framing_for`) — on desktop and tablet the far edge meets the top
and your outer row meets the bottom; on phones the width is fitted in
portrait, the far outer band is cropped in landscape, and a board card is
never smaller than 56 pt. Two-finger pan and pinch zoom work on touch;
Settings › look has the *arena* / *top-down* presets.

The bug icon beside ≡ opens the [GitHub issue form](https://github.com/abysl/kai/issues/new)
directly during play. It leaves the match running and does not submit anything
automatically.

If a rules bug blocks a match, open ≡ → **disable rules enforcement** and
confirm. Any seated player can do this, including during a prompt, setup or a
reported win. Cards, scores and counters stay on the table; rules and automatic
bookkeeping stay off until the next game. All players receive the logged change.

Open ≡ → **manual controls** to edit scores and all declared counters; browse
every public zone and your private zones; move cards to any zone or deck position;
ready/exhaust, mark, reveal or turn cards face down; draw, privately look/search,
finish looking and shuffle either deck; set the turn and battlefield control; or
remove your tokens. **Create tokens** opens the existing token drawer. Resolve
payments, damage, draws, scoring and victory yourselves. Shuffling ends all looks
at that deck. The panel starts with its operation groups open, uses roster names
when available, and keeps card editing tied to the selected card. Other players'
private hands and decks remain private.

Everything drawn over the felt sits in a slot of one tested layout per
viewport class (`hud::layout`, [ux.md §3](wiki/design/ux.md)): the ≡ table
menu, the **turn plate** ("your action · turn 6 · action · enforced", the
acting seat's colour, a pulse while it is you), the phase bar with
click-to-stop dots under it on pointer classes, the **strip** at the top
centre (your prompt with its count chip and numbered chips, the response
window, waiting with its seconds, the opening roll, the amber refusal row —
never a card option), the **chain** panel top-right (newest first, art, a
drop target; on phones a one-line ribbon of thumbnails that opens a sheet),
the seat plates under it (swatch and name, points as pips, victory, XP, hand
and deck counts, runes ready/total), the **history rail** on the left (one
tile per board event, hover for the sentence), the **inspector** bottom-left
(the hovered, selected or pinned card), the **primary** button
bottom-right whose label is the one thing to do (*roll*, *keep*, *pass*,
*resolve*, *end turn*, *done*, *move 2*, *waiting*) with its hotkey glyph, the
*log · chat · tok* drawer tabs under it, and the hand. A card is selected by
a tap, acted on by a second tap, Enter or its numbered chips (*play*, *play ›
zone*, *move › zone*, *hide*, *reveal*, *inspect*, with the reason when one
is disabled), dragged only past the slop and only when a rim says it may
move, and dropped on a lit zone (a release on the felt snaps to the nearest
one; play or hide gets a two-button chooser). Right-click or a long press
pins a card in the inspector; Tab cycles the legal cards; Esc walks chooser
→ drag → prompt cancel → selection. Five legality rims pair a hue with a
stroke — Play solid, March chevron, Activate corner tag, React dashed,
Answer pulsing — and the colour-blind palette keeps them apart. Three coach
marks and an idle hint teach the grammar once each; `?` opens the help
sheet with the verb table, the hotkeys and the rim legend.

**Automation** (Settings › play): with auto-pass on, a window in which you
have no move passes itself after 600 ms — the same delay as a pass-through,
so timing leaks nothing; Ctrl-click the primary to hold this phase,
Ctrl+Shift-click to hold every phase, Shift+Space to pass through until
something new happens, and the stop list to wait on a phase. Forced picks
(exactly the remaining cards) answer themselves unless *ask me anyway* is
on; the end-turn confirm, off by default, needs a second press within three
seconds on the button, W or the plugin's own Space.

On phones the hand is a **drawer**: tucked to a 56–72 pt band at the bottom,
raised by a tap or a prompt that needs a hand card, scrolled by a swipe,
tucked again by dragging a card out or tapping the felt; the primary and
the rune chips ride its top edge. The far seat's counts live in the
opponent strip (portrait) or the top bar (landscape). Safe areas come from
the Android insets and the web `env(safe-area-inset-*)` shim, and a field
under the keyboard scrolls into view.

### Playmats

Under Settings › **look** → *playmat* the felt can be swapped for artwork.
Four official Riftbound pieces are listed to begin with — a Shurima desert, a
Shadow Isles spire, Akali, and the Vi key art from Riot's site — and any
battlefield card in your seated deck is offered too, since battlefield art is
landscape and already fetched. Paste any image link into **add from link** to
keep a custom one; it is fetched once, stored in the spirit store under the
`playmats` journal like card art, listed in `playmats.json` beside your tuning,
and comes back on the next launch without a fetch. Nothing is bundled or
committed: an unfetched mat says so until it lands. The art covers each mat
with a centred crop, and the far seat's mat is turned to face its player, the
way a real playmat sits. On the web the page asks its gateway for the mat and
the gateway answers from its store, from a peer over the mesh, or by fetching
the link once.

### Assets over the mesh

Every image kai shows — card art, card backs, playmats — is a blob in the
spirit store, and every node advertises an `assets` index of what it holds.
Before fetching any URL the art worker asks the mesh: it pulls the indexes of
the peers it knows, and if any of them lists the asset it pulls that one blob
from that peer over iroh. Only when no known node holds it does the URL get
fetched, once, after which this node's index advertises it for everyone else.
The gateways do the same on behalf of browsers, so a mat one player added on
their desktop is served to the whole table without touching the CDN again.

### Status effects

Beside might and damage, a unit can be marked **temporary**, **buffed** or
**empowered**. They are ordinary card counters the Riftbound plugin declares
with a range of zero to one, so the popup draws them as checkboxes and the
badge shows a 20 pt chip — a glyph or a frame rather than a word: hourglass
for temporary, a gold frame for empowered, sword and shield for attacker
and defender, the tilt plus a dim for exhausted — stacked to the right and
truncated to "+2" beyond three. Any plugin gets the same treatment for a
zero-to-one counter.

### Card backs

Hidden cards and opponents' hands wear the real back of the game being
played: the MTG back from Scryfall and the Riftbound back from Piltover
Archive, named by `CARD_BACK_URL` in each game crate. Like every other card
image the back is fetched on first use by the art worker into your spirit
store (journal `card-backs`), never bundled, so a fresh install shows tinted
backs for a moment and the browser, which has no art worker, keeps them.
Riftbound's rune and legend backs are not separately available, so every
Riftbound card wears the standard back.

### The deck editor

The **deck editor** is its own pair of screens, reachable from Home, from the
lobby's deck card and from the deck box, and never mixed into deck selection.
The first screen is the **library**: your saved decks (edit · rename · share
· delete), the draft in progress (*continue editing* · *start fresh*) and
*new deck*. The second screen is the **editor** itself: the list pane and the
catalog browser, the legality meter, rename, save, **import** and **share**
in the footer, and *seat this deck*, which saves the list and returns you to
the Riftbound lobby with it seated. *import* reads your clipboard on its own
— copy a Riftbound or MTG text list, a Piltover Archive deck code, or a
riftdecks.com / piltoverarchive.com / play.riftatlas.com link anywhere, press
*import*, and the resolved deck offers *load into the editor*, *save to your
decks* and *share*, naming the cards that were not found; a `.txt`/`.md`
dropped on the desktop window or a Ctrl/Cmd+V with nothing focused does the
same. Share offers the text list, the deck code, the code list, the Piltover
link and a QR. Saved decks appear in the lobby's deck box to seat, and are
stored in your spirit store (desktop, Android) or the browser's localStorage
(web).

### Choosing a battlefield

A Riftbound deck carries three battlefields and a game uses as many as the
table's options say — one per player by default, never fewer than two, so a
duel plays on two and a three- or four-seat game on three. The Riftbound
lobby's **table options** are "by seat count" until the host picks a
sanctioned mode or house rules (first to N points, N battlefields); once the
table opens they are fixed and every seat reads them back from the game
itself. The choice happens **at the table**: your legend, champion, runes
and main deck are dealt as soon as you sit, so you see who you are up against,
then a tray of your battlefields ("choose your battlefield") sits in the
banner slot until you tap one, which places only that card onto the shared
band (a table with more battlefields than seats has the seats after the
first player place the extra ones, the chosen card first and the deck's order
after). The first-player roll is greyed out with *choose your battlefield
first* until your battlefield is down, so the order is always see the
opponent → choose → roll. Starting a new table after a game re-asks, since
the pick is per game. A deck with a single battlefield never asks; the AI
seat takes the first of its deck.

### Turns and showdowns

On a Riftbound table a strip at the top of the screen carries the game's turn
structure. It is not kai's: the plugin computes it and describes it, kai only
draws what it is told. A game opens with a dice roll: press **roll**, and once
every seat has rolled the dice are revealed together — a commit-and-reveal
roll every replica verifies, so nobody, host included, can steer it; a tie
rolls again. The winner chooses who goes first. A turn is a *beginning phase* (ready your cards, score holds, channel two runes,
draw one — you do those yourself at the table) advanced with **end beginning
phase**, then the *main phase*, ended with **end turn**; `Space` does either.
In the main phase the turn player can open a **showdown** at a contested
battlefield; the attacker has focus, the focus holder can **pass focus** (`W`)
or simply play, which hands focus on, and the showdown closes once everyone
has passed in a row or the attacker ends it (`Q`). The plugin refuses
out-of-turn actions — the host reports "the game action was refused" — and
never restricts card movement. Solo tables have no plugin and no strip.

That is the free table. The lobby's **rules enforced** segment (the default) opens the
table with the referee on instead: the plugin refuses illegal plays and runs
the turn, and the flag rides the table's genesis (`rules_enforced`, written
only when on) so both seats and every reconnect read the same mode and the
dice roll opens in it; the roll winner can still switch the table the other
way at the start. Only the pool decks are scripted, so an enforced lobby
pins the deck column to the scripted pool: every `rules/pool/*.md` whose
`Scripted:` line says `complete` (`src/deck/pool.rs` reads the six files'
deck blocks through the importer's own text-list parser and resolver, so a
pool deck and the same list pasted into the import panel are one deck).
Each pick is your saved copy of that deck if history holds one (a held row
with the same card identity, or one whose label is the pool file's H1 such
as `Lillia (house)`), otherwise the deck built from the pool file: thirty-nine
in the main deck, the chosen champion in its zone, 12 runes, 3 battlefields
and the legend. The host defaults to the first pool deck and stays in the
lobby when the table opens; a joiner is taken to the table as it lands and
takes a deck the host has not dealt — its own legend's deck again when it
reconnects to a seat already dealt, a deck with another legend than the
host's, or the second pool deck when nothing is on the table yet — and its
deck deals itself once it lands unless its legend is already on the table.
Switching decks after the deal takes a new game. `kai-cli soak --deck-a
pool:<slug-prefix>` names the same decks; a prefix two files share
(`pool:lillia`) takes the house list.

### Name and icon

The logo is one SVG, [`assets/icon/kai.svg`](assets/icon/kai.svg): a flame
with a lightning strike through it, the agni kai. Every other form is rendered
from it by `icons` (the embedded `kai-256.png` and the android `mipmap-*`
launcher PNGs, legacy plus adaptive foreground/background layers rendered from
the SVG's `backdrop` and `art` groups — rerun it after editing the SVG and
commit the output).

The window is titled `kai` and carries `kai` as its wayland app id / X11
class. On wayland the compositor never takes an icon from the window; GNOME
looks up a `kai.desktop` matching that app id and shows "Unknown" with a blank
icon when none exists. The nix package installs [`linux/kai.desktop`](linux/kai.desktop)
plus hicolor icons at every size; for the `dev` flow run `desktop-entry` once,
which installs the same into `~/.local/share` with `Exec` pointing at
`target/debug/kai` (matching is what matters there; launching from the app
grid only works for a `run` build, since `dev` binaries need the shell's
`LD_LIBRARY_PATH`). X11 and windows additionally get the icon straight from
the binary (`src/os/icon.rs`). The web build links the SVG as its favicon.

## Headless: kai-cli

`kai-cli` is a seat at a table with no window: it starts its own spirit node,
joins a host over the mesh exactly as the desktop client does — the pinned
engine and plugin fold every entry, faces arrive on the private channel — and
talks in lines instead of pixels. Build it with the `headless` feature and
give it a store the desktop kai is not holding:

```
cargo build --bin kai-cli --features "headless fast-compile"
SPIRIT_STORE=~/.spirit/cli target/debug/kai-cli --join <host node id> \
    --deck my-deck.txt --battlefield 2 --commands /tmp/kai-in --log /tmp/kai-out
```

`--commands` reads a file or FIFO as it grows (stdin when omitted) and
`--log` appends every state change and reply (stdout when omitted), so an
agent — or Claude in this repo's session — can drive a seat by appending
lines to one file and reading the other. `--deck` seats and deals a decklist
as soon as the table is joined.

Commands, one per line: `tables` (open tables gossiped in the mesh), `join
<host>`, `state`, `do <n>` (press the n-th action the plugin offers — turn,
showdown, roll; reveals are sent automatically), `move <card> <zone> [index]`
(zones by name or label, `bf2` for battlefield 2), `play <card>` (to the
chain), `draw [rune]`, `exhaust <card>`, `trash <card>`, `recycle <card>`,
`hide <card> [zone]` (a face-down play), `reveal <card>`, `spawn <token|name>
<zone> [might]`, `playmat <link|card:name|felt>`, `counter <card|seat>
<name> <delta>`, `deck <file>`, `battlefield <n>`, `deal`, `quit`.

`kai-cli --brain random` (or `auto`, the same random picks with the desktop
auto-pilot's forced passes sent first) plays the seat with no model and no
network beyond the table: a uniformly random pick among the legal
affordances and the legal list's moves, one intent at a time, the next only
once the last has folded or been refused. It is the desktop's free opponent.

`kai-cli --ai` hands the seat to a language model: after every fold where the
turn strip offers this seat an action (its turn, its focus in a showdown, a
roll), the harness sends the model the full table state, the rules text and
cost of every card it can see, its own notes from earlier turns and the
numbered actions, and the model acts through tools (`act`, `move`, `play`,
`hide`, `reveal`, `recycle`, `trash`, `exhaust`, `counter`, `spawn`,
`card_text`, `note`, `done`) until it calls `done`. Each tool call runs as a
kai-cli command and the model sees the resulting state, refusals included.
The request is laid out for prompt caching: a stable prefix — the rules, the
glossary for the legal list and the arrows, the tool set for the mode (the
free-table tools are withheld under rules enforced, the deck tools once the
deck is dealt), and the reference text of every card seen so far this game
in first-seen order, so a new card only appends — then the volatile tail:
the model's notes, a one-line recap of its previous decision, the table
state with face-down runs folded into a count and at most twelve notice
lines, the legal list and the arrows. Nothing carries from one decision to
the next except the notes and that recap. Within a decision only the newest
tool result keeps the full state; earlier ones are cut down to their
refusals, and the opening state is stubbed once a newer one arrives. A
tool result ending in the "this decision is over" line (the seat has
nothing left to act on: it passed, ended its turn, or handed priority over)
ends the decision without another round. A mid-game request is about 29 KB,
7–8k tokens on DeepSeek, of which 26 KB is the stable prefix laid out for
the provider's cache; the soak trace
prints the per-decision breakdown as `ai request:` and, after each decision,
the prompt tokens with the provider's cache-hit count beside them
(`prompt_cache_hit_tokens` on DeepSeek, `prompt_tokens_details.cached_tokens`
elsewhere), so a live run says whether the prefix was cached. A decision
that takes several rounds re-sends the request each round, with the earlier
tool results cut down, so a three-round decision costs about three times
one request. What the test pins is the one-round request: it replays a long
random game and measures the request either seat would send at every step,
the seat about to act with its legal list included, against the budget
(`one_round_of_a_decision_for_either_seat_of_a_long_random_game_stays_under_the_budget_and_flat`).
The model runs on NanoGPT (`--model` picks it; the default is DeepSeek V4
Flash, which answered a scripted turn in under five seconds with correct tool
calls, against 32 seconds for GLM 5.3 Flash's reasoning; Gemini 2.5 Flash
Lite is faster still). Set `NANOGPT_API_KEY` in the launching environment to use
an AI model; no credential is bundled. Random opponents need no key.
Card texts come from the Riftbound set ingested into the store
named by `--catalog` (the host's store when the desktop launches the seat);
notes persist in the seat's store as `ai-notes.md`.

From the desktop and from Android, the Riftbound lobby's AI segment seats
that same brain at your own table: pick a brain (random is free and needs no
key; fast and thinking are the two NanoGPT presets) and a deck (one of your
saved decks, your seated deck, or let the AI choose) and press "play vs AI".
The seat runs **in-process** (`kai::ai::local`): a thread drives the same
`Driver` kai-cli runs, joined to your host session through a pair of
channels the way a network peer is joined through iroh — the host sees a
peer connection, seats it with `join_as`, sends its Welcome, the entries and
the faces it is owed, and folds its intents — so nothing is spawned and no
sidecar binary is needed. Its log, chat and notes live under the config
directory's `ai/` folder (`seat.log`, `chat.log`, `ai-notes.md`). `C` at
the table (or the drawer's chat tab under the primary button) opens the
chat: a place to ask for a matchup or tell it which deck to play (messages
land in the seat's prompt and it answers with `reply`), a model switcher
that takes effect on the seat's next decision, and a stop button. With a
model and "let the AI pick" the seat picks its own deck with `load_deck` (a
saved deck's label or a Piltover Archive link), chooses a battlefield and
deals; the free brain takes the pool deck that is not yours. The state
print lists the turn strip, numbered actions, every zone with `#id name` per
card (`?` for a face this seat may not see), and any counter off its start.
Closing the table, leaving it or losing the host stops the seat.

### Self-play soak: kai-cli soak

`kai-cli soak` plays complete two-seat enforced games with no window, no
node and no network: the host session and both seats' replicas live in one
process, the riftbound plugin runs under wasmi (from `AGNI_RIFTBOUND_WASM`,
`assets/plugins/riftbound.wasm`, or the store's `modules/riftbound`), and the
engine folds natively unless `--engine wasm` asks for the bundled one. Deck A
and deck B (`--deck-a lillia --deck-b irelia`: a saved deck's label, a
`pool:lillia` / `pool:irelia` deck built from the rules pool file, a decklist
file or a link) swap seats every game and the first player alternates every
two, so four games put each deck in each seat going first once from each.
Each deck has a brain: `random` (`--brain-a`, the default: a uniformly random
pick among the legal affordances and the legal list's moves, prompts answered
with a random valid option, no network), `auto` (the pilot's quiet presses: `random`
with kai's `table::auto::offer` sending the forced passes, end turns and
picks first — it plays the same game as `random` on the same seed and faults the
game if auto-pass ever fires while the seat had another option; the trace
marks its presses `auto-sends`) or `nanogpt` (the real AI seat with
`--model`). Every game is seeded from `--seed` and its number and records
one JSON line in `--out` (default `soak.jsonl`): the base seed, the game's
own derived seed, the turn cap, the engine and the plugin origin, decks per
seat, who went first, winner, points, turns, entries, wall time and the
ending — a winner, the `--turn-cap` (40), an engine fault, or a stuck prompt
(no seat may act, or `MAX_REFUSALS` offered moves refused in a row). The
summary prints wins per deck and by turn order, averages, and every
non-winner ending with its game number and the exact replay flags; a fault,
a stuck prompt or a game that could not be set up exits 1 and says so on
the last line. `--seed S --turn-cap C --engine E --start <n> --games 1`
with the same decks replays a game move for move, `--trace <file|->` writes
every pick and refusal, and `--jobs` runs games in parallel (one per core by
default; the trace is then flushed per game). Twenty random games run in
about a minute on a workstation.

## Web

> Send a friend a link.

The wasm build at [kai.rae.blue](https://kai.rae.blue) is a real iroh peer.
On first load it generates a node key and persists it in localStorage (a
reload keeps your identity), binds one endpoint, and seeds its mesh from the
dev1/dev2 gateway node ids baked into `src/net/gateway.rs` — those gateways are
real spirit nodes, and gossip introduces everything else from there: peers,
open tables, the lot. Opening the page is the whole onboarding: no QR, no
pasting, tables hosted anywhere in the mesh appear in the multiplayer panel
and joining is one click. The identity and peers panels are live too — the
browser shows its own identity QR for others to scan, and the paste box adds
peers by ticket like the desktop.

Browser transport is **relay-only** over WebSockets: no direct paths exist
from a browser and none are planned, so relay latency is the price of the
platform. It rides n0's public relays today; a self-hosted iroh-relay is a
later infra step. The browser hosts too — same `HostSession`, same
protocol, folding with `engine.wasm` — with one condition the opponent panel
states: keep the tab in front. Bevy's frame loop is `requestAnimationFrame`,
which a hidden tab does not get, so a hidden host tab pauses the table for
everyone at it; the network keeps running underneath and the intents queue
until the tab is back, but a tab hidden for many minutes (or a phone that
switched apps) drops its joiners' connections and they reconnect into their
seats. While hosting, the page holds a screen wake lock where the browser
offers one, so a phone's screen timeout does not hide the tab; closing the
tab withdraws the table on `pagehide`. The *host table* button stays off
until `engine.wasm` has loaded (a browser host always pins its engine, and
the bytes are served from its in-memory blob store so any joiner can fetch
them), and a rules-enforced Riftbound table also waits for the gateway-held
`modules/riftbound` plugin, which the browser fetches from the gateway as
soon as the module list arrives — the button's reason line says which it is
waiting on. The design doc's browser section records what was read to reach
that answer. The status line reads
`meshed via relay — N peers, M tables · bridged via dev1 — 193 card faces reachable`.

What travels over which channel:

- **Game state** — `HostMsg`/`ClientMsg` frames over `spirit-table/1` on the
  browser's own endpoint, identical to desktop and android. Card faces reach
  a joiner through the log's reveals and its private `Faces` messages.
- **Card art for imported decks** — the read-only HTTP gateway bridge below,
  unchanged: fast and immutable-cached. The browser does not replicate card
  stores over iroh; its in-memory blob store exists only to keep the router
  shape identical. Per-card on-the-fly fetching from Riftcodex/Scryfall is
  native+android only; the browser stays gateway-honest.
- **No bundled art** — the bundle ships no card images and no manifest. A
  page that reaches no gateway plays named placeholder cards; art appears
  only once a bridge answers. Fresh public visitors see placeholders, by
  design.

Building wasm needs `RUSTFLAGS='--cfg getrandom_backend="wasm_js"'` and an
unwrapped clang for ring's C sources; `web-build`/`web-clippy` in the devenv
and the `kai-web`/`kai-web-deps`/`kai-web-clippy` derivations in the crane
flake carry both. `web-build` + `web-serve` runs the bundle locally. The
canvas sets `fit_canvas_to_parent`, so it fills the browser window and tracks
resizes.

### The gateway bridge

The art path. On boot the wasm build talks to the read-only HTTP gateways on
the dev VMs (`spirit-node --gateway` behind `tailscale serve`, see spirit's
`wiki/design/gateway.md` and infra's `docs/dev-vms.md`). `src/net/gateway.rs`
holds the one const table that decides where it looks:

- Both gateway base URLs are hardcoded (`dev1`/`dev2` on the tailnet's ts.net
  domain). The client picks one at random for load balancing, health-checks
  `/gateway/status` with a short timeout, falls over to the other, and only
  then falls back to named placeholders — an artless card still plays.
- Each entry also carries the gateway's spirit `node_id`. That id now does
  double duty: it is checked against `/gateway/status` (mismatch logged, not
  fatal) and it is the browser's iroh mesh bootstrap seed — the wasm peer
  dials it by node id over the relay at startup. The id the gateway reports
  live in `/gateway/status` is seeded as well, so a stale pin self-heals for
  any browser that can reach the gateway over HTTP; refresh the pins from
  that endpoint (`curl https://devN.dragon-pierce.ts.net/gateway/status`)
  whenever the gateway services regenerate their identities.

When bridged, the app resolves art out of the gateway's current refs —
manifest first, then each card's art blob lazily by hash (immutable-cached,
so repeat cards are free). `tailscale serve` is tailnet-only by design:
browsers on Rae's tailnet devices reach the gateways for art, anyone else
quietly falls back to named placeholders — but the iroh mesh itself works
from anywhere the public relays reach.

### Copy and paste

> Every box that takes pasted text has a **paste** button next to it.

Neither the browser nor android hands egui a clipboard on its own. On wasm,
winit calls `preventDefault()` on every canvas keydown, which cancels the
browser's default paste — so the `paste` event bevy_egui listens for never
fires and Ctrl/Cmd+V lands nowhere. On android there is no clipboard code in
bevy_egui at all: `EguiClipboard` is compiled out for the platform, so both
copy and paste were silent no-ops.

`src/os/clipboard.rs` is the one way in and out. The **paste** button beside the
deck-import box, the deck-link box, the identity ticket box and the telemetry
token box asks the platform directly — `navigator.clipboard.readText()` in
the browser, `ClipboardManager` over JNI on android, `arboard` on desktop —
and says why when it is refused (an insecure origin, or a browser that will
not grant the permission) rather than doing nothing. The **copy** buttons on
the identity ticket, node ids, peer tickets and the resolved deck code go out
the same way.

The glue in [`web/index.html`](web/index.html) also un-blocks the real
shortcut: a capture-phase `keydown` listener stops Ctrl/Cmd+V, +C and +X
before winit can cancel them, so the browser fires `paste`/`copy`/`cut` and
bevy_egui's own document listeners feed egui as they were always meant to.
kai.rae.blue is HTTPS, so the secure-context requirement is met there; a
plain-HTTP dev server will get the honest "only over https" message.

## Multiplayer

> One table, many hands.

Desktop and android builds play together over iroh, and pairing is one QR per
device: every client shows its identity QR under Settings › **you** from the
moment it starts. Scan a device's QR (android) or paste its identity ticket
(desktop) once and it joins your mesh; gossip introduces everyone to everyone,
so one phone scanning each desktop at the table meshes the whole room. Click
**host table** and your table appears in every meshed client's lobby under
the opponent group — they tap **join** and are seated; nobody types a ticket. The host
is the sequencer, not the state owner: it seats players in join order,
executes each seat's deck deal on request, and assigns every action its place
in a shared append-only log; each client — the host included — derives the table
purely by folding that log, so every seat provably renders the same game.
Your hand's faces travel only to you — everyone else sees card backs sized to
your real hand count until a reveal enters the log. Drops are requests routed
to the host for ordering, and if the host vanishes the session freezes (until
sequencer failover lands) — reconnecting back into your own seat is below.
Protocol, seat agreement and privacy rules live in
[`wiki/design/multiplayer.md`](wiki/design/multiplayer.md) and
[`wiki/design/deterministic-log.md`](wiki/design/deterministic-log.md); the
web build at kai.rae.blue joins the same tables from a browser tab (see the
Web section — bootstrap is automatic, and hosting works with the tab in
front).

### Default peers and identity

Desktop and android seed **dev1** and **dev2** — the fleet's always-on
spirit gateways — into the mesh at node start, by node id from
`src/net/defaults.rs`, the same list the web build probes over HTTPS. iroh's
discovery finds them through the relay, so a fresh install meshes and pulls
the card sets without scanning anyone; the status line reads "serving N
blobs, default peers dev1, dev2" and the peer list names them. Override with
`KAI_DEFAULT_PEERS=<id-or-ticket>,…` (`KAI_DEFAULT_PEERS=none` for a clean
mesh) on desktop.

Your identity is the node key at `<store>/identity/key`: `~/.spirit/store` on
desktop (or `SPIRIT_STORE`), the app's private `files/spirit-store` on
android. Both survive restarts and upgrades; android loses it only on
uninstall. The web build keeps no key and gets a fresh identity every load,
which is fine for a browser tab.

### Reconnecting

> A dropped phone gets its own seat back, not a new one.

A sleeping phone, a suspended app or a host that walked out of range all end
the same way: the link closes, the client's status reads *session over* and
the table freezes. The multiplayer panel now answers that instead of leaving
the app to be restarted. The `Ended` panel offers **reconnect to `<host>`**,
which re-dials the last host the client joined and rejoins its table, and
**back to a solo table**, which drops the session and returns to the table
list. If that host is no longer advertising a table in the mesh the panel says
so under the button rather than letting the dial time out unexplained; the
button stays live because a gossip advert can lag a host that is genuinely
back. A join that failed before it ever seated also leaves the reconnect
button in the solo panel next to the discovered-table list.

The reconnect is an ordinary join over the same code path, so a returning
client still fetches, hashes and verifies the genesis-pinned engine and plugin
in `modules::prepare_join` before folding a single entry — a reconnect can no
more skip a pin check than a first join can.

The host recognises the returning player by the node identity of the QUIC
connection, which it recorded when that seat first joined, and hands back
**the same seat**: no new `Join` entry, no fresh hand, the roster row flips
from *(gone)* back to connected, and the seat's own hidden faces are re-sent so
its hand is readable again. Only that node can reclaim that seat — the
identity is the authenticated one from the iroh handshake, not something the
client asserts. Cards left on the table stay exactly where they were.

If the *host's* own table dies — the endpoint stops, the advert is withdrawn —
kai keeps the `HostSession` rather than throwing it away, and the panel offers
**re-host this table**: the same log, the same seats, the same card faces, and
every player reconnects into the seat they had. Closing the table on purpose
still discards it; that is the difference between losing a table and ending
one.

### Game tables

> Pick a game, and the table grows its zones.

Beside *host table* sit three choices: **free-form**, **MTG**, **Riftbound**.
Free-form is the bare quadrant table — no zones, no plugin, nothing dealt.
The other two open a table whose anatomy is declared by that game's plugin:

- **Riftbound** — per-seat hand, main deck, rune deck, legend, champion,
  trash and sideboard, plus one shared battlefield per player up to three;
  opening hand of four.
- **MTG** — per-seat hand, library, graveyard, exile, battlefield and
  command; opening hand of seven, commander seated in the command zone.

Each is a hardened wasm module shipped as a runtime asset
(`assets/plugins/riftbound.wasm`, `assets/plugins/mtg.wasm`), seeded into the
local spirit store as a version of the `modules/riftbound` / `modules/mtg`
collection on first run, and pinned by hash into the table's genesis. Choosing the game pre-selects its
module automatically (an explicit pick in the modules tab still wins); the
zone table is read from the plugin's own manifest, falling back to the
compiled table, and the status line says which. Joiners that lack the module
fetch it from the mesh by its pinned hash before folding — asking the host
first, then the rest of the mesh, and giving up with "pinned module … not
obtainable" after a minute — the browser via the gateway — so every seat runs
the same bytes. Joiners need no selector at all: genesis pins the game, and
their deck-import panel follows the table they joined.

Playing end to end:

1. host: pick **Riftbound** or **MTG** on the home screen and **host table**
   from the lobby
2. everyone: the lobby's deck box (**switch decks**) — pick a deck you saved;
   to import one (a text list, a PA deck code or a deck link on the
   clipboard) open the **deck editor**, press *import*, save it, then pick it
   here; the deck is staged and dealt when the table opens, and Riftbound
   asks for your battlefield at the table before the roll
3. the host deals each requesting seat. Riftbound: legend and champion
   face-up, 12 runes and the main deck face-down (shuffled by the dealer,
   deterministically seeded), the one battlefield you chose onto the shared
   band, sideboard privately to you. MTG: commander face-up in the command
   zone, the 60-card library face-down and shuffled, seven drawn to hand.

From there it is a physical table: drag cards between any zones (`D` draws
from a hovered deck, `T` sends to trash/graveyard, `E` or double-click
exhausts/readies, a hover or tap shows the card's chips (recycle a rune, trash
a card, play, reveal) and right-click pins the inspector, `H` plays a hand
card face down and `R` reveals it, `K` opens the token drawer, `P` plays to
the chain), faces reveal automatically when a hidden card enters a public
zone, your hand and decks stay yours. The Riftbound plugin runs the turn
structure, the opening roll, start-of-turn choreography (awaken, hold, channel,
draw), rune payment for plays, showdowns and conquer scoring; everything it
does not know yet (combat damage, card text) stays free-form on the table.

### Card art arrives per card

Deck import resolves names through your local spirit store first and the
game's API second — Riftcodex for Riftbound, Scryfall for MTG. Art no longer
needs a full-set download: any **visible** card without art — a seated deck,
a card you draw, or an opponent's reveal landing in the log — queues a
single-card fetch by game id, lands the image content-addressed in your
store, merges it into that game's manifest and refreshes the face in place.
The queue dedupes in flight, throttles per source, retries twice and then
gives up quietly: a card with no reachable art keeps playing as its name
painted on a tinted placeholder, with no error spam. The status line shows
`fetching art… N left` only while work is outstanding. The full-set download
(**download full riftbound set**) remains as the bulk path. Native and
android fetch directly; the browser rides the gateway bridge instead.

## Android

> Scan a QR, own the store.

The same table ships as an APK from [`android/`](android/), and the phone is a
genuine spirit peer: it runs the same in-process node as the desktop, with its
own identity QR. Scan any peer's identity QR and the mesh replicates that
peer's card sets — manifest and every card JPEG over iroh into app-private
storage, hash-verifying every byte — and the freshly synced store then answers
art lookups for whatever is on the table.

```
cd android
direnv allow            # android SDK/NDK, gradle, cargo-ndk, rust
android-build           # cargo ndk + strip + gradle assembleDebug
adb install app/build/outputs/apk/debug/app-debug.apk
```

`android-native` compiles kai's lib as `libkai.so` for arm64-v8a against the
API 28 sysroot, strips it, and removes the stray per-crate cdylibs cargo-ndk
copies alongside it (the JNI library is fully static). `android-build` runs it
and assembles a debug APK; `android-release` runs it and assembles the signed,
versioned one CI publishes. The
Java side is one `GameActivity` subclass that launches the zxing scan intent
and hands the ticket string back to Rust over JNI, and reads and writes the
system `ClipboardManager` the same way — `requestClipboardPaste` answers on
the UI thread through `nativeClipboard`, and `setClipboardText` puts a ticket
or a deck code on the clipboard.

1. desktop: run kai — open Settings › **you** from the gear; it shows the QR immediately
2. phone: open kai → **scan identity QR** → point at the desktop window
3. import a deck and watch the named placeholders become real cards as the
   mesh syncs whatever sets that peer imported

Headless machines have no window to show, so they serve from the CLI instead:
`cargo run -p spirit-node -- serve` prints the same identity QR in the
terminal.

### Typing on android

> The soft keyboard appeared but the characters went nowhere. They do now.

`GameActivity` collects soft-keyboard text through GameTextInput's
`InputConnection` and hands it to its listener as a whole editing buffer.
winit 0.30's android backend matches only `InputEvent::MotionEvent` and
`InputEvent::KeyEvent` and drops `InputEvent::TextEvent` in its catch-all arm,
so it never emits `WindowEvent::Ime` — and bevy_winit's and bevy_egui's IME
translation, which is complete and enabled by default, sits waiting for
messages that never arrive. Hardware keyboards work because those really are
`KeyEvent`s; a soft keyboard commits through the `InputConnection` instead.

One half of the chain already worked: bevy_egui's `process_ime_system` calls
`Window::set_ime_allowed` whenever egui reports a focused text field, and on
android winit routes that to `AndroidApp::show_soft_input`. So the keyboard
already popped up on focus and hid on blur, and still does — that is not
kai's code and is left alone.

[`src/os/ime.rs`](src/os/ime.rs) supplies the missing half, over JNI in the same
shape as the scanner and clipboard bridges. `MainActivity` overrides
`GameActivity.stateChanged` — the `gametextinput.Listener` callback — and
forwards each new buffer to `nativeTextInput`; `onEditorAction` forwards the
IME's **done** key to `nativeEditorAction`. Rust keeps a mirror of that buffer
and diffs each arrival by characters (common prefix, common suffix), turning
it into deletes and inserts, which `drain_text` writes as
`bevy_input::keyboard::KeyboardInput` messages on the primary window ahead of
`EguiPreUpdateSet::ProcessInput`: `Key::Character` for inserted text (egui
turns that into `Event::Text`), `Key::Backspace` per deleted character,
`Key::Enter` for the editor action. Nothing reaches into bevy_egui's
internals; every message is a public bevy type, so the desktop and web paths
are untouched.

The buffer is seeded with a run of spaces when a field takes focus and reset
when it loses focus. Without that padding an IME asked to delete from an empty
`InputConnection` changes nothing, reports nothing, and backspace silently
stops working on text the field already held — a pasted deck code, for
instance. The pad gives every backspace something to consume, and it is
refilled if it runs low. `MainActivity` also sets `TYPE_TEXT_FLAG_NO_SUGGESTIONS`
and `IME_FLAG_NO_EXTRACT_UI` on the editor info so autocorrect does not rewrite
the buffer behind the mirror and a landscape IME does not take over the screen.

What this deliberately does not do: it does not track the egui caret. The
mirror starts empty at focus, so moving the caret by tapping mid-string and
then typing inserts at the caret in egui while the mirror still appends at its
own end — the diff stays correct for append-and-backspace editing, which is
what a phone actually does, and drifts for mid-string surgery. The **paste**
button is unaffected and composes with typing: paste, then keep typing or
backspacing, both land.

### Versioning

Nothing in `android/app/build.gradle` is hardcoded; both fields come from git,
with env overrides so CI can pass them explicitly.

| Field | Source | Override |
|---|---|---|
| `versionCode` | HEAD's commit time in whole minutes since the epoch (`git log -1 --format=%ct` / 60) | `KAI_BUILD_EPOCH` |
| `versionName` | `0.1.<commit count>` (`git rev-list --count HEAD`) | `KAI_BUILD_NUMBER` |
| APK filename | `kai-<versionName>+<sha8>.apk` | `KAI_BUILD_SHA` |

Android refuses an upgrade whose `versionCode` did not increase, so that field
has to be monotonic across every machine that ever ships a build. Commit time
is: any later commit — on any branch, after any rebase — outranks its
predecessor, and unlike a commit count it survives a shallow CI clone, which
carries HEAD's date but not its ancestry. `versionName` stays the readable
`0.1.976`; the sha rides in the filename instead, because Obtainium's
standard-version patterns reject a hex build suffix and quietly turn version
detection off when they meet one. With no git at all the code falls back to
wall-clock minutes and the name to `0.1.0+local`.

### Release signing

`android-release` runs `assembleRelease` and names the output. It signs with
the release key when `KAI_KEYSTORE_BASE64`, `KAI_KEYSTORE_PASSWORD`,
`KAI_KEY_ALIAS` and `KAI_KEY_PASSWORD` are in the environment, and falls back
to gradle's throwaway debug key when they are not, so a plain local build
still works. Point `KAI_SIGNING_ENV` at an env file and the script sources it:

```
KAI_SIGNING_ENV=/run/user/$(id -u)/kai-signing.env android-release
```

The key is `kai_android_signing` in sops (env-file format, four keys); CI
reads the same four from the Woodpecker secret of that name. The base64 is
decoded to `app/build/signing/kai-release.jks` at configure time and never
touches the repo. Certificate SHA-256:

```
A9:AF:EB:9F:2A:D8:90:DA:92:BC:98:41:03:B4:67:B2:CD:D1:CE:92:AE:92:74:E2:1E:16:2F:C7:7C:50:A9:18
```

### Auto-updates

Signed builds are published newest-first at
[kai.rae.blue/apk/](https://kai.rae.blue/apk/) — public, behind the
static-IP ingress on omashu. `kai.apk`
beside them always points at the newest one. A systemd timer on omashu pulls
each new build out of the Forgejo package registry and regenerates that index;
no scp step, no human.

To track it in [Obtainium](https://github.com/ImranR98/Obtainium):

1. **Add app** → URL `https://kai.rae.blue/apk/`.
   Obtainium picks the **HTML** source for an unrecognised URL by itself —
   leave the source dropdown alone.
2. Set three fields, leave the rest at their defaults:

   | Field | Value |
   |---|---|
   | Custom APK link filter by regular expression | `kai-\d+\.\d+\.\d+\+[0-9a-f]+\.apk$` |
   | Version string extraction RegEx | `kai-(\d+\.\d+\.\d+)\+[0-9a-f]+\.apk` |
   | Match group to use for version string extraction RegEx | `1` |

3. Add, then install.

Defaults that matter: *Take first link* off and *Skip sorting* off — Obtainium
natural-sorts the surviving links and takes the highest, so `0.1.1000` beats
`0.1.976` and the page's own order is irrelevant. *Apply version string
extraction Regex to entire page* stays off, or it would match the last version
anywhere in the markup rather than the one it chose. The extracted string is
the APK's real `versionName`, so Obtainium reconciles against the installed
package instead of hashing bytes to guess whether anything changed.

**Uninstall first, once.** Every APK before this one was debug-signed with a
per-machine throwaway key. Android will not upgrade across a signature change,
so `adb uninstall blue.rae.kai` — or uninstall from the launcher — before
installing the first release build. Everything after it upgrades in place.

## What it is / isn't

kai renders; agni decides. Dropping a card emits a request — nothing in this app
mutates game state, so the same simulation can later drive Godot or e-ink front
ends and reject illegal moves from peers.

## Design

- [`wiki/design/table.md`](wiki/design/table.md) — the renderer: billboarding,
  hand layout, foil shader, wasm demo runbook
- [`wiki/design/ux.md`](wiki/design/ux.md) — the UX overhaul (landed, U1–U11): information
  architecture, the table HUD per viewport class, the verb grammar, the
  responsive rules, and the U1–U12 plan
- [`agni architecture.md`](../agni/wiki/design/architecture.md) — the
  simulation boundary

## Protocol compatibility

Multiplayer sessions replicate a shared action log (see
[`wiki/design/deterministic-log.md`](wiki/design/deterministic-log.md));
the log wire format is the compatibility boundary, and phase A replaced the
older event protocol outright. Update every device together — a phase-A
client and a pre-phase-A client will discover each other but cannot play.
The plugin-log generalization (W1 of the agni plugins design) broke the wire
again: genesis carries a table config with zone declarations, deals name
their target zone, and the log grew `Annotate` and `Game` actions — so the
table ALPN moved from `spirit-table/0` to `spirit-table/1`, and mixed
versions will not even connect. The Riftbound MVP (W6) added
`ClientMsg::DealDeck` on the same ALPN; an older host silently ignores the
unknown frame, so mixed versions connect but a joiner on the newer build
cannot deal a deck at an older host — update together anyway.
