# Sol engine review

Implementation commit: `61811ffd`

## Completed

- Audited the emergency command before Riftbound enforcement for prompts, setup and lobby state, completed games, non-turn seats, repeated requests, and reset. Recovery preserves the table and bypasses automatic costs, scoring, draws, cleanup, and settlement.
- Fixed manual reveal-in-place from fully hidden decks. A plugin-created reveal debt now permits only the card owner's reveal entry to pay that debt; an ordinary direct reveal in a hidden zone remains invalid.
- Verified concealment clears public reveal state, private peek grants, pending reveal debt, in-place visibility, and the replicated face without changing card location, counters, or marks.
- Expanded native multiplayer coverage for private deck look, public reveal in place, concealment, deterministic shuffle, movement, counters, marks, turn edits, face delivery, and fresh replay.
- Added owner-only coverage for reveal, conceal, and token removal, plus empty and singleton deck recovery cases.
- Added a recovery regression through freshly built and hardened engine and Riftbound wasm modules, including private look, reveal, conceal, singleton shuffle, and fresh replay.
- Kept the existing session protocol unchanged. The engine and Riftbound plugin artifacts must ship together.

## Changed paths

- `orgs/andrea/projects/agni/agni/games/riftbound-turns/src/manual_tests.rs`
- `orgs/andrea/projects/agni/agni/net/src/host.rs`
- `orgs/andrea/projects/agni/agni/net/tests/riftbound_turns.rs`
- `orgs/andrea/projects/agni/agni/sim/src/log.rs`
- `orgs/andrea/projects/agni/agni/wiki/design/architecture.md`
- `orgs/andrea/projects/agni/kai/plans/manual-match/review/sol-engine.md`

## Validation

- `cargo test -p agni-plugin-sdk`: 42 passed.
- `cargo test -p agni-sim`: 70 unit tests and 4 golden tests passed.
- `cargo test -p agni-riftbound-turns`: 4,544 passed and 177 pre-existing tests ignored; 11 blob compatibility tests and 10 match-state tests also passed.
- `cargo test -p agni-riftbound-turns manual_tests -- --nocapture`: 8 passed.
- Native recovery regression `manual_recovery_search_shuffle_and_edits_replay_without_leaking_deck_faces`: passed.
- Hardened recovery regression `manual_recovery_reveal_conceal_and_shuffle_cross_hardened_module_abis`: passed after building and hardening both real wasm modules.
- The 34-test `agni-net` Riftbound integration file initially reported 33 passed and one stale hidden-affordance assertion. After correcting the helper, that exact hardened test passed, the other 33 tests passed together, and the expensive native/hardened replay-parity test had passed in the initial run.
- `cargo clippy -p agni-plugin-sdk -p agni-sim -p agni-riftbound-turns -p agni-net --tests -- -D warnings`: passed.
- `cargo build -p agni-engine-wasm --target wasm32-unknown-unknown --release`: passed.
- `cargo build -p agni-riftbound-plugin --target wasm32-unknown-unknown --release`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

## Limitations and handoff

- The 177 ignored Riftbound cases are the pre-existing engine-gap set; none were added or hidden.
- Existing matches pinned to older module hashes cannot acquire manual recovery from a renderer update. New engine and Riftbound plugin artifacts must be deployed together.
- No Kai UI, credential, desktop, browser, Android, or visual runtime checks were performed in this phase; those remain Luna's scope.
- No Luna cross-boundary code change is required by the engine fixes.
- Wasm build products remain only in the isolated target directory and are not tracked or published.
