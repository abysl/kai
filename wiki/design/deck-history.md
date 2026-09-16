# Deck History — the decks you have played, kept by identity

> Status: implemented 2026-09-05. The store side is
> `agni_importers::deck::history`; the conversion, resource and menu are kai's
> `deck/history.rs`.

## The problem

A deck arrived by paste, link or code, got seated, and vanished when the app
closed. Playing the same deck again meant finding the list again. The obvious
fix — a list of saved decks — has one hard question in it: what makes two
imports *the same deck*?

## Identity is the list, not the name

A deck's identity is the multiset of `(zone, card key, count)` it holds, plus
the game. Nothing else:

- **not the name.** A label is display text, so it lives on the collection item
  where it can be edited without moving the identity — the same rule
  collections.md already states for playlists.
- **not the art.** Re-importing after card images land must not mint a second
  deck. `image_url` and display names are in the stored snapshot but not in the
  identity.
- **not the order.** Zones sort by name and cards sort by key before hashing, so
  a decklist pasted in a different order is the same deck.
- **not how it was written.** Two entries of `2 Ember Rune` fold into one count
  of 4, and an empty sideboard is the same as an absent one.

So `ci:<hash>` over `blake3(canonical(identity))` is the deck. Import the same
list twice and the second save replaces the first item rather than adding one.
Change a single count and it is a different deck, which is the honest answer:
a 3-of and a 4-of are not the same list.

## What is stored

Exactly the card-identity shape one level up:

```
ci:<deck>                          the normalized list — the identity
blob:<snapshot>                    the full snapshot: names and art too
(ci:<deck>, td:<kai-deck-import>) → blob:<snapshot>    signed by your DGID
col:decks/<game>                   an item per deck, label = display name
```

Re-importing the same deck with better art writes a new snapshot blob and a new
attestation against the *same* identity; resolution takes the newest held
claim, so the entry silently improves. This is the errata rule from
identity.md, applied to decks.

Because it is a collection like any other, a deck history replicates over the
mesh with no new machinery, and a deck saved by a peer you do not trust is
listed but never loaded.

## Where it appears

- **Decks tab** — every saved deck for the current game, with `seat` and
  `forget`.
- **Multiplayer tab** — the same list, compact, next to host/join, so the deck
  you are bringing is chosen where you choose the table.

Seating from history goes through the same `seat_resolved` funnel as a fresh
import, so art staging, face mapping and auto-deal behave identically.

## Saving from the deck editor

The editor ([deck-editor.md](deck-editor.md) §6) writes through
`history::store::{rename, replace, remember_as}`; the draft's origin decides
which:

| origin | list changed | label changed | what happens |
|---|---|---|---|
| `Saved(ci)` | no | yes | `rename(game, ci, label)` — one row, same identity |
| `Saved(ci)` | yes | any | `replace(game, ci, deck, "editor", label)` = `remember_as` then `forget(ci)` — the tile is replaced, not duplicated |
| `New`, `Pool`, `Import`, `Seated` | — | — | `remember_as(deck, "editor", label)` — a new row |
| any, *save as copy* | — | — | `remember_as`, the old row stays |

`replace` with an unchanged list is a plain re-save (the identity is the
same, so nothing is forgotten). An empty label is refused before anything is
written. Seating from the editor saves first with the draft label; after any
save the draft's origin becomes `Saved(new ci)` and it is clean.

## Limits

The browser peer has no local store, so it has no history; the menu is absent
there rather than empty. Labels are derived (legend, champion or commander,
else a card count) and are not yet editable — a rename is one `add` op with a
new label, which the collection already supports.
