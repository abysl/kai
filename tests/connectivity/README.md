# Connectivity suite

Two kai clients on any pair of platforms find each other, seat, deal, roll
and play a random-brain Riftbound game to a winner, with nobody at the
keyboard. The design is `wiki/design/connectivity.md`; this directory is the
orchestrator half of it: the matrix, the runner, the desktop driver and the
report. The autoplay mode the clients run is `src/autoplay.rs`; the web and
android drivers live in `tests/web` and `tests/android` and are called here
by name.

## Run one pair

```
devenv shell -- connectivity --only desktop:desktop
```

`desktop:desktop` needs a debug `kai` binary (`cargo build --bin kai
--features fast-compile`, found at `$CARGO_TARGET_DIR/debug/kai` or named by
`KAI_BIN`), both bundled modules (`modules-build`, or `AGNI_ENGINE_WASM` and
`AGNI_RIFTBOUND_WASM`), `Xvfb` and lavapipe; it runs offline. Every pair with
a browser or an emulator needs the n0 relay (internet); a browser host
serves the plugin its bundle ships (`web/dist/assets/plugins/riftbound.wasm`,
the `web-plugin` probe). The runner probes each pair's `needs` from
`matrix.json` first and reports `skipped: <probe>` for anything missing
rather than failing. A bare `connectivity` runs the nine pairs;
`web:web-lease` (`ci: extra`, the same-context lease case) only by `--only`.

```
run.sh [--only a:b,c:d] [--skip …] [--seed N] [--until winner|seated|turns:N]
       [--timeout S] [--out DIR] [--report PATH] [--keep] [--allow-rejoin] [--list]
```

The run dir defaults to `${CARGO_TARGET_DIR:-target}/connectivity/<stamp>`,
with `<pair>/host/` and `<pair>/joiner/` under it (the id's `:` becomes `_`;
a pair dir is wiped before its run, so `--out` can be reused).
Each side holds `plan.json`, `events.jsonl`, `stdout.log`, `stderr.log` or
`console.log` or `logcat.log`, `driver.log`, `shot.png`, `pid`, `exit`; the
pair holds `meta.json` and `result.json`; the run holds `summary.txt`,
`report.json` and `junit.xml`. Scratch stores are removed after each pair
unless `--keep`. Exit status is 1 if any run pair failed; skips do not fail.

## What a pair does

1. preflight the probes (`xvfb`, `kai-bin`, `web-dist`, `apk`, `kvm`,
   `internet`, `gateway`, and that both drivers exist);
2. start the host driver with `{"role":"host", seed, name "<pair>-host"}` and
   wait up to 120 s for its `hosting` event;
3. start the joiner driver with `{"role":{"join":{"host":<ticket>}}}`, seed+1;
4. wait for an `outcome` on both sides (cap `timeout_s` + 30 s; 60 s for the
   slower side once the first has landed), then stop both drivers by pid
   (TERM, a 10 s grace — 60 s for an android driver, whose teardown kills
   the emulator — then KILL on every pid file in the run dir);
5. assess and write `result.json`.

Assertions: both outcomes present and neither `failed`; results equal and
the plan's `until`; `winner` and `turns` equal on both sides, the winner a
real seat, the turn count at least 1 (or the `turns:N` asked for) and both
sides sent at least one intent; `started.enforced`
matches the plan; host seated at 0 and joiner at 1; the final roster on both
sides lists two connected seats; no rejoin warning unless `--allow-rejoin`
(then a note); refusals at most 8 per side. `sent` counts are reported only.

## The driver contract

```
tests/connectivity/drivers/<platform>.sh <run-dir> <plan-json>
```

A driver launches one client with the plan, appends every `KAI_EVENT`
payload to `<run-dir>/events.jsonl` (one JSON object per line, stamped with
`"at"` epoch millis), keeps raw output under `<run-dir>/`, writes its own pid
to `<run-dir>/pid` and its exit status to `<run-dir>/exit` on the way out,
and stays up until the client emits `outcome` or it is killed by that pid.
The runner exports for every driver: `KAI_SIDE` (`host`/`joiner`),
`KAI_RUN_DIR`, `KAI_PLAN`, `KAI_TIMEOUT_S`, `AGNI_ENGINE_WASM`,
`AGNI_RIFTBOUND_WASM`, `KAI_DEFAULT_PEERS=none` on the offline pair; for web
`KAI_WEB_URL` (a static server on a free port over `KAI_WEB_DIST`, started
once per run unless `KAI_WEB_URL` is already set); for android `KAI_APK`,
`KAI_ANDROID_APK`, `KAI_ANDROID_AVD` (`kai-host`/`kai-joiner`) and
`KAI_ANDROID_MODULES` (the assets dir a host's modules are pushed from); the
emulator port is the first even one from 5554 that `adb devices` does not
list, so a developer's own emulator is never trampled. `web:web-lease` is not two drivers but
`tests/web/web-lease.spec.ts` run once with `KAI_RUN_DIR=<pair dir>`,
`KAI_SEED` and `KAI_UNTIL`; it writes `host/` and `joiner/` itself.

`drivers/desktop.sh` runs `$KAI_BIN` on the runner's private Xvfb with
lavapipe (`VK_DRIVER_FILES`, `WGPU_BACKEND=vulkan`), `KAI_WINDOW=1280x800`,
`KAI_SHOT`, a scratch `SPIRIT_STORE` and `XDG_CONFIG_HOME` per instance,
`RUST_LOG=info,kai=debug,agni_net=debug`, and reads events from stdout only.
It refuses to start without both modules. A `fast-compile` binary finds
`libbevy_dylib` through `LD_LIBRARY_PATH`, which the driver extends with the
target's `deps` and the toolchain's `lib`.

## CI

`.woodpecker/kai-connectivity.yml` runs on manual trigger and the nightly
cron (required) and on pushes touching kai, agni, spirit or the flake
(`failure: ignore`). It builds `.#kai` and `.#kai-web`, takes the browsers
from `nixpkgs#playwright-driver.browsers`, `npm ci`s `tests/web`, builds the
four desktop/web pairs in the `desktop-web` lane and, on manual and cron
only and `failure: ignore`, builds the x86_64 debug APK when the agent has
`/dev/kvm` and runs the five android pairs. The `report.json` is printed at
the end of each lane.

## What it does not test

Rules correctness (the soak does), the gossip tables listing, reconnect as a
scenario, three or more seats, real devices, browsers other than Chromium.
