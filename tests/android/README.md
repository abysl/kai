# Android harness

The android side of the connectivity suite (`wiki/design/connectivity.md`):
an x86_64 debug APK, a throwaway AVD per run, a headless emulator under KVM,
and the app's `KAI_EVENT` lines lifted out of logcat.

## The APK

```
cd android && devenv shell -- android-build-emulator
```

That is `KAI_ANDROID_ABIS=x86_64 android-build`: `cargo ndk -t x86_64` (any
comma list of ABIs works, `arm64-v8a` is the default for phones), strip, then
`gradle -PkaiAbis=x86_64 assembleDebug`. The result is
`android/app/build/outputs/apk/debug/app-debug.apk`. `build.gradle` reads
`kaiAbis` into `abiFilters`, so a release build without the property is the
arm64 APK it always was. Set `CARGO_TARGET_DIR` first when the worktree disk
is tight; `cargo ndk` honours it.

## How the plan gets in

`am start -n blue.rae.kai/.MainActivity --es autoplay_b64 <base64 json>`
(or `--es autoplay '<json>'`). `MainActivity.onCreate` decodes the extra and
calls `nativeAutoplay` **before** `super.onCreate`, so the plan is installed
before bevy's main thread starts. A later intent to a running activity does
nothing. The Rust side (`src/os/android.rs`) installs the plan and starts one
thread that drains `autoplay::drain_events()` every 100 ms into logcat with
the fixed tag `kai.autoplay` (priority I), the message being the full
`KAI_EVENT {…}` line. A refused plan logs `autoplay refused: …` at E on the
same tag after the `outcome failed` line. Read them with

```
adb logcat -v raw -s kai.autoplay:I
```

The tracing copy bevy writes (tag = the tracing event name) is still there;
the harness ignores it.

## Scripts

All of them need the android devenv shell (`cd android && devenv shell`) for
`avdmanager`, `emulator`, `adb`; `emulator.sh` is the library, the others are
one-call wrappers around it. `ANDROID_AVD_HOME` must point under the run dir
so two runs never share an AVD.

| script | does |
|---|---|
| `emulator.sh <cmd> …` | the functions: `avd_create`, `avd_delete`, `emu_boot`, `emu_wait_boot`, `apk_install`, `modules_push`, `app_start`, `events_stream`, `events_stop`, `wait_for_event`, `event_field`, `event_lines`, `app_screenshot`, `app_stop`, `emu_kill`, `free_port` |
| `avd.sh create\|delete <name>` | `avdmanager create avd -k "system-images;android-34;google_apis_playstore;x86_64" -d pixel_6 --force`, then `hw.keyboard`, `hw.gpu.mode=swiftshader_indirect`, `hw.lcd.density=420`, `disk.dataPartition.size=2G`, audio off |
| `boot.sh <name> <port> [run-dir]` | `emulator -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -accel on …` in the background, pid in `<run-dir>/emulator.pid`, serial in `<run-dir>/serial`; waits for `sys.boot_completed`, zeroes the animation scales, keeps the screen on, rotates the display to landscape (`user_rotation 1` plus `wm fixed-to-user-rotation enabled`, then polls `dumpsys window displays` until `mRotation` is `ROTATION_90`, cap `KAI_ANDROID_ROTATE_CAP_S` 30 s) |
| `install.sh <serial> [apk] [--modules [assets-dir]]` | `adb install -r -g`; with `--modules` pushes `engine/engine.wasm` and `plugins/riftbound.wasm` into the app's `files/spirit-store` through `run-as` (a host plan needs them: the app never copies them out of the APK) |
| `start.sh <serial> <plan-json>` | force-stop, `am start -W … --es autoplay_b64 …` |
| `events.sh <serial> <run-dir>` | `logcat -c`, then the pipe: `logcat -v raw -s kai.autoplay:I` → `logcat.log`, the `KAI_EVENT` payloads → `events.jsonl`, each stamped `{"at": <epoch ms>, …}`; start it before the app so nothing is missed |

`wait_for_event <run-dir> <event> <cap-s> [serial]` and
`event_field <run-dir> <event> <field>` are what a caller polls with;
`emu_kill <run-dir>` is the teardown (`adb emu kill`, then SIGTERM, then
SIGKILL by pid).

`emulator` runs with `LD_LIBRARY_PATH` unset: devenv's android module puts the
NDK's host `libc++.so` on the path and the emulator's `libabseil_dll.so` then
fails to resolve its own libc++ symbols.

The landscape lock matters. `MainActivity` is landscape-only and the freshly
booted guest is portrait; the activity is created before the display has
rotated and is then relaunched for the rotation. `GameActivity`'s
`onDestroy` joins the native thread, bevy's loop does not exit on
`APP_CMD_DESTROY`, and the relaunch never reaches `onCreate`: the app sits
with `installed` as its only event until the timeout. `user_rotation 1`
alone does not rotate the display, because the launcher in the foreground
forces portrait; `wm fixed-to-user-rotation enabled` makes the display
follow the user rotation whatever the foreground activity asks, so it is
`ROTATION_90` before `am start` and the activity never sees a config change.

## The driver

`tests/connectivity/drivers/android.sh <run-dir> <plan-json>` is the
composition the orchestrator calls: it re-executes itself under the android
devenv shell when `emulator` is not on `PATH`, creates `kai-<port>` under
`<run-dir>/avd`, boots on `KAI_ANDROID_PORT` (else the first free even port
from 5554), installs `KAI_ANDROID_APK`, pushes the modules for a host plan,
starts the app, streams events, screenshots at `seated` and `outcome`, and
exits 0 on a non-failed `outcome` (1 otherwise, or after `timeout_s + 30`).
Teardown on any exit, with INT and TERM ignored while it runs: stop the
pipe, force-stop the app, dump the full logcat to `logcat-full.log`, kill the
emulator, delete the AVD (kept with `KAI_KEEP_AVD=1`), write
`<run-dir>/exit`. `<run-dir>/pid` is the driver's own pid; kill that to stop
everything, and give it a minute before a KILL.

Two emulators on one box: two run dirs and two AVD names; the second driver
picks the next free even port once the first emulator is listed by
`adb devices` (or set `KAI_ANDROID_PORT` by hand).

## Networking

The guest reaches the internet and the host machine's LAN address through
user-mode NAT; nothing can dial in. An android joiner dials a desktop host's
ticket over the direct LAN address or the n0 relay, and a bare node id through
n0's DNS discovery, so the desktop host needs internet for the node-id case.
An android host's ticket carries `10.0.2.15`; every joiner reaches it over the
relay only.
