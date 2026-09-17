# Multiplayer integration

Audience: contributors working on joining, reconnecting, or session UI.
Read [Kai architecture](../architecture.md) first.

## Layers

The opt-in two-player queue lives in `src/net/matchmaking.rs`. It publishes a
versioned Agni matchmaking ticket through Spirit's existing table gossip and
uses Agni's authenticated reservation protocol. Ordinary invitations do not
use the queue. See [Agni's protocol design](https://github.com/abysl/agni/blob/main/wiki/design/matchmaking.md).

The fingerprint includes game, two-player count, wire version, full table
configuration, and actual engine/plugin pins. Riftbound's omitted defaults and
explicit defaults normalize to the same settings; unknown options remain part
of the digest. Decks and player names are excluded. Queue settings are fixed
until cancellation. Auto-dealing waits for the match to complete, and the
chosen deck is retained when a waiting host becomes the guest.

The joining peer waits for `HostClosed` before requesting its reserved host,
so the old table's close event cannot reset the new client session. A welcome
must match the expected configuration and have two seats before it is accepted.
Cancellation invalidates the network attempt generation and pending welcome.
Leaving the lobby cancels the queue; a completed match uses ordinary table
controls and keeps its admission restriction for reconnects.

Use `KAI_OPEN=matchmaking` for an empty free-form lobby in screenshot checks.

Browser hosting waits for the bundled engine and the selected game's bundled
plugin downloads to finish (or fail after 20 seconds each). A valid bundle takes
precedence over a gateway module in either download order. One game's completed
download cannot unlock another game's pending plugin. Gateway fallback remains
available when the bundle fails. This
keeps released web and Android clients on the same engine fingerprint; an
older gateway copy must not replace the engine shipped with the app. Existing
table sessions keep the engine instance pinned when they were created.

Kai owns the interface and routes intents. Agni owns host/client sessions,
the ordered game log, and versioned messages. Spirit supplies the shared
network endpoint and content exchange. Change a session rule in Agni rather
than reproducing it in Kai.

The host validates and sequences game requests. Clients render accepted views.
A host is a participant with authority, not a neutral trusted third party.

## Joining and module pins

An invitation provides connection information. Before a join can replay the
game, the client needs the exact engine and plugin selected for that session.
Modules are checked by their pinned hashes. The host can serve those pinned
modules; a separate content service is not proof that the correct module loaded.

The wire protocol version is independent of the application version. A mismatch
must fail clearly rather than attempting to decode incompatible messages.

## Dealing a retained deck

A selected deck remains selected when the player opens another table. New
tables and accepted resets rearm auto-dealing; starting, joining and ended
roles cannot consume the request. The request stays pending until the player
has a host/client seat and the table's game matches the deck. An already-dealt
seat is left alone on reconnect. Matchmaking keeps the pending request until
its reservation is complete. This scheduling only emits a deal request; the
host still validates and applies it.

## Visibility and reconnect

A private hand needs seat-specific faces and public commitments, not a complete
face list hidden only by the UI. Review labels, previews, logs, and replay paths
when changing visibility.

Seat reclamation uses authenticated endpoint identity. Reconnecting with a
new identity is not the same as reclaiming the old seat. Do not count a reconnect
as a second join in the deterministic log.

## Browser differences

Browsers use their supported endpoint/runtime path and may need an HTTP gateway
for operations blocked by cross-origin restrictions. The public browser app at
<https://kai.rae.blue> supplies a same-origin `/gateway/` API. Other hosts can
provide that API at their own origin; clients do not need backend server addresses.

`src/net/defaults.rs` seeds public content peers by endpoint ID, without baking
in their hosting topology. Native builds can replace or disable those seeds
with `KAI_DEFAULT_PEERS`, as described in the development guide.

Clipboard access generally requires a secure context. Test browser behavior
independently from native behavior.

## Undo requests

`table/undo_batch.rs` counts presses of Shift+Backspace or Ctrl+Z and restarts
a one-second trailing debounce after each press. Counts are capped at the
history reported by the host. Text entry is excluded; R remains reveal.

`net/undo.rs` routes requests and votes without changing game state itself.
Agni owns checkpoints, revision checks, consent, and restoration. A rollback
frame supplies the retained log boundary and only the receiving seat's private
faces. Clients replay that prefix with their pinned engine/plugin, discard
optimistic intents and old face caches, and refresh the rendered table.

Wire version 7 adds undo status, request, vote, and rollback messages. Older
peers must update together. There is no automatic mixed-version fallback.

## Verification checklist

Use [connectivity tests](../../tests/connectivity/README.md) for supported
platform pairs. Also test refused joins, protocol mismatches, missing modules,
and reclaims. A two-client smoke test does not establish malicious-host
resistance, fairness, or recovery after every kind of failure.
