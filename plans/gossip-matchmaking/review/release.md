# Matchmaking implementation review

Audience: contributors reviewing the v0.19.0 change.

Implemented the opt-in lobby queue, exact canonical settings comparison,
authenticated exclusive reservations, host election, expiring stale claims,
welcome validation, cancellation, and automatic retry after a failed join.
Deck auto-dealing waits until the match is ready. The chosen deck survives
the waiting-host to guest transition. Existing invitations remain separate.

The framework implementation is pinned to Agni `9b875a7`. Its session messages
and game wire version are unchanged. Spirit's gossip schema and public peers
need no change. Public source contains no new runtime assets or deployment
configuration.

Verification before release:

- Kai library: 636 passed, two existing live-service tests ignored, one
  bundled-art test excluded because its third-party assets are not distributed.
- Native all-target fast check and browser library compile: passed.
- Agni fast checks on the release dependency and main: passed.
- Three direct-network matchmaking tests and the existing host bridge test:
  passed, including third-peer gossip discovery, exclusive seating, identical
  replica logs, stale claims, and cancellation closing an active transport.
- Protocol state tests: contention, expiry, reconnect admission, fresh nonces,
  incompatible settings, and stale attempt generations passed.
- Two actual desktop instances discovered one another through the live mesh
  and automatically became a host/client table with two seats.
- Desktop 1280×800 and phone-sized 390×844 layouts inspected. The narrow-width
  UI walk includes the new opponent mode. Search, cancel, restart and Escape
  cancellation were exercised in a running phone-sized window.
- Both repositories passed treefmt.

The browser target was compile-checked; these local interactive checks used
desktop executables, not a physical Android device or browser gameplay.
