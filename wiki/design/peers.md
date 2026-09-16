# Presenting connection information

Audience: contributors changing the peer/debug interface. Assumes the
[multiplayer overview](multiplayer.md).

A known peer, a reachable peer, and a currently connected peer are different
states. Discovery records can outlive connections. Do not label a peer online
because its identifier appears in a registry.

Connection statistics are cumulative per connection, not per device.
Accumulate deltas from each connection's previous sample. Adding the full
counter on every refresh double-counts traffic; replacing a peer's total with
one connection's total loses traffic when several connections exist.

Native and browser clients may observe different details. Show unavailable
information as unavailable, not as a fabricated zero or liveness result.
A cached address is not evidence that a new dial would succeed.

When changing the panel, test repeated samples, reconnects, simultaneous
connections, and a peer known only through discovery. Keep technical identifiers
in diagnostics rather than making users interpret them to join a game.
