# Peers — What The Debug Panel Can Honestly Show

Every kai client runs one spirit node from launch (`src/net/node.rs`) and shows
one identity QR (`src/net/identity.rs`); the android scanner (`src/os/android.rs`) feeds
every scanned identity into the same add-peer path, and the mesh does the
rest. The peer panel (`src/net/peers.rs`) answers the question "who did we
exchange data with, how much, and are they still there" on every platform.

The numbers come from `spirit-node`'s registry (`node/src/peers.rs`), not from
kai. That is deliberate: the CLI, the desktop app and the android app all
observe the same exchange, so the accounting belongs next to the code that
performs it, and kai only renders a snapshot of plain data.

## The two sides do not know the same things

This is the fact that shapes the whole panel.

The **serving** side owns an `iroh::endpoint::Connection` for every peer that
dials it. A `Connection` exposes `stats()` — QUIC's own `udp_tx.bytes` and
`udp_rx.bytes` — and `rtt()`. So the server knows real wire byte counts in
both directions, and a genuine latency estimate.

The **fetching** side has no such handle. `iroh-blobs`' downloader owns its
connections internally and does not hand one back. There is no public hook to
reach the QUIC stats of a download. So the fetcher can only count what it
actually stored: the manifest plus each blob it wrote. That is an accurate
*payload* figure, and it is strictly smaller than the wire figure the server
reports for the same transfer, because it excludes QUIC framing,
acknowledgements and any retransmission.

The panel does not paper over this. `wire_bytes_known` is false on the
fetching side, the wire rows are omitted, and a line says the counts are
unavailable there. A zero would have been a lie.

## Byte counting is delta-based, not last-write-wins

`Connection::stats()` is cumulative *for that connection*. A peer may open
several connections over a session, and may have more than one open at once.
The registry therefore keeps, per live connection, the last `(sent, received)`
pair it observed, and adds only the difference into the peer's running total.
Closing a connection drops its slot but keeps the total.

Summing the raw `stats()` values instead would double-count on every sample;
storing only the latest would erase every finished connection. Both are easy
mistakes and both are covered by tests.

## Reachability: what iroh 1.1 will and will not tell us

`Endpoint::remote_info(id)` returns addressing information for a *recently
used* remote — a list of `TransportAddrInfo`, each an address plus an
`Active`/`Inactive` marker. A `TransportAddr::Relay` is a relayed path and a
`TransportAddr::Ip` is a direct one, which is where the panel's
direct/relay/direct+relay reading comes from. The information decays: once a
remote stops being used, iroh drops it and the call returns `None`.

Two things do not exist in this version, and the panel is built around their
absence:

- **No enumeration.** There is no way to ask an `Endpoint` for every remote it
  knows. `remote_info` needs an id you already have. That is precisely why the
  registry tracks ids itself — it is the only list of peers that exists.
- **No liveness probe.** There is no ping, no probe, no reachability check.
  The only way to learn that a peer is up is to open a real connection to it.

So there is no online/offline flag in the panel, because iroh cannot support
one. What the panel shows instead is the honest decomposition:

| Reading | Means |
|---|---|
| `connected` | we hold a live connection right now — this *is* proof of liveness |
| `reachable` | no connection open, but an exchange completed within the last minute |
| `disconnected` | we reached them once; nothing has been heard for over a minute |
| `never reached` | we hold a ticket naming them and have never made contact |
| active addresses | iroh still holds a working path, direct or relayed |
| last activity | how stale everything above is |

"Connected + a 56ms rtt" is a real online signal. "Disconnected, last activity
40m ago" is not an offline signal — it means we do not know. Presenting that
gap as a red dot would have invented information.

**Amended 2026-09-05.** `connected` alone was too strict to be useful. Only
iroh-blobs connections were recorded, and they exist just while bytes move, so
a fully synced mesh showed every peer as `disconnected` forever while gossip
kept "last activity" at a few seconds. That reading was true and useless.

Two changes fix it without inventing a probe. Gossip is now wrapped in the same
`RecordingHandler` as blobs, so a gossip round is a recorded connection like any
other — it always was one, we simply were not counting it. And a completed
exchange inside the last minute now reads `reachable`: not a guess, but the
plainest statement of what we observed. Beyond that window the reading falls
back to `disconnected`, which still means "we do not know" and not "offline".

Peers silent for over an hour are pruned outright (`prune_dead`), so the panel
stops accumulating nodes that went away. A peer holding a live connection is
never pruned however old its last activity looks, and a seeded peer is never
forgotten by the mesh — it may be down for an hour and still be one we must
keep dialling.

**Amended 2026-09-07.** The hour never elapsed. Every registry edit refreshed
"last activity", including recording a failed dial, so a dead peer we kept
failing to reach looked freshly active each round. And a peer the mesh did
forget came straight back a minute later, because every live peer still listed
it in the peer table it gossips, and a gossip introduction counted as knowing
it again. Two rules now hold. Activity means the peer itself took part in an
exchange — a connection, bytes, a completed gossip round; a failed dial, a
secondhand mention or a pull error are not activity. And a pruned peer is
remembered as forgotten: hearsay about it is ignored, and it returns only when
it speaks to us directly, which an alive node does within a round because it
learns us from the same gossip. A seed is never forgotten and re-seeding clears
the mark. This is what lets the whole mesh converge on dropping a dead node
instead of passing it around forever.

A true probe is implementable — `Endpoint::connect` with a timeout — but it is
a full connection handshake, not a cheap ping, and it only works from a
process that still has a live `Endpoint`. The fetching side drops its endpoint
as soon as the fetch returns. That is why it is not wired up.

## Where the data enters

| Hook | Feeds |
|---|---|
| `RecordingHandler` wrapping `BlobsProtocol` | inbound connections, wire bytes, rtt |
| `fetch` before it dials | provider id and ticket, so unreachable peers still appear |
| `fetch` per stored blob | payload bytes |
| `refresh_reachability` | active/inactive addresses, direct vs relay |
| gossip merge | peers learned transitively, and who introduced them |
| mesh convergence | per-ref held/total counts and which peers are complete |

The recording handler is a plain `ProtocolHandler` that registers the peer,
spawns a sampler, delegates to the real `BlobsProtocol`, takes a final sample
and deregisters. `Router::builder(..).accept(alpn, handler)` accepts any
handler, so wrapping is transparent and `serve`'s return shape is unchanged.

## Provenance is worth showing

Since nodes gossip (see spirit's
[`gossip.md`](../../../../spirit/spirit/wiki/design/gossip.md)), a peer in the
list did not necessarily come from a QR someone scanned. Each row says which:
a scanned ticket, a node that dialed us, or an introduction by another peer —
named. That distinction is the difference between the star topology this
started as and the mesh it became, so it belongs on screen rather than in a
log.

The refs section above the peer list is the other half. Membership without
convergence is a mesh that knows about data it does not have, so each ref shows
complete, partial with counts, or missing, plus which peers hold a complete
copy. Green, amber, red respectively.

Failed dials are shown with a count and the last error. Gossip can introduce
two peers that still cannot reach each other — traversal is not guaranteed —
and that outcome must be visible rather than retried silently forever.

## Panel behaviour

It lives in the settings screen's multiplayer tab, reachable from the ⚙
button on every platform — android has no keyboard, so a key alone would have
made it unreachable there; F3 jumps straight to that tab where a keyboard
exists. While settings are open it re-reads the snapshot about once a second,
which is cheap: a snapshot is a clone of a small map, and the header's peer
count comes from the same snapshot.

The whole module is gated `not(target_arch = "wasm32")`. The web demo has no
p2p at all and must keep building without spirit-node.
