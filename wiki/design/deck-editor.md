# Deck editing

Audience: contributors changing deck authoring, import, or validation.
Read the [architecture overview](../architecture.md) first.

## Separate authoring from selection

The library lists saved decks and the current draft. The editor changes a draft.
The lobby's deck-selection sheet selects a deck for a seat. Do not duplicate
rename, deletion, or import flows in the selection sheet.

A draft can come from a saved deck, a seated deck, or a new list. Preserve its
origin so saving or leaving can do the right thing. Opening another deck over
an unsaved draft needs an explicit choice.

## Data and validation

The editor uses resolved card metadata and the game-specific deck model.
Deck construction is checked by Agni's Riftbound legality code, not by counting
visible UI rows. Missing metadata must be distinguishable from a proven illegal
deck; an incomplete catalog must not produce a false assurance of legality.

Import formats are parsed and rendered by the importer layer. Kai should choose
a format, report parse/resolution errors, and display the result rather than
implementing a second parser.

## Code map

Start in `src/deck/` for catalog, draft, persistence, and exchange logic.
The editor and library screens live in `src/menu/`. Shared deck snapshots
come from Agni. The repository still reads pool data from a sibling Agni
checkout; see [development](../development.md#related-repositories).

## Save and share are different

Saving stores a deck locally. Sharing renders an interchange representation or
link; it does not imply that the recipient has the same card images or catalog.
Report unsupported representation details rather than silently dropping them.

Deck identity is based on the list, not its display name. See
[deck history](deck-history.md) before altering persistence.

## Test a change

Cover new and saved drafts, replacing a dirty draft, importing invalid input,
missing card metadata, and round trips through supported formats. Check the
editor on a narrow screen and verify that keyboard focus and text entry work
on each affected platform. Do not include downloaded artwork in screenshots
or fixtures without appropriate permission.
