# Luna UI review

Commit: bc001161

## Completed

- Opened the manual scores, turn, battlefield and deck groups on first entry, and opened the selected-card status group.
- Made manual card rows full-width and wrapping so long hidden/public labels remain usable in the 360 dp phone sheet.
- Used live roster names for score, turn, battlefield and destination-seat controls, with table-view and numbered fallbacks.
- Reset destination, target seat, position, face-down toggle and label draft when selecting another card; stale or inaccessible selections and removed destinations are cleared before rendering.
- Clamped counter edits to each declared minimum and maximum before emitting a delta.
- Kept the existing private-zone filtering, hidden-face naming, drawn bug icon, 48 dp HUD target, accessible label and GitHub issue URL unchanged.
- Documented the manual panel behavior and the existing egui URL opener paths for wasm and Android.

## Regression coverage

- Manual panel default look count and per-card state reset.
- Counter bounds.
- Roster-name preference and fallback labels.
- Stale manual selection cleanup.
- Existing manual capability, hidden-zone, hidden-face and move-intent tests remain passing.

## Checks

- `CARGO_TARGET_DIR=/home/rae/atlas/orgs/andrea/projects/agni/kai/target devenv shell -- modules-build` passed.
- `CARGO_TARGET_DIR=/home/rae/atlas/orgs/andrea/projects/agni/kai/target devenv shell -- unit-test` passed: 610 passed, 1 ignored, 0 failed.
- Targeted manual tests passed: 7 passed, 0 failed.
- `cargo fmt --manifest-path orgs/andrea/projects/agni/kai/Cargo.toml -- --check` passed.
- `PATH=/home/rae/atlas/orgs/andrea/projects/agni/kai/.wasm-tools/bin:$PATH CARGO_TARGET_DIR=/home/rae/atlas/orgs/andrea/projects/agni/kai/target devenv shell -- bash -lc 'export PATH=/home/rae/atlas/orgs/andrea/projects/agni/kai/.wasm-tools/bin:$PATH; web-build'` passed, including wasm-bindgen, wasm-opt, hardened engine/plugins and web asset assembly.
- Code inspection confirmed `NANOGPT_API_KEY` is environment-only, missing credentials fail before HTTP, and canned/random seats remain credential-free. No literal credential was found.
- Code inspection confirmed the HUD report action uses bevy_egui's enabled URL opener, with native Android `ACTION_VIEW` and wasm browser-tab implementations.

## Runtime validation and limitations

No desktop, Android, wasm-browser, or screenshot runtime validation was performed. The manual panel was reviewed by code and regression tests at the existing sheet/HUD layout contracts; the full browser build was compile/artifact validated only. The first plain-shell native test attempt failed before Kai compilation because `pkg-config` was unavailable; devenv resolved that. The first web artifact attempt used devenv's fallback wasm-bindgen 0.2.121 against the project's 0.2.127 output; rerunning with the repository's matching `.wasm-tools` passed.

## Changed files

- `orgs/andrea/projects/agni/kai/src/table/manual.rs`
- `orgs/andrea/projects/agni/kai/src/table/hud.rs`
- `orgs/andrea/projects/agni/kai/README.md`
- `orgs/andrea/projects/agni/kai/wiki/design/table.md`
- `orgs/andrea/projects/agni/kai/plans/manual-match/review/luna-ui.md`
