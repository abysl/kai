# Local personal Elo implementation review

Implemented in `cea68e8805a8f7bef5aa27e116d79760851514f3` on
`task/trusted-p2p-elo`, based on Kai `097b565`. Version is 0.18.1.
The handoff branch is `review/trusted-p2p-elo`; nothing is merged to main.

## Usage and behavior

Open **Settings → You → personal Elo**, enter the opponent's self-reported
pre-game rating, choose win/loss/draw, review the preview, and record locally.
Each player updates independently. Initial rating is 1200, K is 32, and changes
round to the nearest integer with halves away from zero. Signed integer opponent
ratings from -10000 through 10000 are accepted; estimates have no floor.

The latest 50 entries show opponent, outcome, before/after estimates, and change.
Undo restores the previous estimate and fills the form for correction. Correct
an older retained entry by undoing back to it and re-entering later results in
order. A baseline preserves progress beyond the retained window.

Native storage follows Kai's platform configuration directory; browser storage
uses localStorage. Failed saves preserve the in-memory estimate. Unreadable
history remains untouched and recording is disabled until loading succeeds.

Agni, gameplay, and networking are unchanged. No task-owned server/service or
verification artifacts were present in the clean starting repositories or the
earlier unmodified Agni task worktree. Unrelated existing networking is preserved.

## Verification

- `devenv shell -- bash ci/check.sh`: passed, including six standalone tests and
  the locked native library, binary, and test-target check with `headless`.
- Focused Elo, settings, and startup tests: **26 passed**, including ordinary
  win/loss/draw outcomes at equal/stronger/weaker opponent ratings, invalid input,
  native file reload, bounded history, undo/correction, failed record/undo saves,
  corrupt history, phone/desktop pointer and synthetic touch interactions, and
  existing settings layout walks. Commands used `cargo test --locked --lib --
  elo:: settings:: app::tests` with optimization disabled to shorten this local
  build: `CARGO_PROFILE_TEST_DEBUG=0`,
  `--config 'profile.test.package."*".opt-level=0'`, and
  `--config profile.test.opt-level=0` before `--`.
- Browser compile: passed with `RUSTFLAGS='--cfg getrandom_backend="wasm_js"'
  cargo check --locked --target wasm32-unknown-unknown --lib` in devenv.
- `nix-shell ci/format.nix --run 'treefmt --ci'` and `git diff --check`: passed.
- Built and launched the native app under Xvfb/software Vulkan with an isolated
  configuration directory. At 1280×800, recorded a win against 1600 and observed
  1200 to 1229 (+29), including the saved JSON. Closed and reopened at 360×800:
  1229 persisted. Undo restored 1200; correcting to draw saved 1213. Another
  restart retained 1213. Captured and inspected phone and desktop screenshots
  with synthetic data and no game art. Replaced an unsupported arrow glyph with
  plain text and enlarged the entry field after this visual check.

## Limitations

Android compilation was attempted but blocked by missing Rust target
`aarch64-linux-android` (`E0463`). No Android device or browser runtime storage
test was performed. Touch coverage uses synthetic egui events, not a physical
device. The full application test suite and P2P playtests were not run; this
feature does not touch sessions or the wire format.

One local estimate is shared across games. Clearing app/site data loses it.
Only the retained 50 entries can be undone. Use one instance/tab per store;
concurrent writers are not coordinated. Native atomic file replacement does
not promise power-loss durability. This is an honor-based estimate with no
verification, synchronization, server, or competitive-system scaffolding.
