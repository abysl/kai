# Connectivity — Every Platform Pair Plays a Game

Two kai clients — any of desktop, web, android — must be able to find each
other, seat, deal, roll, and play a rules-enforced Riftbound game to a winner
with nobody at the keyboard. This page designs the suite that proves it: an
**autoplay mode** compiled into every kai build, one **driver** per platform
that launches a client with a plan and collects its events, and an
**orchestrator** that pairs a host with a joiner, waits for both outcomes, and
checks they agree. It is written so four people can build the four parts
without talking to each other; the interfaces between them are spelled out to
the function signature and the event name.

What the suite proves is the whole path of
[multiplayer.md](multiplayer.md) end to end on real builds: node bootstrap,
the table dial, `Welcome`, the pinned-module handshake (`NeedModule`), the
deal, the opening roll, every intent and every fold on both replicas, and the
winner. What it does not prove is rules correctness — the in-process soak
(`kai-cli soak`, README "Self-play soak") does that thousands of times faster
and stays the place for it.

## What exists, and what is missing

The pieces a scripted game needs are mostly there:

| piece | where | state |
|---|---|---|
| the random brain | `src/ai/random.rs` — `options(view, me)`, `pick(rng, options)`, `Rng::seeded`, `Choice::{Action, Move}` | pure over `PluginView`, compiles anywhere, but `pub mod ai` is gated `not(wasm32)` in `src/lib.rs` |
| the forced-pass automation | `src/table/auto.rs` `decide`, `auto_pilot` | every target, GUI only, never makes a non-forced move |
| an own-seat headless player | `src/bin/kai_cli.rs` over `ai::driver::Driver` + `BridgeLink` | a separate process with its own node; it owns a `ClientSession` replica, sleeps on `std::thread`, reads files — not the GUI's seat and not portable |
| an extra AI seat at the host | `src/ai/local.rs` | native only; it is a second seat, not the GUI's |
| the host/join calls | `net::host_table`, `net::join_by_ticket`, `net::rejoin`, `net::leave_session` | every target |
| the deck pin and the auto-deal | `deck::pinned::pin_decks` (`PinnedDeck.side`), `deck::import::auto_deal`, `deck::battlefield::needs_choice` | every target; a pool deck holds three battlefields, so the deal waits for `SeatedDeckRecord.battlefield` |
| the affordance sender | `hud::Sender::fire(&Affordance)` → `PluginActionRequested` → `net::route_plugin_actions` (host folds `own_intent`, client sends `ClientMsg::Intent`) — handles `Commit` affordances (the roll) through `RollSecrets` | every target |
| the card move | `CardDropped { card, to: Zone::Plugin(zone), seat, index: table.in_area(seat, to).count(), hidden }` → `net::route_drops` (as `chips::Act` builds it) | every target |
| the observables | `PluginPanel.view` (`PluginView { status, winner, prompt, legal, affordances, … }`), `SessionInfo { role, status, roster, recovery, options }`, `MySeat`, `toast::Refusals.log`, `ai::soak::turn_of(&view.status)` | every target |
| the screenshot harness | `KAI_WINDOW`, `KAI_SHOT` (`src/app.rs`) | native |

Missing, and built by this design: a way to *tell* a GUI client to host or
join (no CLI arg, env var, URL parameter, intent extra or JS export exists —
the lobby is egui only), a way for the client's **own** seat to play the
random brain, a machine-readable trace of what happened (desktop and android
log nothing for join, seat or winner; `SessionInfo.status` is egui only),
and the three drivers plus the runner around them.

## The autoplay mode

`src/autoplay.rs` is one module on every target — native, wasm, android —
with **no `cfg` in it**. It owns the plan, a phase machine driven by one Bevy
`Update` system, the event record set, a ring buffer of emitted events, and
the exit policy. The platform seams are outside it: who calls `install`, and
who drains the buffer.

### Why the GUI's own seat and not the Driver

The brief asked for "the same code the in-process seat and kai-cli use". The
*brain* is shared — `ai::random` is the one and only decision maker, seeded
the same way, so a plan's seed means the same thing in the soak, in kai-cli
and here. The `Driver`/`Seat`/`Link` machinery is deliberately **not**
reused: a `Driver` builds and folds its own `ClientSession` replica from
`NetToGame` events, which is exactly what `net::drain_net` already does for
the GUI's seat, and `BridgeLink` reads the same process-wide `agni_net::bridge`
statics as `drain_net` — two consumers in one process would each see half the
events. A `Driver` is the right shape for a *second* seat (kai-cli,
`ai::local`), not for steering the seat the window is rendering. Autoplay
therefore drives the GUI's seat the way a player's hand does: it reads
`PluginPanel.view`, asks `random::options`, and sends through
`hud::Sender::fire` or `CardDropped`, so every intent goes through
`route_plugin_actions`/`route_drops` and the host's validation exactly as a
click would.

### The plan

A `serde` struct, JSON on the wire. Every field but `role` has a default.

```json
{"role":"host","game":"riftbound","deck":"lillia-house","enforced":true,
 "brain":{"random":{"seed":7}},"until":"winner","timeout_s":600,
 "turn_cap":60,"battlefield":0,"pace_ms":300,"name":"host-desktop"}
```

```json
{"role":{"join":{"host":"<EndpointTicket or node id>"}},"deck":"irelia-house",
 "brain":{"random":{"seed":8}},"until":"winner","name":"joiner-web"}
```

| field | type | default | meaning |
|---|---|---|---|
| `role` | `"host"` \| `{"join":{"host": string}}` | required | host opens the table; join dials `host` — an `EndpointTicket` (preferred: carries relay + direct addrs, works offline and without gossip) or a bare node id (needs n0 pkarr DNS or a gossip introduction). Both go through `net::join_by_ticket`, whose `Mesh::seed` accepts either |
| `game` | `"riftbound"` | `riftbound` | the only game this round; `TableChoice.game` |
| `deck` | pool slug prefix (`pool::find`) | host `lillia-house`, joiner `irelia-house` — the same defaults `deck::pinned::default_for` uses | the pool deck this seat pins; written into `PinnedDeck.side` |
| `enforced` | bool | `true` | `TableChoice.enforced`; a plan with `false` plays a free table, where the random brain's moves are all accepted and the game ends only by `turn_cap` |
| `brain` | `{"random":{"seed": u64}}` | `{"random":{"seed":0}}` | the only mind this round. The seed feeds `Rng::seeded`; the roll is OS entropy, so two runs differ after the roll regardless |
| `until` | `"winner"` \| `"seated"` \| `{"turns": n}` | `winner` | the success condition — see outcomes |
| `timeout_s` | u64 | `600` | wall clock from `install` to `outcome`; overrun is `failed`/`timeout` |
| `turn_cap` | u32 | `60` | a game still running past this turn is `failed`/`turn cap` (the soak's default is 40; random games over a network run slower and this is a failure, not a stop) |
| `battlefield` | usize | `0` | index into the pool deck's battlefields (`SeatedDeckRecord.battlefield`) |
| `pace_ms` | u64 | `300` | minimum gap between two sent intents, on top of waiting for the fold |
| `name` | string | `"autoplay-<role>"` | the player name in `Join`/the roster; `install` sets `USER` to it (`std::env::set_var`, an in-process table on every target) so `net::player_name` reads it |
| `players` | u8 | `2` | how many seats must be connected before the deal and the roll |
| `exit` | bool | set by the entry point, not the JSON | whether the process exits after `outcome` (native yes, web and android no); a JSON value overrides the entry's default |

`autoplay::install(json, exit_default)` parses, rejects unknown fields
(`deny_unknown_fields`, so a typo in a runner is a `failed` at install and
not a plan that silently waits), and stores the plan in a static; it may be
called before `App::new()` (native, android) or from any later frame (web).
`AutoplayPlugin` is a no-op while nothing is installed.

### Phases

One `Update` system, `autoplay::step`, ordered `.after(auto::auto_pilot)` —
the tail of the `refresh_plugin_view … auto_pilot` chain, so it sees this
frame's view — `.after(net::drain_net)`, `.after(pinned::pin_decks)` (so the
frame a joiner lands in its seat, `pin_decks` has already applied
`side_after` and autoplay's `PinSide` is the last word before
`import::auto_deal` runs) and `.before(net::redeal_after_new_game)`, with its
intents routed by `route_plugin_actions`/`route_drops`
on the next frame, the same latency a click has. It reads
`SessionInfo`, `MySeat`, `PluginPanel`, `GameTable`, `SeatedDeck`,
`PinnedDeck`, `Refusals`, `Menu`, `Time`, and writes `TableChoice`,
`PinnedDeck.side`, `SeatedDeck.0.battlefield`, `Menu.screen`,
`Tuning.auto_pass` (through `bypass_change_detection`, so the scratch
`tuning.json` never learns it), `PluginActionRequested` (via `hud::Sender`),
`CardDropped`, and `AppExit`.

| phase | entered when | does | emits |
|---|---|---|---|
| `Booting` | install | wait for `node::get()` | `meshed` on the first `Some` |
| `Opening` (host) | meshed | `TableChoice { game, enforced, options: None }`; `Tuning.auto_pass = false`; `net::host_table(&mut info)`; `Menu.screen = Screen::Table` | `hosting` when `info.role == Host` |
| `Dialing` (joiner) | meshed | `net::join_by_ticket(&host)`; on `Err` → `failed`/`join: <reason>`; while `Joining` nothing is staged (`rules_enforced` is false until `Welcome` carries the options, and a deck seated then would deal twice); a `Solo` with `join failed: …` is retried every `JOIN_RETRY_MS` (3 s) up to `MAX_JOIN_RETRIES` (20), then `failed`/`join: <status>` | `joining` per attempt, then `seated` when `info.role == Client` (the `follow_session` rule already puts a joiner at the table) |
| `Pinning` | seated (host: hosting) | `PinnedDeck.side = Some(slug)` once `rules_enforced`; when `SeatedDeck.0` exists and `needs_choice`, set `record.battlefield = Some(plan.battlefield)`; `deck::import::auto_deal` then fires `DealDeckRequested` on its own | `dealt` when `pinned::legends_on_table(&table.0, my_seat).mine` is `Some` |
| `Gathering` | dealt | wait until `info.roster.iter().filter(|s| s.connected).count() >= plan.players`; `until: seated` ends here with `outcome seated` once every seat's legend is on the table (`legends.theirs.len() + 1 >= players`), so a host never closes the table under a joiner that is still dealing | `roster` on every roster change |
| `Rolling` | gathered | the lobby is a `PluginView` like any other: press through `random::options` with the **lobby filter** below until `turn_of(&view.status) == Some(1)` | `started { first, enforced }` |
| `Playing` | started | each tick: if `turn_of` changed → `turn`; if `view.winner.is_some()` → outcome; if `turn > turn_cap` → failed; else if no intent is in flight and `pace_ms` has passed: `options = random::options(&view, me)` minus the lobby filter; `pick`; send; remember `view` as `sent_view` | `sent` per intent, `refused` per own refusal |
| in flight | after a send | wait for `panel.view != sent_view` (the fold landed) or a new `Refusals.log` entry (refused; counted) or `FOLD_WAIT` 5 s (counted as a stall; three consecutive stalls → `failed`/`stuck`) | |
| idle | no option to pick | the view is remembered with the time; when it has not changed for `IDLE_FAIL_MS` (60 s) → `failed`/`stuck: no options and no fold for 60000 ms` (a prompt the brain cannot answer, or a replica that stopped receiving folds). In `Rolling` the wait for the other seat's deal does not count | |
| `Ended` (any) | `info.role == Ended` | `Recovery::Rejoin` → `net::rejoin` once (`warn`), a second `Ended`, `Recovery::Rehost`, or the rejoin refused (`Solo` with `join failed: …`) → `failed`/`session ended: <status>` | `warn` |
| `Done` | outcome emitted | after the linger: `net::leave_session` (which closes a host's table), then `AppExit` when `exit`. A joiner lingers `LINGER_MS` (1.5 s); a host lingers until every non-host roster seat reports disconnected (then `LINGER_MS`) or `HOST_LINGER_MS` (10 s), so a slow replica folds the last entries before the table closes | `outcome` |

The lobby filter is autoplay's own, not a change to `random::PANIC_LABELS`:
`"switch to free table"` and `"switch to rules enforced"` are never pressed
(`present.rs` offers them to the roll winner as plain affordances, and the
random brain would take a free table one time in three); when
`plan.enforced` and `view.status` carries `mode: free table`, autoplay
presses `"switch to rules enforced"` deliberately, as the soak does
(`soak.rs` `start`). `"roll"`, `"go first"` and `"let {seat} go first"` stay
random — the game is the same on both replicas whoever goes first. Reveals
are pressed by `plugin_ui::auto_reveal` already; the roll's commitment is
built by `hud::Sender::fire` through `RollSecrets`. Auto-pass is switched off
for the process so the brain is the only sender at this seat; the random
brain picks the pass itself when it is the only option.

`Choice::Action { index, .. }` is sent as `sender.fire(&view.affordances[index])`;
`Choice::Move { card, zone, hidden }` as `CardDropped { card: CardId(card),
to: Zone::Plugin(zone), seat: my_seat.0, index: table.0.in_area(my_seat.0,
to).count(), hidden }` — the two shapes `chips::Act` already produces.

### Events

Every record is one line, `KAI_EVENT ` followed by a single JSON object with
an `"event"` tag and `"ms"` (milliseconds since `install`, `web_time`).
Emission is the same on every target: `println!` (stdout on desktop, dropped
elsewhere), `bevy::log::info!(target: "kai::autoplay", …)` (stderr on
desktop, `console.log` on the web through `tracing_wasm`, logcat on android
under the tracing event's tag), and a push onto `autoplay::RECENT` (the last
512 lines) that `drain_events` empties. The desktop driver reads stdout only,
so the stderr copy is never double counted; the web driver polls
`drain_events` through `window.kai.events()`; on android a logger thread
started by `nativeAutoplay` drains it every 100 ms into logcat under the
fixed tag `kai.autoplay` (priority I, the message being the whole
`KAI_EVENT {…}` line), which is what the android driver filters on.

| event | fields | once/many | when |
|---|---|---|---|
| `installed` | `role`, `until`, `platform` (`"desktop"`/`"web"`/`"android"` from `cfg!` at the call site in the entry, passed to `install` — the one platform word the module carries, as data) | once | plan accepted |
| `meshed` | `node`, `ticket` | once | `node::get()` first returns `Some` |
| `hosting` | `node`, `ticket`, `name`, `seat` (0) | once, host | `SessionRole::Host` landed — the orchestrator starts the joiner off `ticket` |
| `joining` | `host` (as given) | once per attempt, joiner | `join_by_ticket` returned `Ok`; a retry after `join failed` emits it again, after its `warn` |
| `seated` | `seat`, `role` (`"host"`/`"client"`) | once | this seat is live (host: with `hosting`; client: `SessionRole::Client`) |
| `roster` | `seats`: `[{seat, name, host, connected}]` | many | `SessionInfo.roster` changed |
| `dealt` | `seat`, `deck` (label), `battlefield` (name) | once | my legend is on the table |
| `started` | `first` (seat), `enforced` (bool, from `view.status`) | once | turn 1 reached |
| `turn` | `n` | many | `turn_of` changed |
| `sent` | `n` (running count), `kind` (`"action"`/`"move"`/`"hide"`), `label` (`Choice::describe`) | many | an intent left this seat |
| `refused` | `text` | many | a refusal toast for my seat |
| `warn` | `text` | many | non-fatal: an automatic rejoin (`rejoin: …`), a join retry (`join retry N: …`), a stall (`stall N: …`), a battlefield index past the deck's |
| `outcome` | `result`: `"winner"` \| `"turns"` \| `"seated"` \| `"failed"`; `winner` (seat, when known); `turns` (last `turn_of`); `sent`; `refused`; `reason` (failed only); `status` (the `view.status` lines at the end) | once, terminal | see below |

Outcomes. `winner` — `view.winner` is `Some`, with `until: winner` or
`until: turns` (a win before the turn count is still a win). `turns` —
`until: {turns: n}` and turn `n` was reached. `seated` — `until: seated` and
the roster gathered. `failed` — with `reason` one of: `install: <serde error>`,
`mesh: <node status>` (no node within 60 s), `host: <status>` (hosting
refused — `enforced_without_plugin`, `host_block`), `join: <status>` (the
join dropped before seating: `host unreachable …`, `host refused …`,
`join refused: pinned …`, `wire protocol …`), `deal: <note>`,
`session ended: <status>` (also a refused automatic rejoin), `stuck` (three
fold waits with no fold and no refusal) or `stuck: no options and no fold for
60000 ms`, `turn cap`, `timeout`. Every `failed` still carries `turns`, `sent`
and `refused`, so a report shows how far the game got.

### How the plan reaches each platform

| platform | entry | who owns it |
|---|---|---|
| desktop | `KAI_AUTOPLAY='<json>'`, or `--autoplay '<json>'`, `--autoplay=<json>`, `--autoplay @<path>` — `autoplay::from_env_and_args()` (env first, then `std::env::args`, which is empty on wasm and android so the function is portable) called in `app::main` before `App::new()`, `install(json, true)` | core |
| web | `hand.js` exports `kai_autoplay(json)`, `kai_events()`, `kai_node_id()` and `kai_host_block(enforced)` from `src/net/js.rs`; `web/index.html` hangs them on `window.kai = { autoplay, events, nodeId, hostBlock }` after `init()` resolves and, when the page URL carries `?autoplay=<urlencoded json>`, starts the plan — so a Playwright `page.goto` is the whole start. A host plan is held until `kai_host_block(enforced)` clears (the engine and the gateway plugin are fetched; cap `HOST_READY_WAIT_MS` 120 s, then installed anyway so the refusal is reported as `failed`/`host: …`), because `HostReady` otherwise refuses the table before the fetches land. `install(json, false)` | web |
| android | `MainActivity.onCreate` reads `getIntent().getStringExtra("autoplay")`, else `"autoplay_b64"` (standard base64 of the JSON — the form the runner always uses, so no quoting crosses `adb shell`), and calls the static native `nativeAutoplay(String)` **before** `super.onCreate` (the library is loaded in the static initializer; the native main thread starts inside `super.onCreate`). Rust: `Java_blue_rae_kai_MainActivity_nativeAutoplay` in `src/os/android.rs` → `install(json, false)`. Later intents to a running activity are ignored | android |

Exit policy. When `exit` is true, the frame the linger ends writes
`AppExit::Success` (result `winner`/`turns`/`seated`) or `AppExit::error(1)`
(`failed`); `src/main.rs` becomes `kai::app::main(); std::process::exit(kai::autoplay::exit_code())`
(the `exit` line under `#[cfg(not(target_arch = "wasm32"))]` — `std::process::exit`
traps on wasm32 and every page load ended `RuntimeError: unreachable`
without the gate) where `exit_code()` is 0 with no plan or a successful
outcome, 1 after a failed outcome, and 3 when the loop ended with a plan
installed and no outcome (a crash of the window, a closed X server). Web and
android stay up with `outcome` as the terminal event; the driver tears them
down.

Two things outside the module the suite depends on: `src/os/entropy.rs`
takes `SystemTime` from `web_time` (std's panics on wasm, and a browser
seat's first roll killed the page), and `net::node::Node.ticket` is not what
autoplay reports — `meshed` and `hosting` carry a ticket built at
observation time from the live endpoint (`EndpointTicket::from(endpoint.addr())`),
so a desktop host whose relay handshake outlasts spirit's 10 s `wait_online`
still publishes the relay URL a browser joiner needs.

### The web identity lease, stated for the suite

A browser's node key lives in `localStorage` (`kai-node-key`); a second tab of
the same origin finds the live lease (`kai-node-lease`) and takes a per-tab
key instead (`kai-node-key-tab`, multiplayer.md "The browser peer"). The
lease check and the 2 s heartbeat run in a plain script in `index.html`
before the wasm boots, and the verdict is handed to the node through
`sessionStorage` (`kai-key-slot`), because a second page's synchronous wasm
boot in the same renderer used to starve the first tab's beat past the TTL
(now 20 s) and both pages came up with one identity. Two
Playwright **browser contexts** have separate storage, so web↔web gets two
first-tab identities and never exercises the lease; that is the intended
pairing. A variant that opens the joiner as a second *page in the same
context* exercises the lease path and is a cheap extra case for the web spec
(`web:web-lease`), not part of the matrix proper. The node id a page reports
through `kai_node_id()` is the one in its `meshed` event; the runner never
reads storage.

## The matrix

Host × joiner over three platforms. Every join is by the host's
**ticket** from its `hosting` event, so no pairing depends on gossip, the
`tables` list, or pkarr; the gateway seeds matter only where the table below
says so. Wire version must match on both sides — every build in a run comes
from the same worktree.

| host \ joiner | desktop | web | android |
|---|---|---|---|
| **desktop** | `desktop:desktop` — offline-capable: direct addrs in the ticket, no relay, no gateway (`offline: true` in the matrix sets `KAI_DEFAULT_PEERS=none`). **Passes through `run.sh`: winner in 13 turns, 59 s wall clock. CI required** | `desktop:web` — the browser reaches the desktop through the n0 relay (browsers are relay-only); the desktop needs internet. Modules: the browser asks the gateway by hash first, gets 404 for a dev-built engine/plugin, then `NeedModule` from the host. **Passes through the web driver (winner, 13 turns, both sides agree). CI required** | `desktop:android` — the emulator dials out through NAT to the host's LAN addr or the relay. **Passes through `run.sh`: winner in 13 turns, 333 s including the emulator boot. CI once the agent user is in `kvm`** |
| **web** | `web:desktop` — the browser hosts; it needs `./engine.wasm` and `assets/plugins/riftbound.wasm` in `web/dist` (the `web-plugin` probe): the browser takes the plugin from its own bundle first and only asks the gateway's module list for a plugin the bundle lacks, so no gateway has to hold the tree's plugin. The desktop joiner takes the pinned bytes from its store, its bundle, or the host over the table protocol. **CI required** | `web:web` — two contexts, both relay-only, the host's plugin from the bundle. **CI required** | `web:android` — as `web:desktop` with the emulator joining over the relay. **CI with android** |
| **android** | `android:desktop` — the phone hosts. A fresh install has no `<internal>/spirit-store/{engine,riftbound}.wasm` (nothing copies them out of the APK, `modules.rs:878-883`), so enforced hosting is refused; the driver pushes both files with `adb shell run-as blue.rae.kai` (debug APK) before `am start`. The desktop joiner dials the emulator's ticket: its direct addrs are the guest's `10.0.2.15`, unreachable, so the dial completes over the relay — internet on both sides. **Not yet seen to finish; CI with android** | `android:web` — as `android:desktop`, browser joiner. **Not yet seen to finish; CI with android** | `android:android` — two emulators, two AVDs, two `-port`s; both relay. Heavy (two guests) but nothing new. **Not yet seen to finish; CI last** |

Nine pairings, plus `web:web-lease` (`ci: extra`, run only by name — the
same-context two-page case for the identity lease). A bare `connectivity`
runs the nine. Every pairing has finished a game on this host (the final
gate of the build ran all six non-web-host pairs; the three web-host pairs
ran once the browser host served its bundled plugin, see below). Every
pairing with a browser or an emulator needs internet (relay).
`desktop:desktop` is the offline smoke and the one to run first when
anything else fails. Woodpecker (`backend: local`, this host): the four
desktop/web pairings are required; the five android pairings run
`failure: ignore` until the emulator has passed under the agent's user, then
flip to required. Real devices: none; see out of scope.

## Drivers

Every driver has the same contract, so the orchestrator does not care which
platform is behind it:

```
tests/connectivity/drivers/<platform>.sh <run-dir> <plan-json>
```

It launches one client with the plan, appends every `KAI_EVENT` payload (the
JSON only, one per line, as it arrives) to `<run-dir>/events.jsonl`, keeps
all raw output under `<run-dir>/` (`stdout.log`, `stderr.log`, `console.log`,
`logcat.log`, screenshots), writes its pid to `<run-dir>/pid` and, on exit,
its exit status to `<run-dir>/exit`. It stays up until the client emits
`outcome` or the orchestrator kills it by pid (never `pkill -f`). Scratch
state is per run dir: `SPIRIT_STORE=<run-dir>/store`,
`XDG_CONFIG_HOME=<run-dir>/xdg` (so `tuning.json`, `ai/`, `telemetry.toml`
are fresh and `StoreLock` never collides), `AGNI_TUNING` unset,
`KAI_INGEST_TOKEN` unset (no shipping from tests).

### desktop — `tests/connectivity/drivers/desktop.sh`

The `kai` binary on the private Xvfb. Environment, the gate5 recipe:

```
DISPLAY=:<n>  (Xvfb -screen 0 1600x1000x24 on the first free number from :90, started by the runner unless KAI_DISPLAY names one)
VK_DRIVER_FILES=/run/opengl-driver/share/vulkan/icd.d/lvp_icd.x86_64.json
WGPU_BACKEND=vulkan
KAI_WINDOW=1280x800
KAI_SHOT=<run-dir>/shot.png       (the frame-6 startup shot proves the window drew)
KAI_DEFAULT_PEERS=none            (desktop:desktop) | unset (anything with a browser or emulator)
KAI_AUTOPLAY=<plan>
USER=<plan.name>
LD_LIBRARY_PATH from devenv (run under `devenv shell --` or export it)
```

`stdout` is piped through `grep --line-buffered '^KAI_EVENT ' | sed 's/^KAI_EVENT //' | stamp.py >> events.jsonl`
(`stamp.py` adds the `"at"` epoch millis) while `tee` keeps the raw copy;
`stderr` (bevy's log) goes to `stderr.log` with
`RUST_LOG=info,kai=debug,agni_net=debug`. The binary is `$KAI_BIN`, default
`$CARGO_TARGET_DIR/debug/kai` — the runner never builds; a `fast-compile`
binary finds `libbevy_dylib` because the driver extends `LD_LIBRARY_PATH`
with the target's `deps` and the toolchain's `lib`. Modules:
`assets/engine/engine.wasm` and `assets/plugins/riftbound.wasm` (or, for the
nix-built `kai`, `share/kai/assets/…` beside the binary) must exist or be
named by `AGNI_ENGINE_WASM`/`AGNI_RIFTBOUND_WASM`; the driver refuses to
start without both — writing `2` to `<run-dir>/exit` first — (a native host
that pins no engine plays, but a browser joiner then folds a different engine
than the host and the suite would be testing the wrong thing). Second
instance on the same box: same recipe, different run dir.

### web — `tests/web/`

Playwright, not Selenium: it drives Chromium over CDP with first-class
`page.evaluate`, console capture, multiple isolated contexts in one browser
and a pinned browser build from nixpkgs, where Selenium needs a separate
driver binary matched to a browser it does not ship and gives no console or
context isolation of its own.

- `tests/web/package.json` pins `@playwright/test` **1.61.1**, the version of
  `nixpkgs#playwright-driver`; the browsers come from
  `nix build --inputs-from <flake root> nixpkgs#playwright-driver.browsers`
  (the flake's locked nixpkgs, substituted from cache.nixos.org, never
  downloaded by Playwright) with `PLAYWRIGHT_BROWSERS_PATH=<that store path>`,
  `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true` and
  `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` — `tests/web/env.sh` exports the
  three (`PLAYWRIGHT_BROWSERS_PATH` already set wins). `npm ci` from a
  committed `package-lock.json`.
- `tests/web/playwright.config.ts`: one project, chromium, headless,
  `launchOptions.args = ['--use-gl=angle', '--use-angle=swiftshader',
  '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
  '--disable-dev-shm-usage']` (bevy renders through WebGL2 here; software
  ANGLE is what a headless box has), viewport from `KAI_WEB_VIEWPORT`
  (`WxH`, default `1280x800`), test `timeout` `KAI_TIMEOUT_S + 180` s,
  `workers: 1`, `retries: 0`, reporter `list`, `outputDir` under
  `KAI_RUN_DIR/test-results`. With `KAI_WEB_URL` unset the config starts (or
  reuses) `tests/web/serve.mjs` on `KAI_WEB_PORT` (default 8123) over
  `KAI_WEB_DIST` (default `web/dist`); a `globalSetup` refuses a bundle
  without `hand.js` carrying `kai_autoplay` or without `engine.wasm`.
- `tests/web/lib.ts` is the seat model: `Seat.open(plan)` navigates to
  `KAI_WEB_URL + '?autoplay=' + encodeURIComponent(plan)`, pipes
  `page.on('console')`/`pageerror`/`.wasm`/`.js` fetch timings to
  `console.log` (ISO-stamped), waits for `window.kai` (`BOOT_WAIT_MS`
  120 s — a second bevy page in one headless chromium boots in 30-60 s),
  then polls `window.kai.events()` every 250 ms, appending each line as
  `{"at": epoch ms, …event}` to `events.jsonl`; `hosting` also writes
  `node_id` and `ticket`; a screenshot `shot-seated.png` on the first
  `seated` and `shot.png` on the outcome; the outcome wait is
  `timeout_s + 30` s from the page open, the `hosting` wait 120 s.
- `tests/web/autoplay.spec.ts`: three tests gated by the mode —
  `KAI_WEB_MODE=host|join|both`, else the `KAI_PLAN` role, else `both`.
  `host` and `join` take `KAI_PLAN` (json) as sent; `join` also accepts
  `KAI_JOIN_HOST` and builds a plan from `KAI_SEED` (+1 for a joiner),
  `KAI_UNTIL` (`winner|seated|turns:N`) and `KAI_TIMEOUT_S`; `both` opens two
  **contexts** (host, then the joiner off the host's ticket), asserts two
  distinct node ids, and checks agreement, seats and `started.enforced`
  itself. Files land in `KAI_RUN_DIR` (`both`: `<run-dir>/host` and
  `<run-dir>/joiner`), else `tests/web/runs/<mode>-<stamp>`. A test passes
  when the outcome is not `failed`; the orchestrator still does the
  cross-side assertions itself.
- `tests/web/web-lease.spec.ts`: the same-context two-page variant above,
  host and joiner both browsers, run by `--only web:web-lease` (the runner
  invokes the spec directly with `KAI_RUN_DIR=<pair dir>`, not two drivers).
- `tests/connectivity/drivers/web.sh` wraps the spec: `cd tests/web && npx playwright test autoplay.spec.ts`
  with `KAI_PLAN`, `KAI_RUN_DIR`, `KAI_WEB_DIST`, `KAI_WEB_MODE` unset (the
  plan's role picks the test) and `env.sh` sourced; `npm ci` runs itself when
  `node_modules` is missing. The static server is started once per run by
  the orchestrator (`python3 -m http.server` on a free port over `web/dist`,
  exported as `KAI_WEB_URL`; `web-serve` is the by-hand equivalent on 8123)
  and `web/dist` must already hold a bundle with `engine.wasm`
  (`devenv shell -- web-build`). The driver's refusals (`hand.js` without
  `kai_autoplay`, no `engine.wasm`, `npm ci` failed) write `2` to
  `<run-dir>/exit` and exit 2; a missing gateway is only noted on stderr —
  the runner's preflight is what skips a pair. There is no
  `tests/web/README.md`; this page is the doc.

Networking: a browser peer speaks only WebSocket to the n0 public relay, so
every pairing with a browser needs internet; a browser **host** additionally
needs a gateway (`dev1`/`dev2` over the tailnet) for `modules/riftbound`,
holding the same plugin the tree ships (the `gateway` probe compares blob
hashes; a mismatch is `skipped`, never a fail). A browser's mesh seeds are
the gateway node ids; they are not needed for the join itself (ticket), only
for the plugin fetch and the tables list.

### android — `tests/android/`

- **APK for the emulator.** `android/devenv.nix` adds
  `"x86_64-linux-android"` to `languages.rust.targets`, and `android-native`
  takes the ABI list from `KAI_ANDROID_ABIS` (default `arm64-v8a`), running
  `cargo ndk` once per ABI (`-t x86_64` alongside `-t arm64-v8a`) and
  stripping each `libkai.so`. `android/app/build.gradle` reads the project
  property `kaiAbis` (default `arm64-v8a`) into `abiFilters`, so the release
  build is untouched and `gradle -PkaiAbis=x86_64 assembleDebug` yields the
  test APK. `android-build` passes `-PkaiAbis=$KAI_ANDROID_ABIS` through,
  and `android-build-emulator` is `KAI_ANDROID_ABIS=x86_64 android-build`.
  The SDK kai's `android/devenv.nix` already yields carries emulator 37.2.4
  and the `system-images;android-34;google_apis_playstore;x86_64` and
  `arm64-v8a` images (devenv's android module includes both by default) —
  `/nix/store/yb1bchf5…-androidsdk` on this box; nothing to add there.
- `tests/android/emulator.sh` is the library (sourced by the driver and by
  the one-call wrappers below); every function needs the android devenv
  shell (`cd android && devenv shell`) for `avdmanager`, `emulator`, `adb`,
  and `ANDROID_AVD_HOME` under the run dir so two runs never share an AVD.
- `tests/android/avd.sh create|delete <name>`:
  `avdmanager create avd -n <name> -k "system-images;android-34;google_apis_playstore;x86_64" -d pixel_6 --force`,
  then `hw.keyboard=yes`, `hw.gpu.enabled=yes`, `hw.gpu.mode=swiftshader_indirect`,
  `hw.lcd.density=420`, `disk.dataPartition.size=2G`, audio off,
  `hw.initialOrientation=landscape` in `config.ini`.
- `tests/android/boot.sh <name> <port> [run-dir]`: `emulator -avd <name> -port <port>
  -no-window -no-audio -no-boot-anim -no-snapshot -no-metrics -gpu swiftshader_indirect
  -accel on -netdelay none -netspeed full` in the background with
  `LD_LIBRARY_PATH` unset (devenv's NDK `libc++.so` breaks the emulator's
  `libabseil_dll.so`), pid in `<run-dir>/emulator.pid`, serial in
  `<run-dir>/serial`; then a loop on `getprop sys.boot_completed` = 1 (cap
  `KAI_ANDROID_BOOT_CAP_S`, 240 s), `settings put global
  window_animation_scale 0` and the other two scales, screen on and never
  off, and **the display rotated to landscape before any app runs**:
  `accelerometer_rotation 0`, `user_rotation 1`,
  `wm fixed-to-user-rotation enabled`, `wm user-rotation lock 1`, then a
  poll on `dumpsys window displays` `mRotation` until it reports
  `ROTATION_90` (cap `KAI_ANDROID_ROTATE_CAP_S`, 30 s). Without the
  fixed-to-user-rotation the launcher keeps the freshly booted guest in
  portrait whatever the settings say, the landscape-only `MainActivity` then
  rotates the display at its own launch and is relaunched for it, and the
  relaunch never reaches `onCreate` (`GameActivity.onDestroy` joins the
  native thread, bevy's loop does not exit on `APP_CMD_DESTROY`): the app
  sits with `installed` as its only event until the timeout.
- `tests/android/install.sh <serial> [apk] [--modules [assets-dir]]`: `adb install -r -g`,
  then with `--modules` (a **host** plan) `adb push` `engine/engine.wasm` and
  `plugins/riftbound.wasm` to `/data/local/tmp` and
  `run-as blue.rae.kai sh -c 'mkdir -p files/spirit-store && cp … files/spirit-store/'`
  (the names `modules.rs` looks for: `engine.wasm`, `riftbound.wasm`).
- `tests/android/start.sh <serial> <plan-json>`: force-stop, then
  `adb shell am start -W -n blue.rae.kai/.MainActivity --es autoplay_b64 "$(printf %s "$plan" | base64 -w0)"`.
- `tests/android/events.sh <serial> <run-dir>`: `adb logcat -c`, then
  `adb logcat -v raw -s kai.autoplay:I` tee'd to `logcat.log`,
  `grep --line-buffered -o 'KAI_EVENT {.*}' | sed 's/^KAI_EVENT //'`, each
  line stamped `{"at": epoch ms, …}` into `events.jsonl`; pid in
  `<run-dir>/events.pid`. The fixed tag comes from the logger thread in
  `src/os/android.rs`, so no pid filter is needed and nothing is missed while
  the app starts.
- `tests/connectivity/drivers/android.sh` composes them (re-executing itself
  under the android devenv shell when `emulator` is not on `PATH`): create
  `KAI_ANDROID_AVD` (default `kai-<port>`) → boot on `KAI_ANDROID_PORT`, else
  the first even port from 5554 that `adb devices` does not list → install
  `KAI_ANDROID_APK` (+ modules from `KAI_ANDROID_MODULES` for a host) →
  events → start; screenshots at `seated` and at the outcome; exit 0 on a
  non-failed outcome, 1 otherwise or after `timeout_s + 30` s. Teardown on
  any exit, with `INT`/`TERM` ignored while it runs: stop the pipe,
  `shot-final.png`, force-stop the app, dump the full logcat to
  `logcat-full.log`, `adb emu kill` (then `TERM`, then `KILL` by pid), delete
  the AVD (`KAI_KEEP_AVD=1` keeps it), write `<run-dir>/exit`. Two android
  instances are two run dirs and two AVD names (`kai-host`, `kai-joiner`);
  the runner starts the joiner only after the host's `hosting`, so the
  port pick never races.

Networking: the emulator's user-mode NAT lets the guest open outbound
connections to the host machine's LAN address and to the internet, and
nothing can dial in. An android **joiner** given a desktop host's ticket dials
the direct LAN addr (works, guest-initiated) or the relay; an android
**host**'s ticket carries `10.0.2.15`, so every joiner reaches it through the
relay only. The x86_64 emulator with KVM is the runnable path; the arm64
image without KVM is not usable for a game-length run.

### orchestrator — `tests/connectivity/`

- `matrix.json` — the pairings: `{"pairs":[{"id":"desktop:web","host":"desktop","joiner":"web","needs":["xvfb","kai-bin","internet","web-dist"],"ci":"required"}, …]}`
  for all nine plus `web:web-lease` (`"ci": "extra"`, `"spec": "web-lease"`);
  `desktop:desktop` carries `"offline": true` (`KAI_DEFAULT_PEERS=none`).
  `needs` is a list of preflight probes, and the runner adds
  `driver:<host>`/`driver:<joiner>` (the driver script exists) itself:
  `internet` → the relay answers; `gateway` → `/gateway/status` answers;
  `web-plugin` → `web/dist/assets/plugins/riftbound.wasm` is in the bundle;
  `kvm` → `/dev/kvm` writable; `xvfb` → `Xvfb` on `PATH` and the
  lavapipe icd; `kai-bin` → `$KAI_BIN` and both modules; `web-dist` →
  `hand.js` with `kai_autoplay`, `engine.wasm`, `tests/web/node_modules`;
  `apk` → the debug APK and `android/devenv.nix`. `matrix.py ids|default|ci <lane>|show <id>`
  is the reader (`default` is every pair but the extras).
- `run.sh [--only a:b,c:d] [--skip …] [--seed n] [--until winner|seated|turns:N] [--timeout n | --timeout-s n] [--out <dir>] [--report <path>] [--keep] [--allow-rejoin] [--list]`,
  bash, `set -euo pipefail`, functions over copy-paste (`lib.sh`). Per pair:
  1. `<out>/<pair>/` (the id's `:` becomes `_`) is removed and recreated, so
     a reused `--out` never reads a stale `events.jsonl` or `meta.json`;
     preflight the `needs`; a failed probe is `skipped` with the probe named,
     never a fail;
  2. `host/`, `joiner/` under it; `start_driver` truncates `events.jsonl`
     before the driver launches; start the host driver with the host plan
     (`seed`, `name: "<pair>-host"`); wait for a `hosting` line in its
     `events.jsonl` (cap `HOSTING_WAIT_S` 120 s: the mesh, the plugin fetch
     on a web host);
  3. start the joiner driver with `{"role":{"join":{"host": <ticket>}}, …}`,
     `seed + 1`, `name: "<pair>-joiner"`;
  4. wait for an `outcome` on both sides (cap `timeout_s` + 30 s; 60 s for
     the slower side once the first has landed), give the drivers
     `EXIT_GRACE_S` (20 s: a browser tears down slowly after its test) to leave on their own, then stop both by pid:
     `TERM` to `pid` and `driver.pid`, a grace of `STOP_GRACE_S` (10 s; 60 s
     when the run dir holds an `emulator.pid`, the android teardown being
     screenshot, force-stop, logcat dump, `adb emu kill` and the AVD delete),
     then `KILL` to `pid`, `driver.pid`, `kai.pid`, `events.pid` and
     `emulator.pid`, so nothing outlives the runner;
  5. assess (next section) and write `<out>/<pair>/result.json`;
  6. teardown: keep the run dirs (`--keep` keeps `store`, `xdg` and `avd`
     too; otherwise they are removed, logs stay).
  Xvfb: the first free number from `:90` with a lock-free socket, up to
  three tries when a display fails to bind; `KAI_DISPLAY` names one instead.
  The static web server: `python3 -m http.server` on a free port, once per
  run, unless `KAI_WEB_URL` is set.
- `report.sh <out> [report.json]` folds the `result.json`s into `<out>/report.json`
  (`{"pairs":[{id, status: pass|fail|skipped, reason, host: {outcome}, joiner: {outcome}, seconds}]}`)
  and `<out>/junit.xml` (one `testcase` per pair, `<failure>` with the
  reason and the last twenty event lines of each side, `<skipped>` with the
  probe). `run.sh` exits 1 if any pair failed, 0 if every run pair passed
  (skips do not fail).
- `devenv.nix` gains `scripts.connectivity.exec = "$DEVENV_ROOT/tests/connectivity/run.sh \"$@\""`,
  and `web-build`, `engine-build` and
  `plugin-build` learn `CARGO_TARGET_DIR` (their `target/wasm32-unknown-unknown/…`
  paths become `${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/…`), since
  the test builds live in `/build/targets/*` and the worktree disk has filled
  before. No `target` symlink is needed in the worktree, and none should be
  committed (`.gitignore`'s `/target/` matches a directory only).
- `.woodpecker/kai-connectivity.yml`: `when: [{event: manual}, {event: cron, cron: nightly}]`
  plus the `kai.yml` path filters on push (so a change to kai, agni, spirit or
  the flake runs it — as `failure: ignore` on push, required on manual/cron),
  `labels: backend: local`. The `build` step is `nix build .#kai .#kai-web`
  (the same attributes `agni-artifacts.yml` deploys, so the suite exercises
  the shipped bundle shape: `index.html` and `engine.wasm` at the root,
  `riftbound.wasm` under `assets/plugins`, the modules under
  `share/kai/assets` beside the `kai` wrapper), the browsers and node from
  `nixpkgs#playwright-driver.browsers`/`nixpkgs#nodejs` with `--inputs-from .`,
  and `npm ci` in `tests/web`; the `desktop-web` lane runs
  `connectivity --only desktop:desktop,desktop:web,web:desktop,web:web`
  (required on manual/cron, `failure: ignore` on push); the `android` lane
  (manual/cron only, `failure: ignore`) builds the x86_64 debug APK itself
  when `/dev/kvm` is writable and runs the five android pairs. Each lane
  prints its `report.json`. Xvfb runs inside the step on a display picked
  from a free number. The nightly cron also fires `build.yml`'s fleet
  rebuild on the same agent; if the two squeeze each other, give this
  pipeline its own cron name.
- `AGENTS.md` gets a "Connectivity suite" paragraph (where it lives, how to
  run one pair, what it does not test); the README gets the command under
  Multiplayer.

## File ownership

Four implementers, no shared files. Where one needs another's file, the
line to add and the interface consumed are here.

| owner | owns | provides | consumes |
|---|---|---|---|
| **core** | `src/autoplay.rs` (new); `src/lib.rs` (`pub mod autoplay;`, and `pub mod ai;` made unconditional); `src/ai/mod.rs` (the non-portable submodules `brain`, `cards`, `driver`, `local`, `nanogpt`, `seat` behind `#[cfg(not(target_arch = "wasm32"))]`; `random` and `soak` stay everywhere — both are pure over `agni_sim`/`serde`); `src/ai/random.rs`, `src/ai/soak.rs` (only if a helper is needed; none is expected); `src/app.rs` (`from_env_and_args` + `install` before `App::new()`, `.add_plugins(crate::autoplay::AutoplayPlugin)`); `src/main.rs` (the exit code); `src/net/mod.rs` (only the line `#[cfg(target_arch = "wasm32")] pub mod js;` for web, and `pub` on anything autoplay needs that is private today); `Cargo.toml` (nothing expected — `serde`, `serde_json`, `web_time`, `parking_lot` are already on every target) | `pub fn install(json: &str, exit_default: bool) -> Result<(), String>`; `pub fn from_env_and_args() -> Option<String>`; `pub fn drain_events() -> Vec<String>`; `pub fn node_id() -> Option<String>`; `pub fn exit_code() -> i32`; `pub struct AutoplayPlugin`; `pub const EVENT_PREFIX: &str = "KAI_EVENT "`; `pub const ENV_VAR`, `ARG`, `QUERY_PARAM = "autoplay"`, `INTENT_EXTRA = "autoplay"`, `INTENT_EXTRA_B64 = "autoplay_b64"`; `pub struct Plan` with `Deserialize`/`Serialize` and `Plan::example_host()`/`Plan::example_join(host)` used by the unit tests and the docs | `net::{host_table, join_by_ticket, rejoin, leave_session, withdraw_table, rules_enforced, TableChoice, TableGame}`, `table::{SessionInfo, SessionRole, Recovery, MySeat, GameTable, CardDropped, Tuning}`, `table::plugin_ui::PluginPanel`, `table::hud::Sender`, `table::toast::Refusals`, `deck::{pinned::PinnedDeck, pinned::legends_on_table, import::SeatedDeck, battlefield::needs_choice, pool}`, `ai::random`, `ai::soak::turn_of`, `menu::{Menu, Screen}`, `net::node::get` |
| **web** | `src/net/js.rs` (new: `#[wasm_bindgen] pub fn kai_autoplay(json: String) -> Result<(), JsValue>` → `install(&json, false)`; `#[wasm_bindgen] pub fn kai_events() -> js_sys::Array` of `JsString` → `drain_events()`; `#[wasm_bindgen] pub fn kai_node_id() -> Option<String>` → `node_id()`; `#[wasm_bindgen] pub fn kai_host_block(enforced: bool) -> Option<String>` → `net::host_block`); `web/index.html` (the `window.kai` object and the `?autoplay=` start); `tests/web/**` (package.json, package-lock.json, playwright.config.ts, global-setup.ts, lib.ts, autoplay.spec.ts, web-lease.spec.ts, serve.mjs, env.sh — no README.md, this page is the doc); `tests/connectivity/drivers/web.sh` | the four exports, `window.kai.{autoplay, events, nodeId, hostBlock}`, the driver | `autoplay::{install, drain_events, node_id}`; the `pub mod js;` line from core |
| **android** | `android/devenv.nix` (the rust target, `KAI_ANDROID_ABIS`), `android/app/build.gradle` (`kaiAbis`), `android/app/src/main/java/blue/rae/kai/MainActivity.java` (the extras and `nativeAutoplay`), `src/os/android.rs` (`Java_blue_rae_kai_MainActivity_nativeAutoplay`); `tests/android/**` (emulator.sh, avd.sh, boot.sh, install.sh, start.sh, events.sh, README.md); `tests/connectivity/drivers/android.sh` | the x86_64 debug APK at `android/app/build/outputs/apk/debug/app-debug.apk`, the driver | `autoplay::install`, `autoplay::{INTENT_EXTRA, INTENT_EXTRA_B64}` |
| **orchestrator** | `tests/connectivity/{run.sh, lib.sh, matrix.json, matrix.py, plan.py, events.py, stamp.py, assess.py, report.sh, report.py, drivers/desktop.sh, README.md}`; `devenv.nix` (`connectivity`, the target-dir lines); `.woodpecker/kai-connectivity.yml`; `AGENTS.md` (the test section) and `README.md` (the command); `wiki/design/connectivity.md` (this page, after the design round) | the driver contract, the report | the three drivers by path, the event names above |

The web and android owners' drivers live under `tests/connectivity/drivers/`
by name so the orchestrator can call them; the orchestrator owns the
directory's `desktop.sh` and `lib.sh` only. Nobody edits
`src/table/**`, `src/deck/**`, `src/menu/**` or `agni`; if autoplay needs a
field made `pub` there, core asks for it as a request and works around it
(`PluginPanel.view`, `PinnedDeck.side`, `SeatedDeck.0`, `Refusals.log` and
`Menu.screen` are `pub` already).

## Assertions

Per pair, after both outcomes:

1. both drivers produced an `outcome`, neither `failed`;
2. host `outcome.result == joiner outcome.result`, and it is the plan's
   `until` (`winner` for the default run);
3. `winner` equal on both sides and `turns` equal on both sides — both
   replicas folded the same log, so any difference is a replication bug,
   not a flake — and the game was played: with `winner` the seat is one of
   the `players`, `turns >= 1` (`>= n` for `turns: n`) and both sides
   `sent` at least one intent, so a pair where nobody moved cannot pass;
4. `started.enforced == plan.enforced` on both sides (the random brain never
   switched the mode);
5. the joiner's `seated.seat == 1` and the host's `== 0`; the final `roster`
   on both sides lists two connected seats;
6. no `warn` with `rejoin` on either side unless `--allow-rejoin` (a
   reconnect mid-game is a pass with a note, not silently a pass);
7. `sent` counts are reported beyond that floor, not asserted; `refused` counts are reported
   and a pair with refusals above `MAX_REFUSALS` (8, the soak's number) on
   either side is a fail — a random move the engine refuses is a legal-list
   bug the soak would have caught, and over the network it hides a lost
   intent.

A failure report (`result.json` plus the junit `<failure>` body) carries: the
pair id and the seeds; both plans as sent; both `events.jsonl` in full with
their `ms`; the last 200 lines of each side's `stderr.log`/`console.log`/`logcat.log`;
the paths of the screenshots; the host's ticket; the preflight results; the
build identity (`git rev-parse HEAD`, the version from `Cargo.toml`, the
wire version from `agni/net/src/proto.rs`); wall-clock start and end per
side. Timestamps: every event has
`ms` since install; the driver stamps each `events.jsonl` line with the
runner's epoch millis as a second field when it appends (`{"at": …, …}`
merge), so the two sides' streams can be interleaved in one timeline.

## Out of scope this round

- iOS, real devices, cloud device farms, Firefox and WebKit (Chromium only —
  the bundle runs in all three, the suite proves connectivity, not browsers).
- Three or more seats, spectators, mid-game reconnect as a scenario of its
  own (the rejoin path is exercised only if it happens, and then noted).
- MTG and free-form tables; the LLM brain; deck import from links.
- The `tables` gossip listing and the join-by-gossip path (known not to
  list, memory `kai-post-m8-followups`); every join here is by ticket.
- Rules correctness, replay determinism, performance numbers.
- Self-hosted relay or gateway for an offline CI; the tailnet and the n0
  relay are accepted dependencies for eight of the nine pairings.
- An android foreground service; the emulator stays foregrounded and the
  screen on, and a phone host's fragility (multiplayer.md "Known
  limitations") is not exercised.

## Order of work, and what "done" is

Core lands first and alone unblocks `desktop:desktop`: the orchestrator's
`run.sh` with `drivers/desktop.sh` is the first green pair and proves the
event contract. Web and android build against the `autoplay` interface
listed above without waiting on each other. Done for the round: the four
desktop/web pairings pass on this host through `devenv shell -- connectivity`
and in `kai-connectivity.yml` on manual trigger; the five android pairings
pass locally; the nightly cron is on with the android pairs as
`failure: ignore`; `desktop:desktop` finishes in under three minutes wall
clock including a full random game.

## Where it stands

Built, reviewed and run. The build's final gate ran the full matrix with
one seed: the six pairings with a desktop or android host all finished a
random game on the first attempt with both sides agreeing on the winner and
the turn count (`desktop:desktop` 59 s; `desktop:web` 229 s;
`desktop:android` 509 s with the emulator boot; `android:desktop` 353 s;
`android:web` 282 s; `android:android` 440 s with two emulators). The three
web-host pairings were skipped by that gate because the browser host still
took its plugin from the gateway, which held a legacy blob; the browser now
serves its bundled plugin first, the `gateway` need became `web-plugin`,
and those pairings run like the rest. The pipeline has not run on
Woodpecker yet; trigger it by hand once after the merge.

Outside this suite's files, three product-level items the review turned
up: `deck::pinned::side_after` replaces an explicitly pinned side on the
`Joining → Client` transition (autoplay avoids it by pinning only once
seated; a hand-driven joiner who pinned before joining still loses the
pick); `net::redeal_after_new_game` fires a `DealDeckRequested` on the
join-time generation bump without checking `needs_choice` (autoplay
pre-syncs `NewGameWatch` around it); and a landscape relaunch of
`MainActivity` never reaches `onCreate` because bevy's loop does not exit on
`APP_CMD_DESTROY` (the harness sidesteps it by rotating the display first).
