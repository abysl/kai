# Player tools and chat checkpoint

Saved the existing in-progress work on `feat/player-deck-tools-chat` at the
owner's request to clear uncommitted work. This is a preservation checkpoint,
not acceptance of the feature or approval to merge into main.

The checkpoint includes shared deck actions and search/import services,
model-facing deck tools, chat routing, UI integration, the existing Agni lock
revision, and the feature plan. Treefmt normalized two files; no additional
feature implementation was performed during this checkpoint.

## Validation

- Credential-pattern scan of changed and new files: no candidates found.
- `git diff --check`: passed.
- `nix-shell ci/format.nix --run 'treefmt --ci'`: passed after normalizing
  `src/deck/actions.rs` and `src/table/auto.rs`.
- `bash ci/check.sh`: the three network-default tests and three undo tests
  passed. The subsequent Cargo check stopped in `alsa-sys` because
  `pkg-config` was unavailable in the invoking shell. Compilation is unverified.
- Full application tests, WebAssembly/Android builds, and manual UI/network
  verification were not run. The feature plan remains open.

The lockfile pins Agni revision `324d5c031cfcf621e0fb11a258ac4b2ca9fda260`.
Use a compatible isolated sibling checkout and the documented native build
prerequisites when continuing validation. Preserve other active worktrees.
