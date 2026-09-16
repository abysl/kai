# Multiplayer integration

Audience: contributors working on joining, reconnecting, or session UI.
Read [Kai architecture](../architecture.md) first.

## Layers

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

## Visibility and reconnect

A private hand needs seat-specific faces and public commitments, not a complete
face list hidden only by the UI. Review labels, previews, logs, and replay paths
when changing visibility.

Seat reclamation uses authenticated endpoint identity. Reconnecting with a
new identity is not the same as reclaiming the old seat. Do not count a reconnect
as a second join in the deterministic log.

## Browser differences

Browsers use their supported endpoint/runtime path and may need an HTTP gateway
for operations blocked by cross-origin restrictions. A gateway must be supplied
by the application environment; it is not a required private service.

Clipboard access generally requires a secure context. Test browser behavior
independently from native behavior.

## Verification

Use [connectivity tests](../../tests/connectivity/README.md) for supported
platform pairs. Also test refused joins, protocol mismatches, missing modules,
and reclaims. A two-client smoke test does not establish malicious-host
resistance, fairness, or recovery after every kind of failure.
