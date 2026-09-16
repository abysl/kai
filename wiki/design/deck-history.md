# Saved decks and identity

Audience: contributors changing deck persistence. Assumes familiarity with
the [deck editor](deck-editor.md).

A saved deck has both content and presentation. Content determines which cards
and counts belong to the list. Presentation includes its name and available art.
Renaming a deck or downloading a better image must not make it a different deck.

Agni's shared snapshot type defines the content identity. Use that definition;
do not invent a second hash over UI labels or serialized widget state.

Kai's deck storage and library UI keep saved rows, the in-progress draft, and
the list currently seated for play distinct. Editing a seated list should open
a draft rather than silently mutating the ongoing game.

When changing persistence, test save/recall, rename, deletion of a draft's
source, and old snapshots missing newly optional fields. A deck may remain
readable even if the current catalog lacks its art.

See `src/deck/` for storage and `src/menu/decks.rs` for library navigation.
