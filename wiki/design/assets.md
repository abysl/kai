# Assets over the mesh — fetch once, serve peer to peer

> Status: built 2026-09-12 across spirit-node, agni-importers and kai.

Every image kai shows is a blob in the spirit store: card art (the
`riftbound-images` and `mtg-images` journals), the official card backs
(`card-backs`) and playmats (`playmats`). Until now each node fetched each of
them from the CDN on its own, and the browser build could fetch playmats not
at all. Now a node asks the mesh first and the URL is the last resort.

## The index

Each node publishes one ref named `assets` (`spirit_node::assets`): a
canonical-CBOR `{kind: "assets", refs: [blob hashes], entries: {key → hash}}`
map. Keys are `<journal>/<name>` with the name lowercased
(`agni_importers::art::asset_key`), so `playmats/playmat:akali` and
`riftbound-images/unl-082-219` name the same thing on every node. The gateways
add every image of their full riftbound manifest to their index. Because the
map declares its blobs as `refs`, the mesh advertises it like any other ref
(`held/total` in the gossip view) — nobody `--want`s it, though; the index is
read per key.

## The lookup

`ensure_from_mesh(store, journal, name)` in kai's art worker (native and
Android) and `asset_gateway::decide` in the deck gateway (for browsers) do the
same three steps:

1. the local journal — held, done;
2. the peer indexes: pull any `assets` manifest a known peer advertises that
   this store lacks (small, a few KB), then `Mesh::find_asset(key)` — held
   locally under another journal, link it; listed by a peer, pull that one
   blob from that peer with `mesh::fetch_blob` and link it;
3. only if no known node lists the key, fetch the URL once (`fetch_one`),
   then republish this node's index so the next asker gets it from us.

The kai worker waits for the pull (`INDEX_WAIT` 4 s per index, `BLOB_WAIT`
12 s per blob) because it runs on its own thread. The gateway cannot block an
HTTP request on the mesh, so it queues the pull (`Mesh::request_blob`) and
answers `202 {pending}`; the browser retries every 3 s until `200 {hash}` and
then fetches `/gateway/blob/<hash>` as it does for card art. A URL the gateway
would fetch itself must sit on `art::ASSET_HOSTS` (Riot's CDN, Scryfall).

## Reaching the peers

The client no longer bakes gateway URLs: `net::defaults` carries the two node
ids and every node, desktop or browser, dials them by id through iroh's
discovery and relay. The gateways pin their QUIC port (`SPIRIT_QUIC_PORT`,
`spiritGateway.quicPort` in the NixOS module — 4433 on dev1, 4434 on dev2)
and the router forwards those UDP ports (`openwrt/features/11-spirit-quic.sh`),
so a node off the tailnet dials them directly rather than only through the
relay.

## Not yet

- A browser node holds its blobs in memory and does not serve them; it is a
  consumer through its gateway only.
- The full-set download still ingests from Riftcodex; with the gateways'
  indexes in reach the per-card path already comes from the mesh, so the
  download is only a way to pre-warm.
- Indexes are pulled from every known peer; with many peers that is many small
  pulls per miss. A merged, gossiped summary is the next step if it shows.
