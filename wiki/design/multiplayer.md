# Multiplayer — Host-Authoritative Table Sessions Over iroh

Two or more copies of kai — desktop, android, or the browser build at
kai.rae.blue — share one table. The node that opens the table is the **host**: it
owns the canonical `agni_core::Table`, assigns seats, numbers every state
change, and is the only peer that mutates game state. Everyone else renders a
replica that only ever changes by applying the host's events. Seat agreement is
therefore trivial by construction — the seating a client renders came from the
host's `Welcome`/`Roster` messages and nowhere else, and on any disagreement
the host's word is the state.

## The layers

| Layer | Where | Knows about |
|---|---|---|
| identity + node | `kai/src/net/node.rs` | the one iroh endpoint, started at launch — blobs, gossip and table ALPNs on a single router |
| transport | `agni/net/src/table.rs` | iroh connections, framing — opaque byte payloads only |
| discovery | `spirit/node/src/mesh.rs` + `gossip.rs` | table adverts riding the gossip view |
| protocol + session logic | `agni/net/src/{proto,host,client,pins}.rs`, re-exported as `agni_net::session` (+ the fold in `agni/sim/src/log.rs`) | messages, seats, masking, seq — pure, compiled on every target, unit-tested |
| net-to-game bridge | `agni/net/src/bridge.rs` | the event queue and pump loops; no Bevy, no node registry — the caller supplies mesh, table protocol, endpoint, spawner |
| bevy systems + panel | `kai/src/net/mod.rs` | resources, egui — every target; the web host is gated by `host_block` until its engine (and plugin) are loaded |

The transport rides its own ALPN, `spirit-table/1`, separate from
`spirit-gossip/0` and iroh-blobs, per spirit's one-protocol-per-ALPN rule —
but all three ALPNs are registered on the ONE router of the ONE endpoint each
client binds at startup. spirit itself no longer knows the table protocol
exists: kai's `node.rs` passes a closure to `spirit_node::serve_with` (wasm:
`serve_in_memory_with`) that accepts `agni_net::table::TableProtocol` on the
router spirit builds. Frames are 4-byte big-endian length + payload; payloads
are CBOR.

## One QR: your identity

There are no per-feature tickets anymore. Each client's identity panel shows a
single QR — the `iroh_tickets::EndpointTicket` of its one endpoint. Scanning
(android) or pasting (desktop, which has no camera) someone's identity does
exactly one thing: it adds that peer to the gossip mesh (`Mesh::seed` accepts
endpoint tickets, blob tickets and bare node ids). Everything else follows
from membership:

- gossip introduces every peer to every other peer, so one phone scanning N
  desktops meshes all N devices together;
- card stores replicate automatically — the mesh pulls any ref a peer
  advertises, and `identity::watch_refs` re-deals when a set finishes syncing;
- open tables appear in everyone's multiplayer panel via table adverts.

## The browser peer

The wasm build runs the same spirit node — endpoint, router, all three ALPNs,
mesh loop — via `spirit_node::serve_in_memory_with`. spirit-node's native-only
pieces (fs store, gateway, CLI, tokio net/signal) sit behind a default
`native` cargo feature; the wasm configuration keeps endpoint + router +
gossip + mesh + table on `n0-future` spawns and an in-memory iroh-blobs
store. Differences from a native peer, all deliberate:

- **Relay-only transport.** Browsers speak WebSockets to an iroh relay; no
  direct paths exist and none are planned upstream. Today that is n0's public
  relays; a self-hosted iroh-relay is a later infra step.
- **The bridge is same-origin first.** Chrome's local-network-access
  policy refuses a fetch from a public page to a tailnet address unless the
  user grants a permission the page never asks for, so a deployed page
  first tries `/gateway/*` on its own origin — kai.rae.blue proxies it to
  dev1 (dev2 on failure) with a content-addressed cache for `/gateway/blob/`
  — and only then the pinned ts.net gateways, which still serve a page
  opened from a dev server. Local and IP origins skip the same-origin probe.
- **Zero-ceremony bootstrap.** Instead of scanning a QR, the browser seeds
  its mesh from the dev1/dev2 gateway node ids pinned in `bridge.rs` (they
  are real spirit nodes), dialing by node id over the relay; gossip
  introduces every other peer and every open table. Open page → meshed →
  tables appear. The art bridge also seeds whatever node id
  `/gateway/status` reports live (`node::seed_when_ready`), so a stale pin
  self-heals for tailnet browsers that reach the gateway over HTTP.
- **Reload-stable identity, one per tab.** The node key is generated on
  first load and persisted in localStorage, so a reload keeps the same node
  id. localStorage is shared by every tab of the origin, and two iroh
  endpoints with one key cannot both be reached — the relay routes the id to
  whichever registered last, so a second tab joining the first tab's table
  dials itself and hangs in "joining". The first tab therefore stamps a lease
  (`kai-node-lease`, a per-tab token from sessionStorage plus a timestamp,
  refreshed every 2 s); a tab that finds a live lease held by another token
  takes a key of its own from sessionStorage instead (`kai-node-key-tab`), so
  a second tab is a second peer, and a reload of either tab keeps the key it
  had. The join path also refuses to dial the node's own id outright
  (`net::SELF_JOIN`) rather than waiting on a connection that cannot open.
  The identity panel shows the browser's QR for others to scan; there is no
  in-browser camera scanning because bootstrap makes it unnecessary.
- **No store replication.** The mesh runs with replication off and an
  in-memory blob store that holds exactly one thing: the engine the browser
  is running (`node::serve_bytes`, fed by `engine::web` whenever it installs
  a module — the bundled `./engine.wasm` or the gateway's `modules/engine`).
  A browser host pins that engine into genesis like any host, and because
  the bytes sit in its store the iroh-blobs handler answers a joiner's fetch
  for them. Game faces arrive over `spirit-table/1` like any client; art for
  the solo deal keeps using the HTTP gateway bridge.
- **Hosting allowed, with a warning.** The browser hosts exactly as the
  desktop does: `bridge::start_host` opens the `TableProtocol`, sets the
  `TableAdvert` on the mesh and spawns the host loop — on wasm through
  `wasm_bindgen_futures::spawn_local`, so the loop's future is not `Send`
  there (`bridge::HostFuture` is the per-target alias) — and its watchdog
  ticks on `n0_future::time::interval`, which is `setTimeout` in the browser.
  `HostSession` folds with the same engine the join path uses
  (`crate::engine::hosting_engine()`), and on the web that engine must be a
  loaded module: `net::host_block` keeps the *host table* verb off with
  "engine.wasm is still loading" until `engine::web` has one, so a browser
  host never opens an unpinned table that folds natively while its joiners
  fold in wasm. The same gate covers the plugin — a rules-enforced Riftbound
  table needs `modules/riftbound`, which the wasm `modules::platform`
  takes from the page's own bundle (`./assets/plugins/riftbound.wasm`,
  fetched at boot and keyed by its blake3, so a joiner pinned to the same
  build never asks anyone either) and otherwise resolves from the gateway's
  module list (the newest held, trusted row of that name), prefetched the
  moment the list arrives; either way it compiles with
  `web::plugin_with_manifest` when the table opens; until the bytes are in,
  the verb's reason reads "rules enforced needs the plugin: fetching plugin
  modules/riftbound from the gateway…". Should `HostReady` still find no
  engine or plugin, `refuse_hosting` closes the table and keeps its reason
  on the status line — the `HostClosed` that follows only writes "table
  closed" over a table that was actually open. What the browser cannot do is keep
  bevy's frame loop running in a hidden tab, and `drain_net` — the system
  that folds client intents into the host log — runs inside that loop. So a
  hidden host tab pauses the table for everyone at it. Rae's call: the host
  is playing with the tab in front, so that is acceptable, and the opponent
  panel says it plainly — "keep this tab in front — the table pauses for
  everyone while it is hidden; on a phone, switching apps hides it". What a
  hidden tab does to the connections, read from the code rather than
  guessed:
  - bevy_winit on wasm runs `UpdateMode::Continuous` as `ControlFlow::Wait`
    plus `request_redraw()` after every update, and winit's web backend turns
    that into `requestAnimationFrame` (`platform_impl/web/window.rs`,
    `web_sys/animation_frame.rs`). Browsers do not fire animation frames for
    a hidden document, so `app.update()` stops on the frame after the tab is
    hidden and resumes on the first frame after it is shown. Winit's
    `visibilitychange` listener only reports `WindowEvent::Occluded`; nothing
    tears anything down.
  - iroh's tasks are not on that loop. The endpoint, the relay actor and
    the `spirit-table/1` accept handler are `n0_future::task::spawn`
    (`wasm_bindgen_futures::spawn_local`) tasks driven by WebSocket
    `message` events and `setTimeout`, and neither iroh, iroh-relay,
    n0-future nor ws_stream_wasm listens for `visibilitychange`. Inbound
    frames from joiners keep arriving: the accept handler pushes
    `HostEvent::Frame`, the host loop turns it into
    `NetToGame::PeerFrame` and it waits in `NET_EVENTS` until `drain_net`
    runs again. Nothing is lost; it is queued.
  - Timers are what degrade. A hidden tab's timers run at most once a
    second, and Chrome's intensive throttling (hidden more than five
    minutes, timer chain ≥5, no audio, no WebRTC — an open WebSocket is not
    an exemption) drops them to once a minute. iroh's relay actor pings
    every 15s with a 5s pong deadline and QUIC keep-alives every 5s against
    a 30s idle timeout (`HEARTBEAT_INTERVAL`, `RELAY_PATH_MAX_IDLE_TIMEOUT`);
    at one wake-up per second those all still fire on time, so a tab hidden
    for a few minutes only pauses. Past the five-minute mark the host's
    keep-alives and loss-recovery timers drift toward a minute apart while
    the joiners' clocks do not; a joiner's 30s idle timeout can then expire,
    it sees `Dropped { session over … }`, and the host's accept handler
    reports `HostEvent::Left` when it next wakes. The relay WebSocket itself
    tends to survive, because pongs arrive as events and any inbound frame
    resets the ping interval; if it does drop, the relay actor redials with
    its exponential backoff (10ms–16s) and the endpoint is never closed, so
    the host loop's `endpoint.is_closed()` watchdog stays quiet, the
    `HostSession` keeps every seat and the advert stays set.
  - On a phone, switching apps does not merely throttle the page; the Page
    Lifecycle API lets the browser freeze it, and mobile browsers do so on a
    background tab (the spec fixes no delay — Chrome documents it as
    resource-driven, iOS Safari suspends on app switch). Frozen means no
    timers and no event callbacks at all, so the relay socket goes silent,
    the relay server and every joiner's QUIC connection time out, and the
    return is a redial plus `HostEvent::Left` for each seat. Nothing in
    iroh listens for the `freeze` event, so no graceful close precedes it.
  - So the answer is: a short hide pauses the table, a long hide (minutes on
    desktop, an app switch on a phone) pauses it and then drops the joiners'
    connections. Both recover through the existing paths: the joiner's
    `Recovery::Rejoin` reclaims its seat by node id (`HostSession::join_as`
    hands back the same seat and its hidden faces), and the host never has
    to re-open anything — the table is still there when the tab comes back.
  - Two page hooks soften the edges (`net::page`). While hosting, the page
    asks for a Screen Wake Lock (`navigator.wakeLock.request("screen")`,
    called through `js_sys::Reflect` so browsers without it are simply
    skipped) and re-asks on `visibilitychange` once the document is visible
    again, since the browser drops the lock when the tab hides; so a phone's
    screen timeout no longer hides the host tab, only an app switch does.
    And `pagehide` calls `net::withdraw_table` — `TableProtocol::close`
    plus `Mesh::set_table(None)`, both synchronous — so a closed tab closes
    its joiners' connections at once and retracts the advert if one more
    gossip round gets out, rather than leaving a dead advert for the 150s
    expiry while dialers time out against it.

## Table discovery

A host does not hand out a ticket; it gossips. `View` carries an optional
`TableAdvert { name }` — the sender's own open table, never relayed on behalf
of others, so an advert is only ever heard from the host itself and its
provenance is the authenticated iroh connection it arrived over. Opening a
table sets the advert (`Mesh::set_table`); every gossip exchange thereafter is
the heartbeat. Closing the table clears it, and because clearing bumps the
view version, the retraction propagates within a round or two (~5-10s). A
crashed host stops heartbeating instead: peers expire an advert not refreshed
within 150s — chosen as two missed idle-recheck periods, since a quiet mesh
still re-exchanges views every 60s.

Joining dials the host's node id directly over `spirit-table/1`
(`table::join_via` on the shared endpoint, address from the mesh's known-peer
map). No pairing step, no typing: click join, get seated. A joiner missing
the genesis-pinned engine or plugin asks the mesh for the blob with the host
as the first provider to try (`Mesh::request_blob(hash, Some(host))`, the
host id taken from `Recovery::Rejoin`), and the wait is bounded: after
`FETCH_PATIENCE` (60s) without the bytes the join is refused with "pinned
module … not obtainable — neither the host nor the mesh served it", and the
next attempt starts the clock afresh. The `TableProtocol`
handler is always registered; when no table is open it refuses the connection
with close code 1 and reason `no open table`, which the joiner surfaces as
`host refused: no open table` — distinct from an unreachable host. The dial
itself is bounded by `table::DIAL_TIMEOUT` (15s); a host that cannot be
reached at all reports `host unreachable (dial timed out …)`. Either failure
before a first frame also drops the host's table row locally
(`Mesh::forget_table`), so a dead advert stops inviting repeat timeouts
instead of lingering until the 150s gossip expiry.

## The trust surface, stated plainly

Scanning someone's identity QR adds them to your mesh. From that moment they
— and everyone they are meshed with, since gossip introduces peers
transitively — can see your node's addresses, the names and completeness of
the card-set refs you hold, fetch those blobs, see your open table advert, and
join any table you host. There is no authentication, no invitation, no
per-table secret in this pass; the QR is the whole ceremony. That is the
deliberate trade for zero-friction pairing at a physical table. Group trust
(spirit's `core/wiki/design/groups.md`) is where a real boundary would be
built; do not mistake this for one.

## Wire protocol

Client → host: `Join { name }` once, then `Intent { Move { card, to, seat,
index } }` per drop. Host → client:

- `Welcome { seat, seq, roster, snapshot }` — your seat, the roster, and a
  full masked snapshot; `seq` is the last event folded into that snapshot.
- `Roster { roster }` — re-broadcast when anyone joins or disconnects.
- `Event { seq, event }` — `Dealt`, `Moved` or `Reset`, in host order.
- `End { reason }` — reserved; today a session ends by the link dropping.
- `Module { hash, total, offset, bytes }` / `NoModule { hash, reason }` —
  the answer to a client's `NeedModule { hash }`; see Pinned modules below.

Seats go in join order: host is seat 0 (player 1), first joiner seat 1, and so
on — unless the joining node already holds a seat, in which case it reclaims
that one and no `Join` is appended at all (see Reconnecting below). A joiner is
dealt a fresh hand from the host's store the moment its `Join` arrives, then
gets its `Welcome`; everyone else gets the `Dealt` event (masked) plus the new
roster. Because everything flows over a single ordered QUIC
stream per client, `seq` is a consistency check rather than a reordering
buffer: replicas ignore `seq <= last_seq` and apply anything newer.

## Pinned modules

A table's genesis pins the blake3 of the engine and plugin bytes the host is
actually running (`HostSession::with_engine` writes `blob:<hex>` refs into
`TableConfig`, `agni/sim/src/pins.rs`). A joiner must fold under those exact
bytes — `verify_engine_pin`/`verify_plugin_pin` refuse anything else — so
`modules::prepare_join` resolves both pins before the `ClientSession` exists,
and the join sits in `JoinModules::Pending` until they land.

Where the bytes come from, in order:

- **native** — the spirit store by hash (`BlobStore::get`), then a bundled
  module whose hash matches (`assets/engine/engine.wasm`,
  `assets/plugins/*.wasm` or the `AGNI_*_WASM` overrides). A corrupt store
  blob is a refusal, not a fallback.
- **web** — the engine already loaded from `./engine.wasm` when its hash is
  the pinned one, else the gateways' HTTP blob store (`/gateway/blob/<hex>`),
  which holds only the CI-published modules.
- **both, when the above miss — the host itself.** The joiner sends
  `ClientMsg::NeedModule { hash }` over the table link; `HostSession`
  answers with `HostMsg::Module` frames of at most `MODULE_CHUNK_BYTES`
  (256 KiB, well under the transport's 8 MiB `MAX_FRAME_BYTES`) for exactly
  the two hashes it pinned at genesis — kai hands it the bytes it loaded
  (`modules::serve_into` → `HostSession::serve_module`) right after
  `host_from_with` — and with `HostMsg::NoModule { hash, reason }` for
  anything else, including a pinned hash it never received bytes for.
  `ModuleInbox` (`agni/net/src/client.rs`) reassembles the chunks in order,
  caps a transfer at `MAX_MODULE_BYTES`, holds at most
  `MAX_OPEN_ASSEMBLIES` (two: engine and plugin) in flight, grows its buffer
  as chunks land rather than reserving `total` up front, and blake3-verifies
  the whole before it is handed to the join; a mismatch, a chunk out of
  order or a refusal ends the join with `JoinModules::Refused` naming the
  kind, the short hash and the reason ("pinned engine 21c55f3a unavailable
  — the host refused to serve it: …"), shown in the lobby status and by
  kai-cli as `join refused: …`. A native joiner also files what the host
  served into its store, so the next join of that table finds it locally.
  The mesh blob pull (`request_blob`) is still issued natively as a bonus,
  but nothing waits on it.

Both pins are asked for before either is loaded: `join_modules` fetches the
engine's and the plugin's state first, reports a refusal ahead of a pending
transfer, and only compiles once both have bytes — so the engine is not
recompiled on every frame while the plugin is still crossing, and the two
`NeedModule` asks go out together.

The fetch state machine (`modules::fetch::Fetches`, one per process,
shared by every platform) is what stops the failure mode this replaced: a
gateway 404 used to drop the hash from the in-flight set and re-request it
on every frame, forever, with "fetching pinned engine 21c55…" as the only
sign. Now a 404 (`gateway::FetchError::NotFound`) hands the hash to the
host, a transient gateway error (`FetchError::Other`) schedules one retry
after `RETRY_POLLS` frames, doubling up to `RETRY_CAP_POLLS`, and each
source is asked once per attempt, never once per frame. `Module` and
`NoModule` frames count only for a hash the joiner is currently waiting on
from the host (`Stage::Host`); anything else — a stray chunk, a frame for a
hash nobody asked about, a late answer after a reset — is dropped, so a host
cannot open assemblies or poison a hash the joiner never named. When the
host refuses a hash the joiner asked it directly, before the gateway probe
had landed, and a gateway has appeared since, the gateway is tried once
before the join fails ("… the host refused to serve it: … and the gateway
has no such blob"). The status line names the source it is waiting on ("…
from the host (262144 of 1004127 bytes)…"). A new join request or a
dropped link resets every stalled ask and refusal but keeps verified bytes,
so a rejoin of the same table is instant.

The join is not deaf while the modules cross. The host seats a joiner and
broadcasts to it from the moment it sends `Welcome`, so `Entry`, `Faces`
and `Roster` frames arrive while `prepare_join` is still `Pending`.
`PendingWelcome` (`kai/src/net/mod.rs`, shared with kai-cli) absorbs them —
entries and faces queued in arrival order, the roster replaced — and
`seat_replica` folds the welcome log and then replays the queue through
`ClientSession::apply`, so the replica's `next_seq` matches the host's when
it seats instead of refusing every later entry as `BadSeq`.

The host serves a `NeedModule` only from a seated connection (the seat is
taken before `Welcome` is sent, and a joiner asks only after it), and each
hash once per connection (`Conns::asked`, cleared when the peer leaves), so
a peer cannot turn 40-byte asks into unbounded copies of a 1 MB module on
the host's outbound queues; a repeat ask is ignored rather than refused, so
it can never poison a joiner. Serving is a deliberate widening of the trust
surface only in the sense that a table now hands out the very bytes its
genesis already names by hash; a peer that could reach the table could
already fetch those bytes anywhere they were published.

This is also why a dev build can host a browser: the desktop's locally built
engine and plugin are not on any gateway, and before this the browser's join
could never finish. The web build keeps the raw engine bytes beside the
compiled module (`web::engine_bytes`), so a browser host, when hosting lands
there, serves its engine through the same `serve_into`; it has no plugin
loading yet, so a browser-hosted table would pin no plugin until it does.

## Private hands

A hand's faces travel only to the seat that owns it. `mask_cards` strips
`face` from any `Hand`-zone card belonging to another seat before a snapshot,
`Dealt` or `Reset` leaves the host, so a client's replica holds face-less
placeholder cards for opponents' hands — enough to count card backs, nothing
to peek at. The renderer never spawns card entities for hidden cards at all
(`card_shown`); opponents' hands appear only as the card-back fans, now sized
by the replica's real per-seat hand counts. When a hand card hits a board the
`Moved` event carries the face (`reveal`), and once public it stays public in
replicas — matching how a physical table works. The host also refuses intents
that move a card out of, or into, a hand that is not the sender's seat, so a
client cannot force a reveal or stuff cards into someone else's hand.

## What the host trusts

Only seat ownership of hands. Board cards are anyone's to move — that is
deliberate; a paper card table works the same way, and kai renders while agni
decides. The full rules engine slot is still `route_drops`: solo applies
directly, host applies through `HostSession::intent` (the validation point),
clients only emit requests.

## Animation semantics for replicated state

Card entities are keyed by `CardId` and persist across events; an event that
moves a card only retargets its `Slot`, so the card animates from wherever it
currently is to the new slot — the same visual as a local move, correct under
seat rotation because slots are computed through `seat_center`/`seat_yaw`.
Three deliberate choices:

- **Clients predict their own moves.** `route_drops` applies a client's drop
  to the local replica immediately and sends the intent; the host's echoed
  `Moved` re-applies at the host's canonical index, so replicas converge even
  if the optimistic index was wrong. The client UI cannot produce an intent
  the host refuses (hidden hands have no entities to drag), so prediction
  cannot diverge silently.
- **Full rebuilds snap, incremental changes animate.** `Welcome`, `Reset` and
  a solo redeal bump `DealGeneration`, which respawns every card entity; those
  spawns carry `SnapToSlot` and are placed directly at their slot on the first
  frame instead of flying in from nowhere. A card that appears outside a
  rebuild — a reveal, a mid-game `Dealt` — spawns at its owner's hand anchor
  (rotated by the owner's seat yaw) and animates out, so an opponent's play
  visibly comes from their side of the table.
- **Handlers only flag what they mutate.** The net handlers take the Bevy
  `ResMut` wrappers themselves; passing `&mut T` reborrows through `DerefMut`
  and marks a resource changed even when untouched, which is exactly the bug
  that made every host event force-respawn all cards on clients (teleporting
  boards on seats 2+ while the host animated smoothly).

## Rendering a seat that is not zero

`MySeat` replaced the hardcoded `PlayerId(0)`: hand layout, hand scroll,
drop-to-hand mapping and card-back suppression all follow it, so player 3
sees their own hand at whatever seat they are viewing (billboarding from the
viewed seat is unchanged) and card backs for everyone else. `ViewSeat` starts
at your own seat after a `Welcome`. `PlayerCount` follows the roster while a
session is active and the tuning slider is ignored.

## Reconnecting, and reclaiming a seat

A session can end four ways from the client's side — the host closed the
table, the host's process died, the phone slept and the app suspended, or the
link simply dropped — and all of them arrive as `NetToGame::Dropped` or
`HostMsg::End`, land in `SessionRole::Ended`, and freeze the table. Before this
pass that was terminal: the multiplayer panel showed one label and offered no
way out, so the only exit was restarting the app.

`SessionInfo` now carries a `Recovery` alongside the role — `Rejoin { host }`
for a client, `Rehost` for a host that lost its own table — and the panel
renders it as a button wherever the ended state is shown. `Rejoin` records the
host node id at the moment the join is requested, which is enough to name the
table: one open table per host node. `bridge::request_join` is accepted from
`Ended` as well as `Solo`, and it clears the stale `ClientSession` before
re-dialing so nothing from the dead session applies to the new one. The
recovery button is also drawn in the solo panel, because a join that failed
before it seated drops back to `Solo` where the discovered-table list lives.

**The reconnect is a plain join.** It goes through `bridge::run_join`,
`ClientMsg::Join`, `HostMsg::Welcome` and `modules::prepare_join` exactly as a
first join does, so the genesis-pinned engine and plugin are still resolved by
hash and blake3-verified, and `verify_engine_pin`/`verify_plugin_pin` still run
before a single entry is folded. There is no fast path that skips them.

### Seat reclamation

Seats are assigned in join order and a seat, once in the log, is in the log
forever: `LogAction::Join` for an already-seated seat folds to
`FoldError::SeatTaken`. So reclaiming a seat is not a matter of re-joining it,
it is a matter of *not* joining it again.

The host needs to recognise the returning peer. It cannot take the client's
word for it — `ClientMsg::Join` is whatever the client says — so the identity
comes from the transport: `table::HostEvent::Joined` now carries
`Connection::remote_id()`, the endpoint id the QUIC handshake authenticated,
and `bridge` surfaces it as `NetToGame::PeerJoined { conn, peer }` before any
frame from that connection. `HostState` keeps the `conn → node` map beside its
`conn → seat` map.

`HostSession::join_as(node, name)` is then the seating call:

- a node already in `seat_nodes` gets its old seat back — the roster row's
  `connected` flips true, the name is refreshed, **no log entry is appended**,
  and the returned `Option<LogEntry>` is `None`;
- an unknown node falls through to `join`, and the new seat is recorded
  against that node.

The Welcome carries the whole log as always, so the returning client rebuilds
the table — including its own cards, still sitting where it left them. What
the log cannot carry is the seat's *hidden* faces, because they were never in
it: a `Deal` into an owner-visible zone logs card ids only, and the faces went
out once over `HostMsg::Faces`. `HostSession::seat_faces(seat)` recomputes that
debt from current state — cards owned by the seat, not in `revealed`, in a zone
whose visibility is `Owner` — and the host re-sends them on the Welcome. Zones
declared `ZoneVisibility::None` (a face-down deck) are excluded, so a reclaim
cannot be used to read a deck the owner cannot read either.

Because only the node that first took the seat can reclaim it, and because the
node id is the authenticated one from the iroh handshake rather than a claim in
a message, a second player cannot steal a seat by naming it. That is a real
boundary, and it is the only one on this transport — everything else about the
trust surface above is unchanged.

### Re-hosting a lost table

`NetToGame::HostLost` used to drop the `HostSession`, which destroyed the log,
the roster, the seat-to-node map and the dealer's card faces — the table was
gone even though every client still had a copy of the log. It now keeps the
session, clears the connection maps, marks every guest seat disconnected, and
sets `Recovery::Rehost`. **re-host this table** re-opens the transport and the
advert, and the `HostReady` handler reuses the session it finds instead of
building a new one, so the log, the seats and the card faces survive and every
player reconnects into the seat they had.

`HostClosed` — the deliberate **close table** — still discards the session.
Losing a table and ending one are different events and now behave differently.

The host loop's own ending is ordered, not raced. `close_host` drops the
outbound sender, and `run_host` treats that as "stop sending" rather than
"stop": it leaves the select, then keeps draining `TableHost::next` until the
event channel closes — which happens only once every accept handler has
reported `HostEvent::Left` for the connection `TableProtocol::close` just
shut, since each handler holds a clone of the sender. So every joiner's
`PeerLeft` reaches the app after `HostClosed` instead of being lost when
the `TableHost` was dropped mid-close; agni-net's `host_bridge` test asserts
exactly that sequence. The watchdog's `endpoint.is_closed()` exit closes the
protocol itself before returning, so it cannot leave that drain waiting on a
sender nobody will drop.

## Known limitations

- Host disconnect freezes the session (`session ended — table frozen`); no
  failover, no host migration. Closing the table from the host side drops
  every client the same way. Reconnecting is explicit — see below.
- One open table per host node; opening again replaces it.
- Anyone in the mesh can join an open table — see the trust surface above.
- A phone makes a fragile host. Android suspends or freezes the whole process
  when the app leaves the foreground or the screen sleeps — the tokio runtime,
  the accept loop and gossip all stop, while peers keep showing the advert for
  up to 150s and every join attempt times out. In-process mitigations shipped:
  the activity holds `FLAG_KEEP_SCREEN_ON` while a table is open (hosting keeps
  the screen awake), the host loop withdraws the advert if its endpoint closes,
  and joiners get honest, distinguishable errors plus local advert cleanup. The
  real fix — an Android foreground service keeping the node alive in the
  background — is not built; until then, a phone host must stay foregrounded.
- A mobile host that hops networks (wifi↔cellular) is dialed at whatever
  `EndpointAddr` the joiner's known-peer map last recorded; gossip refreshes it
  on direct contact, but a stale third-party relay of the old address can
  transiently clobber a fresher one, and a join dialed in that window fails
  until the next direct exchange.
- Hands are dealt independently from the host's store, so two players can hold
  the same card, and card art travels as JPEG bytes inside faces (a 7-card
  deal is a few hundred KB per joiner) instead of by blob hash.
- Solo-mode leftovers on seats beyond the roster are snapshot as-is; a client
  joining a table that was played solo with more seats may see stray board
  cards.
- A browser peer in a hidden tab stops rendering and folding; it catches up
  when refocused (frames queue in `NET_EVENTS`), but a long-hidden tab's
  relay connection may drop and surface as `session over` on return. A
  browser host pauses the whole table while hidden, and a long hide drops its
  joiners the same way — the browser section above has the full account.
- Whether any desktop/web/android pair can seat and finish a game is proved by the suite in [connectivity.md](connectivity.md).
