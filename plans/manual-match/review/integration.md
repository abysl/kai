# Final integration review

## Release follow-through

The user authorized committing, pushing and verifying the deployment after this
review. The release is version 0.16.0. The connectivity workflow's stale
pre-move Kai paths were corrected so the existing deployment gate can run.
The integration and verification record below describes the pre-release review.

The first release pipeline exposed a missing `Conceal` arm in the plugin's
projection-test adapter; it now translates that effect and generates concealment
in randomized projection cases. The deployment gate also exposed Askama 0.16.1
canonicalizing its config path across Nix vendor symlinks, breaking UniFFI's
generated includes. A version-scoped packaging override preserves the absolute
lexical config path instead. No Spirit application or staged user code changed.
These release follow-ups are shipped as 0.16.1.

The full plugin view test also needed to expect the hidden emergency recovery
affordance. That assertion correction ships as 0.16.2. The full Agni workspace
test command passed with all importer features used by CI and explicit paths to
the prebuilt engine, Riftbound and MTG wasm modules, including all 34 multiplayer
replay tests. Whole-workspace Clippy with warnings denied also passed. The Linux
Spirit CI checks passed with the packaging override; local follow-up Nix tests
were limited by temporary disk space after successful dependency compilation.

Release 0.16.3 moves connectivity to the 64-core build runner and sets the
existing headless browser viewport input to 640x400. The earlier browser pair
advanced to turn 16 with scores 4–5 but reached its 1,500-second timeout on an
8-core runner with a load average over 20. Reducing renderer workload and avoiding
the datacenter build queue preserves the original full-game outcome, replica
agreement and timeout checks. This is not a skipped or weakened connectivity gate.

Askama's config path handling is documented in its
[derive source](https://askama.rs/en/v0.16.0/doc/src/askama_derive/config.rs.html);
the packaging override uses Crane's
[dependency patching hook](https://crane.dev/patching_dependency_sources.html).

Reviewed and integrated on 2026-09-14 into the main working tree. Main remains
uncommitted; unrelated working changes and staged changes were preserved.

## Reviewed changes

- Sol: `02cc3642a4fdb34d7d8d1a19d240671743812c77`, retained as
  `review/kai-manual-sol`.
- Luna: `81acc90bdf5737e352dade1f11ba933f57edcebb`, retained as
  `review/kai-manual-luna`.
- Both clean isolated worktrees were removed after preserving the branches.
- Reviewed all agent deltas, the manual command authorization and state bypass,
  host/replica routing, and the direct playing-HUD issue-report path.
- Accepted the hidden-deck reveal correction: only an owner-attributed reveal
  may satisfy a plugin-created reveal debt in a fully hidden zone. Ordinary
  unsolicited hidden-zone reveals remain rejected.
- Accepted UI selection cleanup, counter bounds, roster names and wrapping.
- No blocking code-review findings remain.

## Combined verification

- Kai native library suite: 610 passed, 1 existing ignored.
- SDK: 42 passed.
- Simulation: 70 unit and 4 golden tests passed.
- Riftbound: 4,544 passed, 177 existing ignored; 11 blob compatibility and
  10 match-state tests passed.
- Native and hardened-wasm manual recovery multiplayer regressions: both passed.
  The initial plain-shell run could not build wasm because `lld` was unavailable;
  rerunning with `AGNI_ENGINE_WASM` and `AGNI_RIFTBOUND_WASM` pointing at the
  freshly rebuilt project-environment artifacts passed.
- `devenv shell -- modules-build`: passed; local engine and plugin assets rebuilt.
- Combined `web-build` with the matching repository wasm-bindgen tool: passed,
  including wasm optimization, module hardening and browser asset assembly.
- Scoped `git diff --check`: passed.

## Delivery limitations

Desktop, browser and Android interactive playtests/screenshots were not performed.
No deployment, push, historical credential removal or credential revocation was
performed. The AI key is no longer embedded in current source; provider-side
revocation of the old key remains the account owner's responsibility.

Ship engine and Riftbound plugin artifacts together. Existing sessions pinned to
older module hashes do not gain recovery merely by updating the renderer.
