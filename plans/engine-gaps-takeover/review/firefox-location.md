# Firefox and location picker review

Firefox 155 was exercised at 1280x720 against the original `web/dist` bundle
using a local HTTP server and WebDriver BiDi. The first capture,
`/tmp/kai-firefox.png`, contains only the HTML loading quote. It was taken
before the wasm app created its canvas and does not show egui text.

The first local server also omitted response MIME types. After ten seconds
Firefox still had no canvas and the loading element remained, which identified
the harness failure as module loading rather than an egui font failure. The
server was corrected to return `text/javascript` for `hand.js` and
`application/wasm` for wasm resources. Firefox then created the canvas and the
menu screenshot `/tmp/kai-firefox-app.png` showed the egui labels and buttons
with readable text. Firefox emitted only WebGL deprecation and initialization
warnings; no font or canvas text errors appeared.

No CSS, font family, or atlas change is included because the reported Firefox
text defect was not reproducible after the bundle was served with its required
MIME types. A future report should include a post-boot canvas capture before
changing the loading page or egui styling.

The card action strip already received one chip per legal play or hidden
destination from the engine view, but its four-chip truncation could hide a
legal battlefield behind `more…`. Location chips now keep the whole set
visible, while ordinary inspect and table actions retain the existing
truncation. The hidden action list also preserves every engine-supplied hide
destination. The row wraps within the stage width so every destination stays
tappable on a narrow viewport. The regression covers two play and two hide
destinations and asserts that an unoffered destination is absent.

Validation performed:

- `git diff --check`
- Firefox WebDriver BiDi boot and post-boot canvas/text capture
- Rust formatting for the changed Kai modules

The native Kai workspace gate was started with
`CARGO_TARGET_DIR=/build/agni-takeover/kai CARGO_BUILD_JOBS=12 cargo check
--workspace --all-targets`. It reached dependency compilation and stopped in
`alsa-sys` because the shell lacked `alsa.pc`; no Kai source error was
reported. The partial cache was released for the integration gate.

## Follow-up visual gate

The current Kai source is `94c80d66` on `takeover/kai-visual`, after merging
integration `1b517611`. The native binary was built with:

```text
CARGO_TARGET_DIR=/build/agni-takeover/kai CARGO_BUILD_JOBS=12 \
  devenv shell -- cargo build --bin kai --features fast-compile
```

The hardened assets used by the native run were the gate artifacts:

```text
engine.wasm    33078d637890dcd948de04548b8543c40cfc2356d92f993ada9c21cbbc335ed1
riftbound.wasm a7d3a8147ee738a459bf6e96604a43086e3408a59560713d583b280d6879b3c3
```

The first current-source launch reproduced a real Bevy startup failure:

```text
error[B0002]: ResMut<kai::table::primary::TurnActivity> in system
kai::table::primary::primary_ui conflicts with a previous system parameter
```

`primary_ui` had both `Res<TurnActivity>` and `hud::Sender`, whose system
parameter already contains `ResMut<TurnActivity>`. Commit `94c80d66` reads the
activity through `sender.activity` and adds
`primary_ui_system_initializes_without_duplicate_turn_activity_access`. The
focused test passed (`1 passed, 602 filtered`), as did native `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --all -- --check`.

The native two-client route also reached a real 800x360 table under Xvfb with
the current engine and plugin assets. Random autoplay repeatedly entered
reaction prompts before establishing two controlled battlefields, so those
frames are retained as startup/runtime evidence but are not claimed as picker
evidence.

For deterministic visual evidence, the test-only Bevy source at
[`kai-location-fixture.rs`](kai-location-fixture.rs) uses the actual shared
`chips::chips`, `chips::shown`, `chips::card_chip_text`, and
`chips::chip_widget` functions. Copy it temporarily to
`src/bin/kai_location_fixture.rs`, then build and run it with the same
`CARGO_TARGET_DIR` and llvmpipe/Xvfb environment as Kai. The source asserts
the five normal-card `(zone, hidden)` dispatch payloads
`[(1,false),(2,false),(3,false),(2,true),(3,true)]`, and asserts that the
already-face-down actions carry no destination. It renders at 800x360 with
normal-card legal choices `Base`, `Battlefield 1`, and `Battlefield 2`, hide
choices at Battlefield 1 and 2, and already-face-down `play from hidden` and
`reveal`; Battlefield 3 is intentionally absent. The captured image is
`/build/agni-takeover/captures/kai-location-fixture.png`. Remove the temporary
`src/bin` copy after the run; no production hook or fake gameplay path remains.

The existing pure dispatch regression
`table::chips::tests::every_legal_play_and_hide_destination_stays_clickable`
also asserts the rendered actions carry `[bf1, bf2, bf1, bf2]`, all are
enabled, all five stay visible, and the illegal third battlefield is absent.

The full current-source gate was run with both fresh asset overrides:

```text
CARGO_TARGET_DIR=/build/agni-takeover/kai CARGO_BUILD_JOBS=12 \
  AGNI_ENGINE_WASM=/tmp/agni-takeover/orgs/andrea/projects/agni/kai/assets/engine/engine.wasm \
  AGNI_RIFTBOUND_WASM=/tmp/agni-takeover/orgs/andrea/projects/agni/kai/assets/plugins/riftbound.wasm \
  devenv shell -- cargo test --workspace --all-targets --no-fail-fast
```

The first run exposed a vocabulary mismatch from the merged statics engine:
`answer_words(OrderTriggers)` had added `has Vision` and `has Weaponmaster`,
but Kai's act tool and system prompt only documented `trigger` and
`is Temporary`. The existing derived test correctly caught that omission.
`ACT_TOOL` and the system prompt now name all four trigger labels. The focused
test passed, and the full gate then completed with **602 passed, 0 failed, 1
ignored**. All runtime asset tests passed with the overrides.
