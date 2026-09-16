# Manual match recovery

Implement an in-match emergency switch that bypasses Riftbound decision and
cleanup logic, preserves the physical table, and records the switch for all
players. Any seated player can disable enforcement after an explicit local
confirmation; it remains disabled until a new game. Preserve the existing
consensual free-table proposal for compatibility.

Provide a touch-accessible manual panel for scores and all declared counters,
card movement and ordering across zones, ready/exhaust and status marks,
hidden movement and reveal, private deck search/look, shuffle, turn tracking,
battlefield control, and token removal. Existing token creation remains in the
tokens drawer. Keep hidden information private and route mutations through the
host and deterministic log.

Reference: [Riftatlas simulator](https://riftatlas.com/play),
[deck look controls](https://riftatlas.com/changelog/2026/week/31), and
[manual public-zone controls](https://riftatlas.com/changelog/2026/week/28).
Use its manual-table approach as guidance; this task does not add timeline undo.

Also remove the embedded NanoGPT credential and fail clearly when no
`NANOGPT_API_KEY` is configured, preserving credential-free canned/random seats.

## Implementation status

The reviewed implementation is present in the working tree: an emergency command
handled before Riftbound rules, persisted manual state, a capability-gated manual
table-menu panel, a direct HUD bug icon, environment-only AI credentials, and
regression tests. These changes have not been merged or published. Native Kai
compilation passed. The first full tests found assertions that needed the new
hidden recovery action and reserved bug-icon slot; those have been updated.

Pre-existing incorrect relative include paths in the pool loader and test
fixtures were corrected so Kai can compile and run its existing tests.

## Delegated implementation and verification

Delegation started from checkpoint `76a2fd6acb919454ab2cbae85a869af83e1bfcb6`
on `work/kai-manual-baseline-20260914`:

| Phase | Model | Agent | Isolated branch |
|---|---|---|---|
| Engine/privacy/replay | Sol, high reasoning | `01a0a2fe-a12d-7950-92d7-e4cb783ffae7` (Hegel) | `work/kai-manual-sol-20260914` |
| Player UI/credentials/reporting | Luna, high reasoning | `01a0a2fe-a1db-77b0-8740-676ed1ed794c` (Franklin) | `work/kai-manual-luna-20260914` |

Both agents completed their committed reviews. Their changes have been reviewed
and integrated into the main working tree, preserving unrelated changes and the
existing index. Commits are retained on `review/kai-manual-sol` and
`review/kai-manual-luna`; their clean temporary worktrees have been removed.
Nothing has been pushed or deployed. Combined verification is recorded in
`review/integration.md`.

Baseline verification before delegation (2026-09-14): Kai native compile passed;
Kai 606 tests passed / 1 ignored; SDK 42 passed; simulation 69 passed;
Riftbound 4,541 passed / 177 ignored. The delegated and combined verification results are recorded in the review files. No test failure is being concealed
by these ignored counts; they are the suites' existing ignored cases.

### Phase 1 — Sol: engine, privacy and replay completion

Complexity: high. Scope: the sibling `agni/` project only, plus
`plans/manual-match/review/sol-engine.md` in this project.

- [x] Audit emergency recovery before the rules engine: blocked prompt,
  setup/lobby, won game, non-turn player, repeated requests, and reset. Preserve
  physical cards/counters/score and ensure no rules settlement runs afterward.
- [x] Complete and run the native host/client recovery regression. Verify
  private deck looks, reveal in place, conceal, shuffle, counter/card edits,
  fresh replay and face delivery. Fix any failures.
- [x] Validate the new `Conceal` effect against SDK, simulation and module ABI
  behavior; it must revoke visibility without moving or removing a card. Audit
  reveal/conceal and token-removal authorization and singleton/empty decks.
- [x] Run SDK/simulation/Riftbound tests and build engine plus plugin for wasm.
  Exercise the recovery sequence with hardened wasm modules if feasible. The
  engine and plugin must ship together; avoid a session wire bump unless the
  actual session protocol changes.
- [x] Update engine docs and write the review summary with exact validation,
  unresolved limitations and changed paths. Commit to the isolated branch.

### Phase 2 — Luna: player-facing controls and bug-report completion

Complexity: medium. Scope: this project's `src/`, README and table design docs;
do not edit the sibling `agni/` project. Review summary:
`plans/manual-match/review/luna-ui.md`.

- [x] Review the manual panel on desktop and 360 dp portrait/landscape layouts.
  Scores/counters, all zones, top/bottom/position, ready/exhaust/status labels,
  draw/look/search/finish-look/shuffle, turn/control and token creation/removal
  must be discoverable and touch-accessible. Repair concrete UI problems.
- [x] Keep hidden faces and opponents' private zones out of card names and
  selection. Test capability/session gating and verify new fields persist while
  editing. Use real roster names where possible.
- [x] Ensure the bug icon stays directly on the playing HUD, opens
  `https://github.com/abysl/kai/issues/new`, has an accessible name/tooltip and a
  phone-sized hit target, and never leaves or resets the session. Keep a menu
  link too. Verify the opener path on web and Android from existing integration.
- [x] Verify no AI API key is embedded or silently substituted; missing
  `NANOGPT_API_KEY` must fail before HTTP while canned/random opponents still
  work. Do not print credentials or rewrite history.
- [x] Run Kai native tests and browser compile (with existing devenv scripts).
  Record any actual runtime/screenshot checks separately from code inspection.
  Update user docs and commit to the isolated branch.

### Phase 3 — Orchestrator: review and delivery

- [x] Review both diffs and summaries; coordinate cross-boundary fixes without
  overlapping agent file ownership.
- [x] Create `review/<name>` branches from completed agent branches and remove
  clean worktrees. Do not merge to main or publish without the review workflow.
- [x] Report completed behavior, validation, module rebuild requirements and
  any remaining manual verification accurately.

## Runtime follow-up

Desktop, mobile and browser interaction/screenshot playtesting remains unperformed.
The layout review below was code/test based; successful builds are not runtime
validation. This integration is local and uncommitted on main.

## Validation

- Regression tests for emergency recovery from a blocked/won game, no automatic
  costs/scoring/draws after recovery, replayable manual operations, and private
  deck access.
- UI command/gating tests and native compilation; check browser compilation
  when the toolchain is available.
- Manual review on desktop and touch: disable during a prompt, repair a score,
  search and reorder a deck, move and mark cards, spawn/remove a token, then
  continue with another player.
