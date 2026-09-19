# KAI-02 implementation review

Date: 2026-09-19
Card: KAI-02 on the Braindump Kanban board.

## Inputs and isolation

- KAI-01 was accepted in the Kanban supervision review. Its published Kai
  reconcile commit `1317cf0` contains owner checkpoint `097b565` without
  changing the owner checkout.
- GAME-02's cross-platform baseline was accepted separately. Its three harness
  defects were repaired by reviewed Kai commit `42fc703` on
  `review/kai-05-harness-repair`.
- This branch was created from `1317cf0` in a separate Kai worktree, then merged
  `42fc703` as `df914d7` before feature validation.
- A separate sibling Agni worktree was detached at `8765cdf`, the exact revision
  in Kai's unchanged `Cargo.lock`. Existing Kai, Agni, and build worktrees were
  not reset or reused.

## Completed behavior

- Human and model deck operations share the existing draft edits, catalog,
  importer, legality report, persistence, and export rendering. Native model
  tools perform supported-site search and import directly; browser tools use
  the same bounded gateway requests and validate response shapes.
- Model deck selection now completes the preserved `next_deck` path. Selection
  during an active game leaves the current deck untouched and stages the draft
  for the host's next-game boundary. Selection before play or after a winner
  requests an ordinary host reload. Invalid free-table drafts require explicit
  confirmation and rules-enforced tables refuse them.
- Chat requests carry no caller-supplied seat. The host derives the sender from
  the joined connection, validates Agni's 2,000-byte contract, permits five
  lines per seat per five seconds, assigns monotonic IDs, and broadcasts the
  result. Clients retain 100 ordered lines and queue at most four outgoing
  messages. Local model seats receive and reply through the same connection.
- Player and design guides now describe session lifetime, provider disclosure,
  search boundaries, authentication, limits, and next-game deck staging.

## Verification

- `nix-shell ci/format.nix --run treefmt` and `treefmt --ci`: passed with no
  remaining formatter changes.
- `bash ci/check.sh` in the devenv shell: passed all ten focused tests, the
  merged driver path behavior test, and the locked all-target headless check.
- `cargo test --locked --lib`: passed after building the required sibling Agni
  modules. The initial run had 663 passes and six fixture-dependent failures
  because `assets/plugins/riftbound.wasm` had not been built; after
  `plugin-build`, the final run passed 670 tests with zero failures and two
  intentional ignores: the live paid-model test and live token-art test.
- `devenv shell -- web-build`: passed with the reviewed pinned wasm-bindgen
  toolchain and produced a local 64 MiB ignored `web/dist` tree plus hardened
  engine, Riftbound, and MTG modules.
- `cargo check --locked --target wasm32-unknown-unknown --lib`: passed after the
  final browser response-parser refactor.
- Focused browser gateway parsing tests accept the documented result shape and
  reject error, missing-result, and invalid-JSON responses.
- `git diff --check`: passed. `Cargo.lock` and the Agni worktree are unchanged.

## Release handoff

The code and documentation are ready for human review. The application version
was not changed because this task explicitly forbids lock updates and Kai's
package version is mirrored in `Cargo.lock`. The release owner must authorize
and perform that coordinated version/lock change before shipping.

No branch was pushed or merged, no Atlas gitlink moved, no deployment or remote
workflow ran, no secret changed, and no live cache was published. Paid-provider,
physical-device, touch, and deployed-browser interaction remain KAI-03 checks.
