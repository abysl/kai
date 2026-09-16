# Android connectivity harness

Audience: developers adding or debugging Android multiplayer tests.
Read the [connectivity guide](../connectivity/README.md) first.

## Build the test application

From the repository root:

```sh
cd android
devenv shell -- android-build-emulator
```

This builds an x86_64 debug APK for the emulator, not a signed release for users.
The expected output is `android/app/build/outputs/apk/debug/app-debug.apk`
relative to the repository root.

The test machine needs the Android SDK/NDK, emulator tools, and usable KVM.
The Android devenv environment is separate from the desktop environment.

## Run through the orchestrator

Prefer the connectivity runner to starting an emulator by hand. It assigns a
run directory, an unused emulator port, a temporary Android Virtual Device
(AVD), and a structured autoplay plan.

Each side needs its own AVD and store. Do not point `ANDROID_AVD_HOME` at a
personal emulator directory for destructive create/delete operations.

The host needs compatible hardened modules. The driver pushes them into the
debug application's storage rather than assuming they were extracted from
the APK.

## Events and failure diagnosis

The application receives the plan through its launch intent before the native
game loop starts. A later intent sent to an already-running activity is not
equivalent. Events are emitted with the `kai.autoplay` log tag.

The driver saves logcat, structured events, screenshots, and its exit status.
Start by comparing the last event with the plan's expected stage.

The harness rotates the emulator before launching the activity to avoid
recreation while the native loop is running. It also avoids passing the
NDK's runtime-library path to the emulator. Preserve those constraints when
reworking startup.

## Cleanup and network limits

Let the driver stop its own process tree and AVD; do not kill unrelated
emulators. Give teardown time to complete before forcing termination.

Emulator networking uses NAT. A successful desktop-only check is not proof
that an Android-hosted game is reachable. Browser and emulator pairs may
require a working relay path.
