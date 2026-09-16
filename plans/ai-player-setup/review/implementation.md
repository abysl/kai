# AI player setup review

Audience: contributors reviewing the implementation or preparing a release.

## User-visible result

The table lobby opens an AI settings sheet before creating or adding a
model-backed player. It provides OpenRouter/NanoGPT selection, masked API-key
entry, live model discovery with search and dropdown selection, and a keyless
random player. Closing the sheet does not host a table or add a seat.

Browser hosting now includes the AI option and the chat drawer. Native and
browser seats continue to submit normal game requests through host validation.
The version is 0.18.0; there is no wire-protocol change.

## Privacy and correctness

- Credentials are session-only, redacted from Debug output, and passed only
  to the selected provider's fixed chat endpoint. Model-list requests do not
  carry credentials. Errors do not include raw response bodies.
- Confirmation requires a nonblank key and a model from the tool-capable
  catalog, unless random play is selected. Catalog discovery is not account
  authentication and does not prove that credit or model access is available.
- Browser model calls yield during HTTP and between commands. Late decisions
  cannot survive stop, replacement, reset or rollback. Undo consent suspends
  action execution; invalidation also drops unsent intents.
- The browser builds context from its own ClientSession replica. A regression
  test verifies that the human player's private face never enters its snapshot.
- A canned asynchronous decision sends a move, waits for the host's accepted
  frame, and stops issuing commands after cancellation. Tests make no paid calls.

## Verification

Native compilation, the fast PR check, WebAssembly library compilation and
treefmt passed. The native AI suite passed with the existing generated plugin
fixture supplied. Five browser-runner tests passed on the native test runtime,
including the asynchronous tool/host loop, hidden faces and cancellation.
The settings tests cover confirmation, navigation and phone/desktop widths.

The final broad library suite passed 627 tests with zero failures. The existing
live-provider test remained ignored, and the bundled-card-back test was excluded.

Rendered screenshots were inspected at 1280x800 and 360x800 in an isolated
graphical session with no credentials and an empty content store. API-key and
search fields have 48-point touch targets. Provider catalogs and chat CORS
preflights were checked without credentials or generation requests.

An initial broad test run encountered absent game-module fixtures and the
pre-existing bundled-card-back test. Supply the generated engine/plugin paths
for integration tests. The card-back test is excluded because its art files
are intentionally absent from the release checkout; no artwork was restored.

## Remaining release checks

No paid-provider gameplay session, interactive browser gameplay session, or
physical Android/touch-device test was performed. Browser execution is covered
by a WebAssembly compile check and native tests of the same async decision and
seat lifecycle code, not an installed-browser end-to-end test.

The change is committed locally on its feature branch, not pushed or deployed.
No private infrastructure, credentials, generated screenshots or game artwork
are included in the commit.
