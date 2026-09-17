# KAI-01 reconcile review

Date: 2026-09-17
Card: KAI-01 on the Braindump Kanban board; track backlog at
`orgs/abysl/plans/backlogs/kai.md` in Atlas.

## Reconcile

`reconcile/player-deck-tools-chat` merges the preservation checkpoint
`097b565` (`feat/player-deck-tools-chat`, based on AI-setup main `68169f0`)
into playtest main `79c2c14` (the 0.21.0 release). The checkpoint commit,
the feature branch, and the main checkout were not modified; the merge was
built in a separate temporary worktree and lives as this branch only.

`Cargo.lock` was the only textual conflict, and pinning it correctly took
two attempts. The checkpoint's pin `324d5c0` sits on the agni chat line that
diverged from the playtest line at `696e1ba`: it carries the versioned
table-chat and public deck-search APIs this feature calls, but not the
matchmaking and personal-asset modules main's net code imports. Pinning
`324d5c0` failed `cargo check` with unresolved `agni_net::matchmaking` and
`agni_net::personal_asset`. The reconciled branch pins every `agni-*`
package to `8765cdf` — agni main, the first tip containing both merged lines
— which is also the revision of the existing sibling Agni checkout, so the
documented sibling rule already holds. `spirit-*` stays on main's newer pin
`df0d47c`; the checkpoint's spirit pin `f54d79a` is an ancestor of it and is
superseded. The lock was resolved with
`cargo update -p agni-core --precise 8765cdfd…`, never by hand.


## Superseded by main

The checkpoint's `src/table/auto.rs` trigger-order fix (`ORDER_TRIGGERS_WHY`
forced-index escape and the dusk-rose/temporary test) is textually identical
to the fix main already landed through `fix/trigger-order` (`1b2fc29`,
0.18.1). The merge keeps main's version; nothing from the checkpoint was
dropped. Matchmaking, phone trash and Reflow actions, wire 10, and the ABI
bumps were already on main and required no action.

## Kept from the checkpoint

Table chat routing (`src/net/chat.rs`, `HostState::publish_chat`,
`SessionInfo.chat`, the drawer chat tab), shared deck actions
(`src/deck/actions.rs`), the search/import service (`src/deck/service.rs`,
the `ImportPanel.search` panel), model-facing deck tools (`src/ai/decks.rs`,
the `deck-action` command and tool set), AI seat drafts and chat feed
(`src/ai/driver.rs`), browser-local execution paths, and the
`plans/player-tools-chat/` plan and checkpoint review.

## Validation

- `nix-shell ci/format.nix --run 'treefmt --ci'`: passed, zero changes.
- `bash ci/check.sh` in the devenv shell: passed — the network-default
  origin tests, the undo batch tests, and
  `cargo check --locked --all-targets --features headless` completed clean
  against the `8765cdf` agni pin with the merged chat and deck-tool code.
  The temporary worktree needed a sibling `../agni` path for the importer
  testdata `include_str!`s; it was provided read-only as a symlink to the
  existing clean Agni checkout at the matching revision.
- `ci/agni-revision.py` cross-check: the sibling Agni checkout
  (`orgs/abysl/projects/agni/agni`, clean at `8765cdf`) matches the new
  lock pin without any checkout change.


## Not done here

- No merge into Kai main, no push, no submodule pointer move in Atlas.
- Full application tests, WebAssembly and Android builds, and manual UI
  verification remain open with the feature plan; KAI-02 finishes them.
