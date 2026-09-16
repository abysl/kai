# Run a two-client connectivity check

Audience: developers testing multiplayer changes. Assumes Kai builds locally;
see [development](../../wiki/development.md) first.

The suite launches a host and joiner, drives them with structured autoplay
plans, and compares outcomes. It tests the path between clients, not complete
game-rule correctness.

## Start with desktop to desktop

From the repository root:

```sh
devenv shell -- modules-build
devenv shell -- cargo build --bin kai --features fast-compile
devenv shell -- connectivity --only desktop:desktop
```

You also need Xvfb and the lavapipe software Vulkan driver. Set
`VK_DRIVER_FILES` if your driver is not at the harness's default path.
`KAI_BIN` can select an already-built binary; `AGNI_ENGINE_WASM` and
`AGNI_RIFTBOUND_WASM` can select compatible hardened modules.

The runner probes requirements and may skip a case. Read the report: an exit
without a failure does not mean every requested platform ran.

## Add browser or Android cases

Browser cases need a built web bundle, the tests' Node dependencies and browser
runtime, and reachable relay services. Set `KAI_WEB_DIST` or `KAI_WEB_URL`
for your test build. If the scenario needs a gateway, configure `GATEWAY_URL`
for a service you operate; no private gateway is a prerequisite.

Android cases need an x86_64 debug APK, the Android tools, and usable KVM.
See the [Android harness guide](../android/README.md).

Use `bash tests/connectivity/run.sh --list` to inspect cases and
`--only desktop:web` to select one. Case names and prerequisites live in
`matrix.json`.

## Read the results

Outputs are written under the selected run directory, by default
`target/connectivity/<stamp>/` (or the configured Cargo target directory).
Look at `summary.txt`, `report.json`, and `junit.xml`, then the failing
side's raw log and event stream.

The suite checks seating, progression, and agreement between the two results.
An absent or failed outcome is a failure. Unsupported prerequisites are skips.
Use `--keep` only when you need temporary stores for diagnosis; do not publish
those stores or logs without checking for private content.

The [design guide](../../wiki/design/connectivity.md) explains driver ownership
and how to add a case.
