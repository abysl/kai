# Kai implementation rules

Audience: coding assistants and contributors making changes after reading
[Contributing](CONTRIBUTING.md) and [development](wiki/development.md).

## Boundaries

Kai renders accepted state and emits requests. Never mutate game state from
a click, drag, animation, or UI handler. Simulation, sessions, and wire formats
belong in Agni. Read [table design](wiki/design/table.md) before renderer work
and [multiplayer](wiki/design/multiplayer.md) before session integration.

Keep `src/lib.rs` to module declarations and public re-exports.
Keep `src/main.rs` as the startup shim; application startup belongs in
`src/app.rs`.

Code carries no comment lines. Explain APIs and design constraints in the wiki.
Keep functions small and names precise.

## Platforms

Do not assume networking has one native-only feature gate. Some modules compile
on Android, desktop, and WebAssembly through different implementations.
Use the existing platform abstractions and check each affected target.

Clipboard users call `os::clipboard::{read, write}`, not a platform library
directly. Text entry on Android includes a custom IME bridge; do not remove it
because desktop input works. Bevy's Android activity feature and the Android
activity dependency must be updated compatibly.

Portable stores, keys, and settings must use platform-owned paths. Do not bake
a contributor's home directory or private infrastructure into code or docs.

## Builds and versioning

Cargo.lock pins Git dependencies. Kai still needs a sibling Agni checkout
for included pool data and module-build helpers. Align it with the lockfile
using `ci/agni-revision.py`; do not assume arbitrary Agni main is compatible.

Every change that ships a new application build bumps the package version.
Documentation-only changes do not. Wire protocol and module-state versions
are separate and change only with their respective contracts.

When a wire format changes, coordinate compatible clients and golden fixtures
in Agni. Do not regenerate goldens just to silence a failure.

## Verification

Run `bash ci/check.sh` and treefmt as documented in the development guide.
The fast check runs the network-default tests and compile-checks all targets;
it does not execute Kai's full application test suite.
Run relevant unit tests and manual checks separately.

Test UI changes at narrow and wide viewports and with pointer/touch input.
Use the screenshot harness with synthetic or appropriately licensed content.
Test host/client visibility for hidden-card changes.

Do not commit game artwork, personal stores, credentials, internal deployment
configuration, or generated build output. The Markdown under Agni's rules
pool is parsed as data; a prose-formatting pass must not rewrite it.
