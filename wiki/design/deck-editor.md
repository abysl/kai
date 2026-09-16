# Deck Editor — build, check and share a Riftbound deck from the deck box

> Status: designed 2026-09-12, synthesised from three candidate designs
> (player-first, data-first, arena-first). Four implementers own four disjoint
> file sets (§9) and built against the interfaces in §10 without talking. All
> four sets — core, browser, editor, exchange — are built and reviewed; §12
> records where the build departs from the text above it and §13 what the
> review changed. **§14 (0.15.0)** records the move from a sheet over the
> deck box to two screens of their own — the library and the editor — and
> the champion-slot fix; where §4–§7 say "sheet" or "deck box", §14 wins.

Today a deck enters kai only as a pasted list, a link, a code, a pool tile or
a history tile, and leaves it only as the deck code of the *last* import.
Nothing can be built from scratch, a card count can only change by swapping
one copy between main and sideboard between games, nothing checks the
construction rules, and a pool, history or sideboarded deck cannot be shared
at all. The editor fixes all of that with one new sheet over the deck box, one
pure legality module in the rules crate, and four export forms beside the
parsers that already read them. MTG keeps its import path exactly as it is:
the editor, the legality checker and the exporters are Riftbound-only.

## The layers

| Layer | Where | Knows about |
|---|---|---|
| deck document | `agni_riftbound::ResolvedDeck` (`agni/agni/games/riftbound/src/lib.rs`) | the one deck value every parser resolves into, `deal_plan_with` deals from and history snapshots — unchanged shape, two new card fields |
| legality | `agni_riftbound::legality` (new) | rules 103.1–103.4 as a pure `check(&ResolvedDeck, Mode) -> Report`; no I/O, compiled on every target |
| forms | `agni_importers::riftbound::{text_list, code_list, deck_code, link, snapshot}` | render beside parse: community text list, `SET-NNN-COUNT` list, Piltover Archive base32 code, `deckbuilder?code=` link, `agni_deck::Snapshot` |
| catalog | `kai/src/deck/catalog.rs` (new) | the browsable full set per platform — store manifest, gateway manifest, or the pool union — folded to one row per name |
| browser | `kai/src/menu/browser.rs` (new) | search, filter chips, a virtualised art grid whose tap adds a copy, the card detail |
| editor | `kai/src/deck/editor.rs` + `kai/src/menu/editor.rs` (new) | the `Draft` resource and its single mutation path, the sheet: list pane, meter, findings, footer, save |
| exchange | `kai/src/deck/{import,exchange}.rs`, `kai/src/os/{clipboard,qr}.rs` | every way a deck enters or leaves: paste, link, code, file drop, history, pool, clipboard, QR |

## 1. The deck document

There is no second deck struct. The editor's working copy *is* an
`agni_riftbound::ResolvedDeck` — `legend`, `chosen_champion`, `main_deck`,
`runes`, `battlefields`, `sideboard` — wrapped in a kai `Draft` that adds
label, origin, dirty flag, the last legality report and an undo ring. Every
consumer that exists today (`seat_deck`, `deal_plan_with`, the history
snapshot, thumbs, the sideboard step, the deck code) keeps speaking the type it
already speaks.

Two fields join `ResolvedCard`, because two rules cannot be checked without
them:

```rust
pub struct ResolvedCard {
    pub name: String,
    pub riftbound_id: String,
    pub image_url: Option<String>,
    pub kind: Option<String>,
    pub energy: Option<u8>,
    pub power: Option<u8>,
    pub might: Option<u8>,
    pub domain: Vec<String>,
    pub tags: Vec<String>,
    pub signature: bool,
}
```

`ResolvedCard` gains `#[derive(Default)]` so its 21 literal sites can say
`..Default::default()`. `tags` is the Riftcodex `tags` array (`["Lillia"]` on
the legend *Lillia - Bashful Bloom*, `["Fae", "Lillia", "Ionia"]` on the
champion unit *Lillia - Fae Fawn*); `signature` is `supertype == "Signature"`
(61 prints in the current dump) — **not** `metadata.signature`, which marks
signed legend art. The same two fields travel, with serde defaults, through
every shape that carries a card:

| shape | file | change |
|---|---|---|
| `riftcodex::ApiCard` | `importers/src/riftbound/riftcodex.rs` | reads `#[serde(default)] tags: Vec<String>` |
| `ingest::RiftboundCard` (the store manifest, CBOR) | `importers/src/riftbound/ingest.rs` | `#[serde(default)] tags: Vec<String>`; signature derived from `supertype` at load — an old manifest decodes with empty tags |
| `catalog::CatalogCard` | `importers/src/riftbound/catalog.rs` | `tags`, `signature`, plus `set_id: Option<String>` and `text: Option<String>` for the browser's set filter and text search |
| `agni_deck::SnapshotCard` | `games/deck/src/snapshot.rs` | `#[serde(default, skip_serializing_if = "Vec::is_empty")] tags`, `#[serde(default, skip_serializing_if = "std::ops::Not::not")] signature` — **not** part of `identity()` |
| `json.rs` deck reply | `importers/src/riftbound/json.rs` | each card object gains `tags` and `signature`; kai's `parse_reply` reads both with defaults |
| `net::gateway::BridgeCard` (web) | `kai/src/net/gateway.rs` | widened to every manifest field the node already serialises |

Names entering the draft are canonical: `resolve::canonical_name` folds
`(Alternate Art)` prints and the alias table so two prints of one card count as
copies of one name (103.2.b), while `riftbound_id` stays the chosen print so
identity, art and the deck code stay print-exact. Imports already fold
(`resolve::fold_names`); the editor folds at `Edit::Add`.

**Identity is unchanged.** `Snapshot::identity()` still hashes
`(zone, key, count)` only ([deck-history.md](deck-history.md)), so changing a
print mints a new identity and re-saving over a saved deck is *replace*, not
*append* (§6).

The `riftbound_snapshot`/`imported` pair moves from kai's `deck/history.rs`
into `agni_importers::riftbound::snapshot` — `snapshot(&ResolvedDeck) ->
Snapshot` and `deck(&Snapshot) -> Option<ResolvedDeck>` — so the gateway, the
CLI and kai share one round trip; kai's `history.rs` keeps the MTG half and
delegates the Riftbound half.

## 2. Legality — rules as data

New module `agni/agni/games/riftbound/src/legality.rs`, in the crate that
already owns `MAIN_DECK_SIZE`, `RUNE_DECK_SIZE` and `BATTLEFIELD_COUNT`. Pure,
no I/O, no dependency on the importers, so it compiles on wasm and runs on
every platform after every tap.

```rust
pub const COPY_LIMIT: u32 = 3;
pub const SIGNATURE_CAP: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode { Standard }
impl Mode { pub fn battlefields(self) -> u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade { Break, Unverified, Advisory }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone { Legend, Champion, Main, Runes, Battlefields, Sideboard, Deck }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    LegendMissing,
    LegendNotLegend,
    OutOfIdentity { name: String },
    MainSize { have: u32 },
    MainOverSize { have: u32 },
    ChampionMissing,
    ChampionNotChampionUnit,
    ChampionTag { legend_tags: Vec<String>, champion_tags: Vec<String> },
    CopyLimit { name: String, have: u32 },
    SignatureCap { have: u32 },
    SignatureTag { name: String },
    TagsUnverified,
    RuneCount { have: u32 },
    RuneOutOfIdentity { name: String },
    BattlefieldCount { have: u32 },
    BattlefieldDuplicate { name: String },
    SideboardWouldBreak { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: Rule,
    pub cite: &'static str,
    pub grade: Grade,
    pub zone: Zone,
    pub cards: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Meter {
    pub legend: bool,
    pub champion: bool,
    pub main: (u32, u32),
    pub runes: (u32, u32),
    pub battlefields: (u32, u32),
    pub signatures: (u32, u32),
    pub sideboard: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict { Legal, Broken(usize), Unverified }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub verdict: Verdict,
    pub identity: Vec<String>,
    pub meter: Meter,
    pub findings: Vec<Finding>,
}

pub fn check(deck: &ResolvedDeck, mode: Mode) -> Report;
pub fn identity(legend: &ResolvedCard) -> Vec<String>;
pub fn fits_identity(identity: &[String], domains: &[String]) -> bool;
pub fn champion_tags(legend: &ResolvedCard) -> Vec<String>;
pub fn rune_split(domains: usize, total: u32) -> Vec<u32>;
```

`Report` has no `Default` on purpose: an empty deck is `check`ed like any other
and reports its shortfalls. `cards` holds riftbound ids so the UI can scroll to
the row. `cite` is the rule number as printed in
`rules/riftbound-core-rules-v2026-07-16.txt` §101–103. `Verdict` is
`Broken(n)` when any finding is `Break`, else `Unverified` when any is
`Unverified`, else `Legal`; `Advisory` findings never change the verdict.

| rule | cite | grade | what fires |
|---|---|---|---|
| `LegendMissing` | 103.1 | Break | no legend |
| `LegendNotLegend` | 103.1 | Break | the legend slot holds a card whose kind is not `Legend` |
| `OutOfIdentity` | 103.1.b.3–4 | Break | a main-deck or battlefield card with a domain outside the legend's domains; a multi-domain card needs every domain inside; empty or `Colorless` always fits (kai's convention, moved from `pool.rs:314-317`) — one finding per name |
| `MainSize` | 103.2 | Break | `main_deck` + chosen champion < 40; the sideboard never counts |
| `MainOverSize` | 103.2 | Advisory | > 40: legal by "at least 40", but kai deals a shuffled 40 from the whole list, so say it |
| `ChampionMissing` | 103.2.a | Break | no chosen champion |
| `ChampionNotChampionUnit` | 103.2.a.2 | Break | the champion is not a champion unit (a Signature unit such as Tibbers is not) |
| `ChampionTag` | 103.2.a.2 | Break / Unverified | no tag shared between `champion_tags(legend)` and the champion's tags; Unverified when either side has no tags |
| `CopyLimit` | 103.2.b | Break | more than 3 of one name across main + champion |
| `SignatureCap` | 103.2.d.1 | Break | more than 3 signature cards across main + champion |
| `SignatureTag` | 103.2.d.2 | Break / Unverified | a signature card whose tags miss the legend's champion tag; Unverified when its tags are empty |
| `TagsUnverified` | 103.2.a.2, 103.2.d | Unverified | every card in the deck has empty tags — the catalog that resolved it predates tags; one finding, so a pre-re-ingest store reports the tag rules honestly instead of passing them |
| `RuneCount` | 103.3.a | Break | runes ≠ 12 |
| `RuneOutOfIdentity` | 103.3.a.1 | Break | a rune outside the identity |
| `BattlefieldCount` | 103.4.a | Break | battlefields ≠ `mode.battlefields()` (3) |
| `BattlefieldDuplicate` | 103.4.c | Break | two battlefields with one name |
| `SideboardWouldBreak` | — | Advisory | a sideboard card that would break identity or the copy limit if swapped in; the rules text has no sideboard, this is kai's between-games convention |

`champion_tags(legend)` is the legend's tag that names the champion: the last
comma-separated token of the name stem before ` - ` (`champion_name`:
`Vi - Piltover Enforcer` → `Vi`, `Yordle, Kennen - Heart of the Tempest` →
`Kennen` — *Yordle* is a species, not a Champion Tag in the sense of 133.8.b,
so a Teemo or Poppy unit is not Kennen's champion); when no tag equals that
token, all its tags. It is right for all 180 legends in the current dump (the
18 truncated `new: true` draft records such as `Heart of the Tempest` take the
fallback) and the fallback softens a future legend whose tag is not in its
name.

`rune_split(2, 12)` = `[6, 6]`; `rune_split(1, 12)` = `[12]`;
`rune_split(3, 12)` = `[4, 4, 4]`; a remainder goes to the first domain. The
editor uses it for *fill runes* (§5); the rune cards themselves come from the
catalog, which the rules crate never sees.

Copy counting is by `card.name`, which the entry paths keep canonical (§1).
The module does not know the importers' alias table and must not.

Who reads the report:

- the editor meter and findings drawer after every edit (§4);
- `import_step` beside the summary of a resolved import, so a pasted list is
  judged before it is seated;
- `seat` gates: under rules enforced `Broken` disables seating with the count
  in the button label, `Unverified` seats with an amber note; on a free table
  `Broken` seats after a second tap (*seat anyway — plays on free tables
  only*);
- the `pool.rs` test that today hand-checks the six house decks calls `check`
  and asserts no `Break` finding (Unverified is expected while pool markdown
  carries no tags).

## 3. Forms — exact grammars, with one deck through all of them

Every form is rendered beside the parser that reads it, in ungated modules so
the browser build can produce them too. The running example is the pool deck
`rules/pool/lillia-jonnynick.md`.

### 3.1 Text list (community format)

`agni_importers::riftbound::text_list::render(&ResolvedDeck) -> String`
writes the list Rift Atlas and Piltover Archive both import and both export:
a header per section with a colon, a blank line between sections, `N Name`
lines with canonical names merged per name, main sorted by kind (units,
spells, gear) then energy then name; an empty section is omitted.

```
Legend:
1 Lillia - Bashful Bloom

Champion:
1 Lillia - Fae Fawn

MainDeck:
3 Lonely Poro
3 Ravenbloom Student
1 Disarming Rake
1 Janna - Savior
2 Charm

Battlefields:
1 Dusk Rose Lab

Runes:
7 Calm Rune
5 Mind Rune
```

The parser is wider than the writer, so every list either site or a player
produces reads back (`parse_text` over `deck::text_list`):

- headers as `Legend:`, `Legend`, `~~Legend~~` (Rift Atlas), `## Legend` /
  `### Legend` (Piltover Archive), with or without a trailing count
  `(39)`; the header words are `legend(s)`, `champion(s)` / `chosen
  champion`, `main deck` / `maindeck` / `mainboard` / `deck` / `main`,
  `rune(s)` / `rune deck`, `battlefield(s)`, `side` / `sideboard`;
- a single `# title` line and `// …` lines are ignored (a Piltover Archive
  export's deck name);
- counts as `3 Name`, `3x Name`, `3 x Name`, `Name x3`, `Name x 3`, or a
  bare name for one copy;
- a trailing `[SET-NNN]` (Rift Atlas' export with codes) names the print
  and wins over the name — `1 Whoever [OGN-043]` seats Charm;
- names in either punctuation: `Lillia, Fae Fawn` (Piltover Archive, the
  Vendetta printings) or `Lillia - Fae Fawn` (Riftcodex), through the
  catalog's clean-name index.

Round trip: `parse_text(render(d))` resolved against the catalog that produced
`d` yields a deck with the same `Snapshot::identity()` **up to print** — the
text list is name-keyed, so an alternate-art print re-imports as the catalog's
base print. That is stated in the share menu (*names, not prints*), not
hidden. A list without a `Champion:` section — a code list, a Tabletop
Simulator list, a hand-typed one — takes the legend's champion unit out of
the main deck as the chosen champion on resolve (`resolve::infer_champion`),
exactly as Rift Atlas does when it decodes a code.

### 3.2 Code list

`code_list::render(&ResolvedDeck) -> Result<String, String>`. Tokens
`SET-NUMBER[-COUNT]` from `CardCode::from_riftbound_id`, whitespace separated,
legend first, then champion, main, runes, battlefields; a count of 1 is
omitted. Reprint sets fold to the base print (`CardCode::base_print_of_id`,
`opp-183-298` → `OGN-183`); the grammar has no champion slot, so the champion
rejoins the main deck on re-import, and no sideboard section, so a non-empty
sideboard is `Err(code_list::NO_SIDEBOARD)` and the share entry is disabled
with that reason.

```
UNL-189 UNL-082 SFD-036-3 OGN-103-3 SFD-032 SFD-053 OGN-043-2 OGN-045-3 OGN-046-3 OGN-095-3 OGN-058-3 UNL-083-2 OGN-093-2 UNL-069-3 OGN-105 OGN-123 SFD-042-2 OGN-060-3 UNL-078-3 OGN-042-7 OGN-089-5 UNL-209 SFD-215 SFD-217
```

Round trip: `parse_code_list(render(d))` → resolve → the same cards, the
champion inferred back into its slot, every base-set print preserved (an
`UNL-116a` alternate stays `UNL-116a`).

A **Tabletop Simulator list** — Piltover Archive's third export, one
`SET-NNN-V` token per physical card with its variant index, repeated per
copy (`UNL-189-2 OGN-043-1 OGN-043-1 …`) — is told apart from a code list by
its repeated tokens and its variant suffix on every token
(`code_list::looks_like_tts_list`), folded to counts and read the same way;
the variant index is not a print and is dropped.

### 3.3 Deck code (Piltover Archive)

`deck_code::encode_deck(&ResolvedDeck) -> Result<String, String>` moves from
`query.rs` (which is `riftbound-native` only and therefore absent on wasm) into
the ungated `deck_code.rs`, taking `encodable` with it; `query::deck_code_for`
becomes a re-export and `mod.rs::deck_to_code` (which never folded reprints)
is deleted. Reprint sets fold through `CardCode::base_print_of_id`
(`opp-251-298` → `OGN-251`); a print with no wire set and no base fold is the
error, naming the print. The encoder picks the lowest version that carries the
deck (v3 with a champion, v4 with rune-numbered cards, v5 for SP numbers or
counts over 12 main / 3 side).

The Lillia list, with its sideboard and champion, encodes to

```
CMAAAAAAAAAQCAAAFIAACAIAABMQAAYGAAAC2LR2HRPWOAIDAASAEBAAIVHAGAQAAAVV2AIDAAVACBAAKMBQEAAANF5QIAYAEA25OAOZAEBAIAF5AHIQCAACAEBQAIACAUACQPIDAIAAA5D3AEBQASQBAQAFGAIEABJA
```

and `decode` returns 23 main entries (legend, runes and battlefields ride in
`main` and are re-split by kind on resolve), 7 sideboard entries and champion
`UNL-082`. Round trip: `parse_any(code)` → resolve → same identity, prints
preserved up to base-print folding.

The chosen champion is **one of the forty main-deck cards** in the official
code: Rift Atlas and Piltover Archive both write the champion's copy into the
main list and keep `chosenChampion` as a pointer into it, and both take one
copy back out of main when they read a code. kai writes and reads the same
way (`encode_deck` lists the champion in main; `parsed_from_decoded` takes it
out); a code with a pointer to a card absent from main — riftdecks published
such codes — keeps the champion as an extra card rather than losing it, so
both readings seat the same forty.

### 3.4 Link

`link::piltover_url(code)` = `https://piltoverarchive.com/deckbuilder?code=<code>`
and `link::riftatlas_url(code)` = `https://play.riftatlas.com/?deckCode=<code>`.
Either pasted into kai is read without a fetch — `link::code_in_url` lifts the
code out of the query string, so it works in the browser too, where no page
can be fetched — and a riftdecks/riftmana page link still goes through the
fetch-and-scrape path. Pasting the link back into kai's import box
round-trips. No
riftdecks link is emitted: riftdecks.com has no documented import-by-code URL
(and returned 403 to a fetch during the design); kai reads their pages, they
do not read ours.

### 3.5 Snapshot

`riftbound::snapshot::snapshot(&ResolvedDeck) -> agni_deck::Snapshot` and
`deck(&Snapshot) -> Option<ResolvedDeck>`: zones `legend`, `champion`,
`main`, `runes`, `battlefields`, `sideboard`; the history store's CBOR.
Print-exact and sideboard-carrying — the only form that keeps everything, and
the one history uses.

| form | prints | sideboard | where it goes |
|---|---|---|---|
| text list | folded to names | yes | any site or person reading `N Name` lines |
| code list | exact | no | kai, tools that read `SET-NNN` tokens |
| deck code | exact up to reprint folding | yes (≤ 3 per print below v5) | Piltover Archive, kai, a QR across the table |
| link | as the code | as the code | a browser |
| snapshot | exact | yes | the spirit store (history) |

## 4. Screens per size

The editor is `Sheet::DeckEditor`, drawn by `hud::sheet_with` — the deck box
sheet with two options: `SheetLayout { width: SheetWidth::Wide, body_scroll:
false }`. Wide is `min(screen.width, 760)` on Desktop, `min(screen.width,
640)` on Tablet and the full screen on phones, exactly as `sheet_rect` does
today; `body_scroll: false` skips the outer `ScrollArea` so the two panes own
their scrolling and the wheel never fights a nested region. Every other sheet
keeps `SheetLayout::default()` (380 pt, scrolling body) so nothing else
moves. Style comes from `viewport::dress_sheet`: buttons 48/40/24 pt and
body 16/15/14 pt per class without editor-local sizes; the editor only chooses
widths.

Body, top to bottom: **title row** (the label as a `TextEdit::singleline`
behind a pencil link, a `·` after it while dirty), **meter strip** (40 pt),
**panes**, **footer** (56 pt phone / 44 pt desktop).

The meter strip is a row of `menu::chip`s that never scroll away:
`legend ✓` / `legend –`, `champion ✓`, `main 37/40`, `runes 12/12`,
`fields 2/3`, `sig 1/3`, then the verdict pill — `legal` in `tokens.green`,
`3 problems` in `tokens.danger`, `unverified` in `tokens.amber`. Tapping any
chip toggles the **findings drawer**, a `CollapsingState` under the strip
listing `glyph · detail` rows coloured by grade; tapping a finding scrolls the
list to its first card, or, for a shortfall (`MainSize`, `RuneCount`,
`BattlefieldCount`, `LegendMissing`, `ChampionMissing`), opens the browser
with the matching filter. Under the meter on desktop and tablet a 40 pt
**energy curve** (`Draft::curve()`, eight painter bars for cost 0–7+, hover
shows `4 cards at 3`); phones fold it into the counts line.

### 4.1 Desktop (≥ 1100 pt; 1280×800)

`ui.horizontal_top`: the **list pane** at 360 pt on the left, the **browser
pane** taking the rest, each its own `ScrollArea::vertical().id_salt(..)`.
The browser grid runs 3 columns at the ~370 pt that remain (tile min 92 pt,
`TILE_MIN_W_CARD`; 4 columns from 400 pt).
The search field takes focus when the sheet opens and on any keypress while
nothing else is focused; Ctrl/Cmd+F refocuses it; Ctrl/Cmd+Z undoes.

### 4.2 Tablet (600–1099 pt; 1024×768)

The same two panes in a 640 pt sheet: list 300 pt, browser the rest (3
columns). Below 700 pt available the phone-portrait layout applies.

### 4.3 Phone portrait (< 600 pt; 360×800)

Full screen. Under the title a `menu::segmented(ui, &mut draft.pane,
&[(Pane::List, "deck"), (Pane::Cards, "cards")])`; one pane at a time. The
grid runs 3 columns at 360 dp (tile min 92 pt). While the **cards** pane is
up the meter strip and the curve line are not drawn — the verdict pill moves
into the title row and the grid gets their height. The back rung
(`Rung::CloseSheet`) first closes an open detail, then returns from `Cards` to
`List`, then closes the sheet back to the deck box — three taps never lose a
draft, and closing with a dirty draft never asks: the draft persists (§6).
Adding from the cards pane raises a 1.5 s toast `+1 Lonely Poro (2/3)` as an
overlay over the grid's top edge (nothing below it moves) so the player never
leaves the grid to check. The search field never auto-focuses on phones (the
keyboard would cover the grid); `os::ime` handles it when tapped.

### 4.4 Phone landscape (h < 480; 800×360)

Both panes 50/50 with the meter reduced to the verdict pill in the title row;
the findings drawer overlays. The curve is dropped.

### 4.5 The list pane

Sections, each under `menu::step_title` (moved from `deckbox.rs` and made
public):

- **Legend** — one 160×112 landscape tile (`battlefield::wide_tile`, the
  chooser's painter made reusable) with art, name and domain pips; empty →
  a dashed *choose a legend* whose tap filters the browser to legends.
- **Champion** — the same tile; empty → *choose a champion* (filter:
  champion units that fit the identity and share a champion tag when tags are
  known); filled → a `swap` link and `also in main ×N`.
- **Battlefields** — three slots as wide tiles (2 columns at 360 dp via
  `battlefield::tile_fit`); empties say *add a battlefield*; a `×` clears one.
- **Runes** — one row per rune name with a `− N +` stepper; while runes < 12
  a *fill runes* button (disabled with *choose a legend first* when no
  legend) fills to 12 with the basic rune of each identity domain per
  `rune_split`.
- **Main deck** — `Units` / `Spells` / `Gear` groups with weak counts, rows
  sorted by energy then name.
- **Sideboard** — rows with the stepper plus `to main`; main rows carry
  `to side` in their detail.

A row is `rows::card_row` (which the sideboard step adopts too): 40 pt thumb
(placeholder `tokens.surface_2` with the first word when no art), the name as
a frameless button opening the detail, a small `energy · might` line with
domain pips in `theme::domain_color`, then right-to-left `+`, a strong count,
`−`. `+` is disabled at three copies (*three copies is the limit*). A row a
finding names carries a 2 pt danger/amber bar at its left edge; the drawer's
jump lands on it via `scroll_to_rect` on the next frame.

### 4.6 The browser pane

Top: `TextEdit::singleline(&mut search).hint_text("search name or text")`
full width with a trailing `×`. Below it the `menu::chip`s, wrapped into as
many rows as the pane needs on desktop and tablet (two at 370 pt) — kinds
`units · spells · gear · runes · fields · legends` (multi-select, empty =
all), the six domains as pip + name, `fits identity` (selected by default
once a legend exists and drawn **first** while it is on, so the count drop it
causes is explained; cleared, out-of-identity cards show greyed but addable
so a deck can be built before its legend), energy `0-1 · 2-3 · 4-5 · 6+`, and
a `more` menu with *champions only*, *signatures only*, the set toggles and
*reset filters*. On phones the chip row is a horizontal strip.

Grid: `ScrollArea::vertical().show_rows(ui, row_h, rows, ..)` over the
filtered group indices, one tile per **name** (the base print; the other
prints are in the detail), chunked by `tile_columns_for(available, gap,
TILE_MIN_W_CARD)`. A tile is the art (or a placeholder: the energy cost in a
pip filled with the card's domain colour, domain pips along the bottom, the
name inside only when the caption says something else) as a frameless button
— **tap adds one copy**, **long-press or right-click inspects** — with a
`2/3` count badge top-right when the card is in the deck (`3/3` greys the art
and the tap is refused with a 1.5 s amber note floated over the grid's top
edge), and a 32 pt caption (48 dp on phones) whose tap opens the **detail**
(hover on desktop too).
Legend tiles read *set as legend*; a champion unit with a matching tag shows
`★ champion` while the slot is empty and the first tap fills it. Art streams
for the visible rows only: `Thumbs::stage_cards` for what `ArtCache` holds,
`ArtQueue::request_ids` for what it does not, capped at 8 new requests per
frame. The grid's last line is `N cards · <source note>`.

The **detail** is an `egui::Popup` anchored to the tile or row on desktop and
tablet (its scrollbar always visible when it overflows), a full-screen
`egui::Modal` on phones: the name and `×`, then the buttons `add` / `to side`
/ `remove` / a `print` menu listing `set · collector` per print of the name
(choosing one is `Edit::ChangePrint`, which keeps the count) — above the art
so they never fall below the fold — then the card at 220 pt wide (art capped
at 60 % of the screen height on every class, a 140 pt placeholder when no art
has landed), stats, rules text, set label.

### 4.7 The footer

`seat this deck` (primary; the label becomes `seat · 3 problems` and is
disabled under rules enforced when Broken; on a free table the first tap turns
it into `seat anyway`; between games on an active Riftbound table it reads
`use this list next game` and routes through `ReloadDeckRequested` as the
sideboard step does), `save` (native; the web shows `copy code` instead —
there is no store on the web), `share` (§7), `more` (`save as copy`, `undo`,
`discard changes` armed once, `clear deck`). Phones draw the same as one
row of 48 pt buttons.

## 5. Flows

### 5.1 Opening the editor

- **Deck box › your decks** gains a first tile **new deck** (*build from the
  catalog*). With a dirty draft it reads `<label> · draft` / *continue
  editing* and a small *start fresh* link, armed once like forget.
- **History tile › more › edit** loads the saved snapshot (`Origin::Saved(ci)`).
- **Pool** — an `edit a copy` link under each pool tile (`Origin::Pool(slug)`,
  label `<label> (copy)`); saving never touches the pool. Under rules
  enforced the editor still opens — a player prepares a deck for the next
  free table — but seating explains that enforced tables pin the pool decks.
- **Sideboard step › edit the whole deck** seeds from
  `SeatedDeckRecord.deck` (`Origin::Seated`).
- **Import result › edit** beside *seat this deck* seeds from
  `panel.resolved.deck` (`Origin::Import(source)`) with every unresolved
  `(identifier, reason)` copied into `Draft.unresolved`, rendered in the
  findings drawer as amber `not found: <identifier>` rows whose tap opens the
  browser pre-searched so the player picks the intended card.
- **A file dropped on the desktop window** runs the import (§5.3) and the
  result offers `edit` like any other.
- The AI seat never opens the editor.

Every entry builds a `Draft` and hands it to `editor::open`, which remembers
the sheet it replaces (`DeckEditor.return_to`) and sets `menu.sheet =
Some(Sheet::DeckEditor)`; closing the editor (×, the back rung, *discard*)
returns there, seating closes to the lobby. When a **dirty** draft already
exists, every entry point but the tile's own *continue editing* parks the new
draft in `DeckBoxState.pending` and the box asks *replace the <label> draft
with <new>?* — `replace` discards the old one, `keep the draft` drops the
request — so no entry silently overwrites unsaved work.

### 5.2 Editing

`Draft::apply(Edit)` is the only mutation path. `Add(card)` routes by kind
exactly like `Game::place` — Legend replaces the legend (the old one is
dropped, not pushed to main: an explicit editor choice), Rune → runes,
Battlefield → battlefields, a champion unit whose tag matches an empty
champion slot → the slot, else main — and returns `Placed` so the UI can name
the zone, or `Err` with the reason (*already 3 copies of Lonely Poro — rule
103.2.b*, *already 3 battlefields*, *a second Seat of Power*). `SetCount`
with 0 removes; `ToSideboard`/`ToMain` keep the pool constant;
`ChangePrint` keeps count and canonical name; `FillRunes` takes the rune cards
the editor picked from the catalog per `rune_split`. Every successful apply
pushes the previous deck onto a 32-deep undo ring, recomputes
`report = legality::check(&deck, Mode::Standard)` and sets `dirty`.

### 5.3 Import — every path, unchanged grammar

- **Paste** — the deck box's four-row box, `paste` (clipboard slot
  `IMPORT_PASTE`, the one clipboard entry) and `import`, exactly as today:
  `begin_import` → `parse_any` → dispatch → `collect_results`. New on the
  result: the verdict chip next to the summary, `edit`, and the two literal
  reds (`import.rs:997`, `lobby.rs:346`) become `tokens.danger` / `tokens.amber`.
  On desktop a Ctrl/Cmd+V with nothing focused while the deck box is open
  fills the box (`egui::Event::Paste`), so *copy on riftdecks, alt-tab, paste*
  needs no click.
- **Link** — unchanged (`is_link_line` → native `ureq` via
  `resolve_query(DeckQuery::Url)`, web via `/gateway/resolve/deck?url=`). New:
  the page title the reply already carries (`query.rs:146-157`) lands in
  `ResolvedImport.title` and seeds the history label and the draft label
  (*Lillia Aggro* beats *Lillia - Bashful Bloom* at a table), falling back to
  the legend name.
- **Code and code list** — unchanged; the editor's own `copy deck code` is
  proved to round-trip by test.
- **File** — desktop only: `exchange::file_drop` reads bevy's
  `FileDragAndDrop::DroppedFile`; a `.txt`/`.md` under 64 KiB is read (the
  real error into `panel.error` otherwise), `## Deck` fences extracted by
  `text_list::deck_block`, the text put in `panel.paste`, the deck box opened
  on `Sheet::DeckBox(DeckSeat::Mine)` and `begin_import` run — one gesture to
  a resolved deck. Android and the web have no file path (egui has no picker;
  `rfd` would drag gtk into the nix build); paste covers them. Autoplay plans
  gain `saved:<label>` so an editor-built deck can drive the connectivity
  suite; kai-cli and soak keep `--deck <file|url|pool:>`.
- **History and pool** — tiles seat as today; `edit` variants open the editor
  (§5.1). `recall_in`'s backfill also fills `tags`/`signature` by name from
  the store catalog so an old snapshot is checkable.
- **Dead code goes**: `quick_import`, the `QUICK_IMPORT` and `IMPORT_URL`
  clipboard slots and `ImportPanel.quick` — nothing calls them, and
  `AGENTS.md` rows 106–110 still describe the 🃏 button that no longer exists.

### 5.4 Between games

The sideboard step stays the one-tap pool-constant swap (rows now
`rows::card_row` with its `swap` variant) and still does not save to history.
*edit the whole deck* opens the editor seeded from the seated record; seating
from the editor **does** save. Two tools, one job, one persists — the
sideboard step says which in its status row.

### 5.5 MTG

Untouched. The deck box keeps `run_mtg_query` and its wasm refusal; `edit` is
not shown for `ImportedDeck::Mtg`; an editor opened on an MTG lobby shows one
weak line — *the deck editor builds Riftbound decks — MTG lists import from
the box below* — and the import step. `text_list::render` is Riftbound-only.

## 6. Save, history and the draft slot

`exchange::save(draft) -> Result<CiHash, String>`:

| origin | list changed | label changed | what happens |
|---|---|---|---|
| `Saved(ci)` | no | yes | `history::store::rename(game, ci, label)` — one row, same identity |
| `Saved(ci)` | yes | any | `history::store::replace(game, ci, deck, "editor", label)` = `remember_as` then `forget(ci)` — the tile is replaced, not duplicated |
| `New`, `Pool`, `Import`, `Seated` | — | — | `history::store::remember_as(deck, "editor", label)` — a new row |
| any, *save as copy* | — | — | `remember_as`, the old row stays |

After a save `origin = Saved(new ci)` and `dirty = false`. Seating from the
editor saves first with the draft label. On wasm `history::store` is the same
API over `localStorage["kai.decks"]` (one JSON list of `{ci, game, label,
source, snapshot}`, the ci being the snapshot's content identity exactly as
the native store computes it), so the deck box lists, edits, renames and
deletes browser-saved decks like native ones; the footer keeps `copy code`
beside `save` on the web because the list lives in that one browser only.

The **draft slot** makes closing safe without a dialog: `os::drafts` writes
`canonical::to_vec(&snapshot(&deck))` + label + origin as `deck-draft.cbor`
under `os::paths::config_dir()` (native and android) or base64 in
`localStorage["kai.deck-draft"]` (web), debounced 1 s after the last change by
a `persist_draft` system shaped like `refresh_history`; the slot is cleared on
save or discard and restored into `DeckEditor.draft` at startup. A stale draft
whose ids the catalog no longer knows reports them as `not found` rows.

## 7. Share

`share` is a `menu_button` on the editor footer, on the lobby deck card (so a
pool, history or sideboarded deck shares without opening the editor) and on
the import result: `copy text list`, `copy deck code`, `copy code list`,
`copy piltover link`, `show QR`. Each goes through
`os::clipboard::copy_status` and reports *copied to clipboard* in the editor
footer's status line (right of `more`) or the import panel's flash; an entry
whose render fails is disabled with the reason as hover text, verbatim from
the renderer (`encode_deck` names the print: a reprint-set print with no base
fold reads *`<CODE>` is a `<SET>` print with no Piltover Archive set number
and no base print to fold to*, and the token `sfd-t03` — the one print in the
current dump no code carries — fails as an unparsable card code; VEN-SP1
encodes as v5 and is not an error; `code_list::NO_SIDEBOARD` for a benched
card). `show QR` paints the deck code with the `qrcode` crate kai
already depends on — `net::identity::qr_image` moved to `os::qr::image(text)`
— in a modal drawn once per frame from `menu_ui` after every sheet, so it
shows over the deck box on a phone too: the phone-to-phone path across a
tournament table; the opponent scans and pastes the code.

Nothing is added to the wire. A deck-list message would bump `WIRE_VERSION`
and split web↔desktop until main deploys, and a joiner's main deck is
`ZoneVisibility::None` by design; the code travels by clipboard, QR or a
message outside kai. A wire share is a named later phase, scheduled with the
next planned bump.

## 8. Catalog per platform

`deck::catalog::Catalog` is the browser's data, loaded once and rebuilt when
its source changes:

| platform | source | note in the grid footer |
|---|---|---|
| desktop, android | `ingest::load_manifest(store_dir)` when the `riftbound` ref exists **and** lists at least as many prints as the pool (`outranks_pool`); reloaded when the ref changes, so the full-set download lands — a manifest that only holds the few records deck-art fetches wrote stays on the pool | `1451 prints · full set` |
| web | `net::gateway::riftbound_catalog()` once `boot()` has the manifest | `1451 prints · from your gateway` |
| anywhere without those | `pool::cards()` — Origins complete, other sets only the six decks' cards, no art | *offline catalog: Origins and the pool decks — download the full set in Settings › advanced* |

Groups fold the 1451 prints of the current dump to 938 names (the full ingest
keeps Riftcodex's duplicate ids — `(Metal)` reprints and truncated draft
records share an id with their base print — and so does the test fixture);
the default print is the base-set print `CardCode::base_print_of_id` would
fold to. `ingest_deck_art` now writes the record it fetched art for from the
`CatalogCard` the lookup returned (`ingest::record_of`: kind, domain, stats,
tags, text, set), so a deck's art no longer pads the manifest with kind-less
cards; a record padded by an older kai (`CatalogCard::is_padded`) is hidden
unless the search matches its name, and both the import path
(`catalog::Layered { store, Riftcodex }`) and the art worker fall through to
Riftcodex for a padded hit or a miss. `tags_known` is true when any card
carries tags; when false, Settings › advanced adds *re-download to check
champion tags*. The browser never calls the live API: a search miss shows
`0 cards` (a Riftcodex `fuzzy(name)` fallback is a later phase). Filtering
is a linear scan over ~1k short strings, cached per (filter, search,
identity, generation) so `show_rows` sees a stable slice; the set and domain
chip lists are computed once per catalog.

## 9. Ownership — four implementers, disjoint files

**core** (agni): `agni/agni/games/riftbound/src/lib.rs` (the two fields,
`Default`, `pub mod legality`), `agni/agni/games/riftbound/src/legality.rs`
(new), `agni/agni/games/deck/src/snapshot.rs`, everything under
`agni/agni/importers/src/riftbound/` (`catalog.rs`, `ingest.rs`,
`riftcodex.rs`, `resolve.rs`, `json.rs`, `text_list.rs`, `code_list.rs`,
`deck_code.rs`, `query.rs`, `link.rs`, `mod.rs`, new `snapshot.rs`),
`agni/agni/importers/src/bin/resolve_riftbound.rs` (`--text`/`--code`
flags), `agni/agni/games/riftbound/rules/pool/*.md` untouched. Tests: every
`Rule` on a minimal fixture deck; empty tags → Unverified not Break;
`champion_tags` on the two name shapes; `rune_split`; `render` round trips for
the six pool decks in all three text forms; `encode_deck` errors with the
print name for a `ven-sp1-006`-only deck; `extract(piltover_url(c))` returns
`c`; a manifest without `tags` loads with empty tags; a snapshot CBOR written
before the fields decodes and `identity()` is unchanged with tags present.
Also `kai/src/deck/pool.rs`'s `in_domain_identity` delegates to
`legality::fits_identity` and its scripted-deck test calls `check` — the one
kai file core touches, because it is the fixture set.

**browser** (kai): `src/deck/catalog.rs` (new), `src/menu/browser.rs` (new),
`src/deck/thumbs.rs` (`stage_cards`), `src/render/art.rs`
(`ArtQueue::request_ids`), `src/net/gateway.rs` (`BridgeCard` widened,
`riftbound_catalog()`), `src/theme.rs` (`Tokens.domain`, `domain_color`),
`src/settings/advanced.rs` (the `tags_known` note). Tests: groups fold the
scratchpad dump to 989 names with base-set defaults; filter by kinds, domains,
identity, energy band, text; padding hidden; pool fallback flagged; headless
egui at 400 pt and 360 pt: column counts, a tap on a tile returns
`BrowserAction::Add`.

**editor** (kai): `src/deck/editor.rs` (new), `src/menu/editor.rs` (new),
`src/deck/rows.rs` (new), `src/os/drafts.rs` (new), `src/deck/history.rs`
(delegate to `riftbound::snapshot`, `remember_as`, `replace`, backfill tags),
`src/deck/sideboard.rs` (`card_row` from rows, *edit the whole deck*),
`src/deck/battlefield.rs` (`wide_tile`), `src/menu/deckbox.rs` (entry
points), `src/menu/mod.rs` (`Sheet::DeckEditor`, the rung ladder, `pub fn
step_title`), `src/table/hud.rs` (`SheetLayout`, `sheet_with`),
`src/settings/mod.rs` (`DeckParams` gains `editor`, `catalog`, `browser`
thumbs), `src/app.rs` (calls the three `register` fns). Tests: every `Edit`
invariant, undo cap, report recompute; `save` semantics against a temp store
(rename-only, replace forgets the old ci, copy keeps both); drafts round trip
in a temp config dir; headless egui at 360×780, 780×360, 1024×768, 1280×800
(meter and footer inside the screen rect, one pane and the segmented row on
the phone, both panes on the desktop); the `hud` sheet tests still see 380 pt
for every other sheet.

**exchange** (kai): `src/deck/import.rs`, `src/deck/exchange.rs` (new),
`src/os/clipboard.rs`, `src/os/qr.rs` (new, from `net/identity.rs`, which
then uses it), `src/menu/lobby.rs` (deck card `share`, the literal red),
`src/autoplay.rs` + `tests/connectivity/plan.py` (`saved:<label>`),
`AGENTS.md` rows 106–110. Tests: `render` for every `Share` against the six
pool decks; `parse_reply` reads `tags`/`signature`/`title` with defaults;
`import_step` returns `ImportAction::Edit` for Riftbound and never for MTG;
the code round-trips through `parse_any`; a file drop feeds `panel.paste`;
`saved:` resolves a held row by label.

**wiki**: this page (editor owner keeps it current), `ux.md` §2.4 links here,
`deck-history.md` gains the save/replace table of §6.

## 10. The interfaces — exact, copy them

Cross-owner names. Each owner may stub the other side's items to compile; the
signatures do not move.

core → everyone:

```rust
agni_riftbound::ResolvedCard { .., pub tags: Vec<String>, pub signature: bool }   // + #[derive(Default)]
agni_riftbound::legality::{check, Mode, Report, Verdict, Meter, Finding, Rule, Grade, Zone,
                           identity, fits_identity, champion_tags, rune_split, COPY_LIMIT, SIGNATURE_CAP}
pub fn legality::check(deck: &ResolvedDeck, mode: Mode) -> Report
pub fn legality::fits_identity(identity: &[String], domains: &[String]) -> bool
pub fn legality::champion_tags(legend: &ResolvedCard) -> Vec<String>
pub fn legality::rune_split(domains: usize, total: u32) -> Vec<u32>
agni_importers::riftbound::catalog::CatalogCard { .., pub tags: Vec<String>, pub signature: bool, pub set_id: Option<String>, pub text: Option<String> }
agni_deck::SnapshotCard { .., pub tags: Vec<String>, pub signature: bool }   // serde default, not identity
pub fn agni_importers::riftbound::text_list::render(deck: &ResolvedDeck) -> String
pub fn agni_importers::riftbound::code_list::render(deck: &ResolvedDeck) -> Result<String, String>
pub fn agni_importers::riftbound::deck_code::encode_deck(deck: &ResolvedDeck) -> Result<String, String>
pub fn agni_importers::riftbound::link::piltover_url(code: &str) -> String
pub fn agni_importers::riftbound::snapshot::snapshot(deck: &ResolvedDeck) -> agni_deck::Snapshot
pub fn agni_importers::riftbound::snapshot::deck(snapshot: &agni_deck::Snapshot) -> Option<ResolvedDeck>
pub fn agni_importers::riftbound::resolve::canonical_name(riftbound_id: &str, name: &str) -> String   // exists
json deck reply: each card object gains "tags": [..], "signature": bool
```

browser → editor, exchange:

```rust
#[derive(Resource, Default)]
pub struct crate::deck::catalog::Catalog { pub cards: Vec<CatalogCard>, pub groups: Vec<Group>, pub source: Source, pub tags_known: bool, pub generation: u32 }
pub struct Group { pub name: String, pub prints: Vec<usize>, pub default_print: usize, pub kind: CardKind, pub domain: Vec<String>, pub champion: bool, pub signature: bool, pub tags: Vec<String>, pub energy: Option<u8>, pub might: Option<u8>, pub power: Option<u8>, pub text_lower: String }
pub enum Source { None, Pool(usize), Store(usize), Gateway(usize) }
impl Catalog {
    pub fn card(&self, print: usize) -> &CatalogCard;
    pub fn group_of(&self, riftbound_id: &str) -> Option<usize>;
    pub fn find_name(&self, name: &str) -> Option<usize>;
    pub fn resolved(&self, print: usize) -> ResolvedCard;
    pub fn basic_rune(&self, domain: &str) -> Option<usize>;
    pub fn filter(&self, filter: &Filter, search: &str, identity: &[String]) -> Vec<usize>;
    pub fn note(&self) -> String;
}
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Filter { pub kinds: BTreeSet<CardKind>, pub domains: BTreeSet<String>, pub fits_identity: bool, pub energy: Option<EnergyBand>, pub champion_only: bool, pub signature_only: bool, pub sets: BTreeSet<String> }
pub enum EnergyBand { Low, Mid, High, Top }
pub fn crate::deck::catalog::register(app: &mut App);

#[derive(Default)]
pub struct crate::menu::browser::BrowserState { pub search: String, pub filter: Filter, pub detail: Option<usize>, pub note: Option<(String, f64)> }
pub enum BrowserAction { Add(usize), SetLegend(usize), SetChampion(usize), ChangePrint(usize), ToSideboard(usize), Remove(usize) }   // usize = print index
pub struct BrowserContext<'a> { pub identity: &'a [String], pub champion_tags: &'a [String], pub copies: &'a BTreeMap<String, u32>, pub champion_empty: bool, pub class: ViewportClass, pub input: InputKind }
pub fn crate::menu::browser::browser_pane(ui: &mut egui::Ui, state: &mut BrowserState, catalog: &Catalog, thumbs: &mut Thumbs, art: &mut ArtCache, ctx: &BrowserContext) -> Vec<BrowserAction>
pub fn crate::menu::browser::card_detail(ui: &mut egui::Ui, state: &mut BrowserState, catalog: &Catalog, group: usize, thumbs: &Thumbs, ctx: &BrowserContext) -> Vec<BrowserAction>
pub fn crate::theme::domain_color(tokens: &Tokens, domain: &str) -> egui::Color32
impl Thumbs { pub fn stage_cards<'a>(&mut self, contexts: &mut EguiContexts, registry: &mut EguiArt, images: &mut Assets<Image>, cards: impl IntoIterator<Item = &'a CatalogCard>, art: &mut ArtCache) }
impl ArtQueue { pub fn request_ids<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) }
pub fn crate::net::gateway::riftbound_catalog() -> Option<Vec<CatalogCard>>
```

editor → browser, exchange:

```rust
#[derive(Resource, Default)]
pub struct crate::deck::editor::DeckEditor { pub draft: Option<Draft>, pub note: Option<String> }
pub struct Draft { pub deck: ResolvedDeck, pub label: String, pub origin: Origin, pub dirty: bool, pub report: legality::Report, pub unresolved: Vec<String>, pub undo: VecDeque<ResolvedDeck>, pub pane: Pane, pub browser: BrowserState, pub scroll_to: Option<String> }
pub enum Origin { New, Pool(String), Saved(CiHash), Import(String), Seated }
pub enum Pane { List, Cards }
pub enum Zone { Legend, Champion, Main, Runes, Battlefields, Sideboard }
pub enum Edit { SetLegend(ResolvedCard), ClearLegend, SetChampion(ResolvedCard), ClearChampion, Add(ResolvedCard), AddTo { zone: Zone, card: ResolvedCard }, SetCount { zone: Zone, id: String, count: u32 }, ToSideboard { id: String }, ToMain { id: String }, ChangePrint { zone: Zone, id: String, card: ResolvedCard }, FillRunes(Vec<(ResolvedCard, u32)>), Clear }
pub enum Placed { Legend, Champion, Main, Runes, Battlefields, Sideboard }
impl Draft {
    pub fn new(label: &str, origin: Origin) -> Self;
    pub fn from_deck(deck: ResolvedDeck, label: &str, origin: Origin) -> Self;
    pub fn apply(&mut self, edit: Edit) -> Result<Placed, String>;
    pub fn undo(&mut self) -> bool;
    pub fn copies(&self) -> BTreeMap<String, u32>;
    pub fn identity(&self) -> Vec<String>;
    pub fn champion_tags(&self) -> Vec<String>;
    pub fn curve(&self) -> [u32; 8];
}
pub fn crate::deck::editor::open(editor: &mut DeckEditor, menu: &mut MenuState, draft: Draft);
pub fn crate::deck::editor::register(app: &mut App);
pub fn crate::deck::rows::verdict_chip(ui: &mut egui::Ui, verdict: legality::Verdict) -> egui::Response;
pub fn crate::deck::rows::card_row(ui: &mut egui::Ui, card: &ResolvedCard, count: u32, thumb: Option<egui::TextureId>, flagged: Option<Grade>, action: RowAction) -> Option<RowEvent>
pub enum RowAction { Stepper { cap: u32 }, Swap }
pub enum RowEvent { Plus, Minus, Open, Swap }
pub fn crate::deck::history::store::remember_as(deck: &ImportedDeck, source: &str, label: &str) -> Result<CiHash, String>
pub fn crate::deck::history::store::replace(game: &str, old: CiHash, deck: &ImportedDeck, source: &str, label: &str) -> Result<CiHash, String>
pub enum crate::menu::Sheet { DeckBox(DeckSeat), DeckEditor }
pub fn crate::menu::step_title(ui: &mut egui::Ui, text: &str)
pub struct crate::table::hud::SheetLayout { pub width: SheetWidth, pub body_scroll: bool }   // Default = Standard, true
pub enum SheetWidth { Standard, Wide }
pub fn crate::table::hud::sheet_with(context: &egui::Context, id: &str, class: ViewportClass, side: Side, title: &str, open: &mut bool, layout: SheetLayout, body: impl FnOnce(&mut egui::Ui))
```

exchange → editor:

```rust
pub enum crate::deck::exchange::Share { TextList, DeckCode, CodeList, PiltoverLink }
pub fn crate::deck::exchange::render(deck: &ResolvedDeck, what: Share) -> Result<String, String>
pub fn crate::deck::exchange::share_menu(ui: &mut egui::Ui, deck: &ResolvedDeck, qr: &mut Option<String>) -> Option<String>   // Some(status note) after a copy
pub fn crate::deck::exchange::qr_modal(context: &egui::Context, code: &mut Option<String>)
pub fn crate::deck::exchange::save(draft: &mut Draft, game: &str) -> Result<CiHash, String>
pub fn crate::deck::exchange::register(app: &mut App);   // file_drop, paste-when-unfocused
pub struct crate::deck::import::ResolvedImport { pub deck: ImportedDeck, pub unresolved: Vec<(String, String)>, pub code: Option<String>, pub title: Option<String> }
pub enum crate::deck::import::ImportAction { Seat, Edit { deck: ResolvedDeck, label: String, source: String, unresolved: Vec<String> } }
pub fn crate::deck::import::import_step(ui: &mut egui::Ui, ..existing params.., offer_edit: bool) -> Option<ImportAction>
pub fn crate::os::qr::image(text: &str) -> Result<egui::ColorImage, String>
```

Removed, by exchange: `import::quick_import`, `ImportPanel.quick`,
`clipboard::{QUICK_IMPORT, IMPORT_URL}`; by core: `riftbound::deck_to_code`.

## 11. Why this shape, and what was not taken

The three candidates agreed on the document (`ResolvedDeck`, no second
struct), on legality as a pure module in the rules crate, on tags reaching the
card, on a 760 pt two-pane sheet that collapses to segmented panes on phones,
on the community text list rendered beside its parser, on `deckbuilder?code=`
as the only link, and on nothing new on the wire. The differences, resolved:

- **Sheet, not a full-screen page.** The arena-first design drew the editor
  with `screen_frame`; the deck box is a sheet and the editor is its fifth
  step, so it stays a sheet with two layout options rather than a new screen
  class.
- **One `Draft` over `ResolvedDeck`, not a `DeckDraft` that converts.** A
  second struct means two truths and a conversion to test.
- **Fields on `ResolvedCard`, not a facts closure.** 21 literal sites versus a
  `&dyn Fn` threaded through the checker; `Default` makes the literals
  one-line edits and the checker stays a function of the deck alone.
- **An `Edit` enum with one `apply`** (data-first) rather than a method per
  operation: the undo ring costs nothing and every invariant is one test.
- **Rule cites on findings** (data-first), because a player reading *rule
  103.2.b* can look it up.
- **`encode_deck` in the ungated `deck_code.rs`** (data-first): `query.rs` is
  native-only, so the player-first plan's `query::deck_code_for` would have
  left the web unable to copy a code.
- **Tap adds, caption opens** (player-first) rather than rows with a `+`: the
  art is the value on a phone; the count badge, the toast and the stepper
  make a mis-tap two taps to undo. Drag-and-drop (arena-first) is not built —
  it fights `ScrollArea` on touch and tap-to-add is faster on a mouse too.
- **Replace on save, no lineage.** The data-first parent-`CiHash` lineage
  costs one store read per saved deck per 2 s refresh and a second history
  model; replace + *save as copy* covers the need.
- **No `[OGN-030a]` print notes in the text list** (arena-first): it changes
  the community grammar every other site reads; the code list and deck code
  are the print-exact forms.
- **No new dependency.** The QR uses the `qrcode` crate kai already has; no
  `rfd`, no image downscaling — the grid stages textures for visible rows only
  and the existing `ArtCache` bounds memory.
- **No kai-cli `deck` subcommands, no wire share** — each is a named later
  phase.

## 12. As built — the editor set

What the editor owner built differs from the design text in these places;
the interfaces in §10 are unchanged unless named here.

- **`Draft.edits: u64`** joins the struct: it counts every successful apply,
  undo and rename, so the art requester and the draft slot know when the
  deck changed without hashing it each frame. `Draft::rename` trims and
  ignores an empty or unchanged label. `Draft::locate(name)`,
  `rune_fill(basic)`, `back()`, `flagged(id)` and `champion_fits(card)` are
  helpers the sheet uses; `main_groups` / `sorted_by_name` / `shortfall_filter`
  are free functions beside them.
- **`copies()`** counts main + champion + runes + battlefields by name and
  leaves the sideboard out, because the browser's badge and `capped` read one
  map for every kind (`3/3` on a unit, `1` on a battlefield) while the copy
  limit must not see benched copies.
- **`apply` refusals**: main and champion share the 3-copy limit by canonical
  name; the sideboard caps at 3 per name; battlefields refuse a second copy of
  a name and a fourth card; `SetLegend`/`SetChampion` refuse a card whose kind
  is known and wrong (*X is a Spell, not a champion unit*); replacing a
  champion drops the old one like a legend. Runes are never refused by
  `apply` — the row stepper caps at 12 and the meter says the rest. A no-op
  apply (clearing an empty slot) is `Ok` but is not an edit.
- **`Add` routing** puts a champion unit into the empty champion slot only when
  a legend is chosen and `legality::champion_unit` says yes and the tags
  match; without tags it goes to main.
- **Layout by screen, not only by class.** `layout_for(class, screen)`:
  phone portrait → segmented panes; phone landscape, or any screen under 520 pt
  tall (an 800×480 window classifies as *Tablet*), → split 50/50 with only the
  verdict pill in the title row and no curve; a tablet screen under 700 pt
  wide → segmented; otherwise two panes (360 / 300 pt list). The phone-portrait
  meter is one 40 pt row that scrolls horizontally with the verdict pill first;
  on phone portrait `share` folds into the `more` menu as a submenu so the
  footer's four actions fit 328 pt. The label is a `rename` link, not a pencil
  glyph (the UI font has no ✎).
- **Rows open the browser's detail.** A row's name tap calls
  `BrowserState::open_detail(group, row_rect)` and the browser pane draws its
  popup anchored to the row (a modal on phones, where the pane also switches
  to *cards*); the list pane never draws a detail of its own. A card the
  catalog does not know says so in the status line instead.
- **Tiles.** `battlefield::wide_tile(ui, WideTile { .. })` paints art or a
  placeholder (dashed for empty slots), a name band, domain pips and the
  selection stroke; the chooser's `battlefield_step` uses it too. Every
  Riftbound print but a battlefield is portrait (legends included), so the
  legend and champion are cropped to their art band (`art_uv`) rather than
  stretched into the landscape tile; in the browser grid a battlefield is
  letterboxed inside the portrait cell (`browser::art_box`) and the detail
  view sizes its art by the same rule.
- **Toast and status** are two timed slots on `EditorSheet` (1.5 s and 4 s);
  the toast wins while both live. Clipboard results, save results and
  refusals land in the status; adds land in the toast as `+1 Name (2/3)` or
  `Name → zone`.
- **Seat from the editor** saves first on native (the web notes it cannot),
  seats through `import::seat_deck`, requests `ReloadDeckRequested` when the
  sideboard step would (`sideboard::reload_allowed`), closes the sheet and
  keeps the (now clean) draft, so *new deck* reads *new deck* again.
- **Draft slot.** `persist_draft` writes 1 s after the last change while the
  draft is dirty and clears the slot as soon as the draft is clean or gone;
  `os::drafts` stores `{label, origin, snapshot}` as canonical CBOR in
  `config_dir()/deck-draft.cbor` (native) or JSON text under
  `localStorage["kai.deck-draft"]` (web — no base64 dependency). A restored
  draft is dirty.
- **Sideboard step** returns `SideboardOutcome { reload, edit }`; *edit the
  whole deck* seeds `Origin::Seated`; its note says swaps never save.
- **History store** gained `_in(dir, ..)` variants of `remember_as`, `replace`,
  `rename` and `forget` for tests; `backfill` also fills `tags`/`signature`
  by name and re-folds the canonical name.
- **Harness hooks** (`src/app.rs`): `KAI_OPEN=deck-editor[:new|:draft|:pool:<slug>]`
  or `KAI_OPEN=deck-box` opens the Riftbound lobby and that sheet at frame 2;
  `KAI_SHOT_FRAME=<n>` moves the startup screenshot later than frame 6. A
  sheet needs a few frames before egui settles its layer order — a frame-6
  shot shows the lobby through any sheet, the deck box included.
- **Not built here**: Ctrl/Cmd+F lives in the browser pane (it owns the
  search field); Ctrl/Cmd+Z undoes when nothing has keyboard focus. The
  `not found` rows come from `Draft.unresolved` only — a stale draft slot is
  not cross-checked against the catalog on restore.
- **Departures from §4 as drawn**: the desktop browser pane is ~370 pt, so
  the grid is 3 columns with `TILE_MIN_W_CARD = 92` (§4.1 said 4 at 104);
  `Draft::back` closes the detail before it leaves the cards pane (§4.3 had
  the order reversed); the copy/save status is the footer's status line and
  the toast an overlay (§4.3/§7).

## 13. After the review

The 2026-09-12 review of the four sets drove these changes; each is the
current behaviour and the text above already reads that way.

- **Art in the grid**: `stage_draft_art` stages the browser's visible prints
  through `Thumbs::stage_cards` every frame, so tiles get textures as soon as
  the bytes land (before, only cards already in the deck did).
- **The store never shrinks the catalog**: `platform::load_catalog` keeps the
  pool while a manifest lists fewer prints than the pool (`outranks_pool`),
  `ingest_deck_art` writes full records (`record_of`) instead of padded ones,
  and `run_riftbound_query` resolves through `Layered { store, Riftcodex }`
  so a padded or missing store record never makes an import `unverified`.
- **Champion tags**: `champion_tags` keeps only the tag equal to the last
  comma token of the name stem (`Kennen`, not `Yordle`); Kennen + Teemo is a
  `ChampionTag` Break.
- **`Game::place`** keeps the extra copies of a counted `Legend`/`Champion`
  line in the main deck (`Champion\n2 Lillia - Fae Fawn` → one in the slot,
  one in main).
- **Navigation**: `DeckEditor.return_to` sends × / back / discard to the
  sheet the editor replaced; `DeckEditor.opens` salts the list pane's scroll
  and `EditorSheet::begin_open` resets the drawer, rename and armed flags per
  open; the import QR modal is drawn from `menu_ui` so it shows over a
  phone's deck box.
- **Dirty drafts**: every entry point routes through the *replace the …
  draft?* confirmation (`DeckBoxState.pending`), not only the new-deck tile.
- **Phone cards pane**: no meter strip or curve line while browsing (the pill
  sits in the title row); the toast and the refusal note are `menu::float_note`
  overlays so the grid never reflows under a tap; captions are 48 dp on
  phones and long-press inspects; a unit/rune row's middle column is bounded
  by `rows::controls_width` so the stat line never wraps under the stepper.
- **Chips**: wrap on desktop and tablet at any pane height (the 520 pt strip
  threshold is gone), `fits identity` is drawn first while active, set and
  domain lists are cached on the `Catalog`.
- **Detail**: actions under the title, the placeholder 140 pt without art,
  the 60 % height cap on every class, a visible scrollbar.
- **Deck box**: the new-deck tile shares the first pool row (`pool_rows`),
  each pool tile carries `edit a copy`, the `edit a copy of…` menu is gone;
  `paste`, `done`, `save name`, `cancel` are full-size buttons; the import
  result's `seat this deck` is the green primary and sits with `edit`/`share`
  directly under the summary line; the paste box scrolls inside four rows;
  the `code:` row and its copy button are gone (the share menu carries it);
  the legality report of a resolved import is computed once
  (`ImportPanel::set_resolved`).
- **Small things**: the zone toast reads `Name · legend` (U+2192 has no
  glyph in the UI font); `clear deck` is armed like *discard changes*;
  `backfill` refills a padded `Other` kind and counts only cards it changed;
  the offline pool catalog keeps each card's rules text (`text` search and the
  detail's rules line work on the pool fallback).

## 14. Screens of their own (0.15.0)

The player's complaint: *change deck* opened a sheet that was half a deck
picker and half a deck editor, and the editor itself was a sheet over that
sheet. Now:

- **Two screens, not sheets.** `Screen::Decks` is the **library**
  (`src/menu/decks.rs`): the draft in progress, *your decks* (every game's
  saved rows: edit · rename · share · delete), *scripted decks* (*edit a
  copy* · share), and the **import** box with *save to your decks* / *edit*
  / share. `Screen::DeckEditor` is the editor of §4, drawn full-screen by
  `editor_screen` with a ‹ back to the library instead of an ×. Both are
  reached from Home's *deck editor* row, the lobby deck card's *edit this
  deck* / *deck editor* links, and the deck box's one link *open the deck
  editor*; `Menu.decks_from` remembers which, and the back ladder walks
  detail → cards pane → library → that screen. Seating from the editor
  (`seated_leave`) returns to the Riftbound lobby or the live table.
- **The deck box only selects.** Pool tiles and saved tiles seat (or pin, or
  set the AI's choice), the battlefield and sideboard steps stay, and the
  new-deck tile, *edit a copy*, the rename/edit/delete menu and the import
  step are gone from it. The sideboard step's *edit the whole deck* still
  opens the editor (through `decks::open_draft`, which parks the draft and
  asks *replace the … draft?* over a dirty one).
- **Import saves, the lobby seats.** The import result no longer seats a
  deck; *save to your decks* (`store::remember_as` under the page title or
  the headline card) files it, and the lobby's deck box lists it. MTG lists
  import and save too (`game_of_paste` picks the parser); only Riftbound
  decks edit. A desktop file drop or an unfocused Ctrl/Cmd+V lands on the
  library.
- **The champion slot.** Every existing store manifest carries the
  `Champion` supertype but no tags, and a legend's `champion_tags` falls
  back to its name stem, so the browser cue already worked with a legend
  chosen — but with **no legend yet** `champion_tags` was empty and every
  champion unit went to main, a kind-less (padded) record could never be a
  champion, and a champion whose three copies were already in main was
  refused with the copy limit. Now `Draft::champion_fits` fills the empty
  slot with any `Name - Title` unit before a legend is chosen (the
  `ChampionTag` rule judges it once the legend lands), treats an unknown
  kind as a unit, and `SetChampion` promotes a main copy when main is full
  (`take_one_named`); the browser cues every champion unit while no legend
  is chosen (`champion_cue`), lets a capped one still be picked, and the
  detail popup offers *set as champion* by hand for a champion unit outside
  the legend's tag (`champion_offer`, the verdict says so).
- **Import lives in the editor and reads the clipboard itself.** The
  library has no paste box: the editor footer's *import* (in *more* on
  phones) calls `os::clipboard::request_paste(IMPORT_SLOT)`, and
  `import_modal` polls the slot, detects the game and form
  (`import::game_of_paste`, `begin_import_any`) and shows the result with
  *load into the editor* (`Draft::load` — an undoable `Edit::Replace`; a
  fresh *new deck* takes the import's name, a named draft keeps its own),
  *save to your decks* and share. A desktop file drop or an unfocused
  Ctrl/Cmd+V opens the editor (a fresh draft if none) and runs the same
  import (`menu::editor::import_text`).
- **No preconstructed lists.** Every card is scripted, so the deck box and
  the library no longer show the pool decks and rules enforced no longer
  pins them: a saved deck seats on an enforced table like on a free one
  (`autoplay::Plan` accepts saved decks with `enforced: true`). `pinned::pin_decks`
  only fills an empty seat (the AI's *let the AI pick* and the autoplay
  harness still draw from `pool`), and the coverage chip is gone.
- **The battlefield is chosen at the table.** The lobby never blocks on it
  (`DeckState::NeedsBattlefield` is gone, the deck box has no battlefield
  step, the AI card no battlefield line). `import::body_plan` deals a
  multi-battlefield deck without its battlefields the moment the table is
  active, so the opponent's legend is visible; `battlefield::prompt_ui`'s
  tray asks once my legend is out, `place_chosen` turns the pick into
  `PlaceBattlefieldRequested`, and `net::route_battlefield_placements` sends
  `import::battlefield_plan` — a `Spread(battlefield-)` group the host
  accepts as a second deal (`battlefield_only`, refused only once that seat's
  battlefield is on the band). `plugin_ui::gate_roll` disables the
  first-player `Commit` affordances with *choose your battlefield first*
  while `battlefield::placement_pending`, so the roll follows the choice.
- **Storage on every platform.** The web store is now `history::kept`, the
  same logic over a `KeyValue` trait that the wasm build binds to
  localStorage and the tests run over an in-memory map; `kept::identity_of`
  is the native store's ci. `src/deck/roundtrip_tests.rs` takes one real
  deck through six sources (PA code, PA link, PA page, riftdecks.com page,
  Rift Atlas link, text list) and both stores, plus the editor's save and
  the draft slot.

## Known limitations

- Champion-tag and signature rules need `tags` on the card. Every existing
  store manifest (this machine's Sep 8 `riftbound` ref, every android install,
  every gateway) lacks them until *download full riftbound set* runs again and
  the gateways re-publish; until then those rules report `Unverified` and the
  meter says `unverified`, not `legal`. The amber row and the Settings note say
  so; nothing forces the re-download.
- `champion_tags` is a name-stem heuristic (the last comma token) that fits
  every legend in the current dump; a future legend whose tag is not in its
  name falls back to all its tags. Refresh the fixture with each ingest.
- Copy counting trusts canonical names. A snapshot saved before this change
  with an `(Alternate Art)` name still counts as a separate name until
  `recall_in`'s backfill re-folds it.
- The text list folds prints; the code list carries no sideboard; the deck
  code folds reprint sets and refuses prints with no base fold. Each is stated
  in the share menu rather than fixed.
- Deck identity is print-keyed, so a print swap on a saved deck replaces its
  history row (the label carries over).
- Before the full-set download the desktop browser is an Origins-plus-pool
  grid whose art streams in one Riftcodex fetch per visible tile, and a search
  miss shows `0 cards` (no live fallback). Android on cellular at a
  tournament feels this most.
- The web has no history store: the draft slot and the clipboard are its only
  persistence, and widening `BridgeCard` grows the boot fetch to roughly 1–2 MB
  of manifest JSON — measure; a `/manifest?fields=` projection is a spirit
  change if it is slow.
- File import is desktop drag-and-drop only; no android share intent, no web
  picker.
- MTG gets nothing new: no editor, no legality, no export.
