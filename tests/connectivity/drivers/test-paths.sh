#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

plan='{"role":"join","brain":{"random":{"seed":7}},"until":"winner","timeout_s":30,"name":"paths"}'
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

kai="$work/kai"
caller="$work/caller"
mkdir -p "$kai/tests/connectivity/drivers" "$kai/tests/web/node_modules/@playwright/test" \
  "$kai/web/dist" "$kai/android" "$kai/assets/engine" "$kai/assets/plugins" "$caller"
cp "$here/web.sh" "$here/android.sh" "$here/desktop.sh" "$kai/tests/connectivity/drivers/"
cp "$here/../stamp.py" "$kai/tests/connectivity/"
: >"$kai/tests/web/env.sh"
printf 'kai_autoplay\n' >"$kai/web/dist/hand.js"
: >"$kai/web/dist/engine.wasm"
: >"$kai/assets/engine/engine.wasm"
: >"$kai/assets/plugins/riftbound.wasm"

stubs="$work/stubs"
mkdir -p "$stubs"
cat >"$stubs/npx" <<'STUB'
#!/usr/bin/env bash
pwd >"$STUB_RECORD"
exit 0
STUB
cat >"$stubs/devenv" <<'STUB'
#!/usr/bin/env bash
printf 'devenv-cwd=%s\n' "$PWD" >"$STUB_RECORD"
for arg in "$@"; do printf 'arg=%s\n' "$arg" >>"$STUB_RECORD"; done
exit 0
STUB
cat >"$stubs/kai" <<'STUB'
#!/usr/bin/env bash
printf 'cwd=%s\nshot=%s\nstore=%s\n' "$PWD" "$KAI_SHOT" "$SPIRIT_STORE" >"$KAI_SHOT.record"
exit 0
STUB
chmod +x "$stubs/npx" "$stubs/devenv" "$stubs/kai"

driver() {
  local name=$1 record=$2
  (
    cd "$caller"
    PATH="$stubs:$PATH" KAI_BIN="$stubs/kai" DISPLAY=:99 STUB_RECORD="$record" \
      timeout 20 bash "$kai/tests/connectivity/drivers/$name.sh" "target/connectivity/$name-demo" "$plan"
  )
}

web_driver_places_output_across_its_cwd_change() {
  local run_dir="$caller/target/connectivity/web-demo"
  driver web "$work/web-stub.log"
  [[ -f "$run_dir/exit" && "$(cat "$run_dir/exit")" == 0 ]]
  [[ -f "$run_dir/stdout.log" && -f "$run_dir/stderr.log" && -f "$run_dir/pid" ]]
  [[ "$(cat "$work/web-stub.log")" == "$kai/tests/web" ]]
  [[ ! -e "$kai/tests/web/target" ]]
}

android_driver_reexec_forwards_absolute_run_dir() {
  local run_dir="$caller/target/connectivity/android-demo"
  driver android "$work/android-stub.log"
  [[ -d "$run_dir" ]]
  grep -qxF "devenv-cwd=$kai/android" "$work/android-stub.log"
  grep -qxF "arg=$run_dir" "$work/android-stub.log"
  ! grep -qxF "arg=target/connectivity/android-demo" "$work/android-stub.log"
}

desktop_driver_launches_client_with_caller_absolute_paths() {
  local run_dir="$caller/target/connectivity/desktop-demo"
  driver desktop "$work/desktop-stub.log"
  [[ -f "$run_dir/exit" && "$(cat "$run_dir/exit")" == 0 ]]
  [[ -f "$run_dir/stderr.log" && -f "$run_dir/stdout.log" && -f "$run_dir/events.jsonl" ]]
  [[ -f "$run_dir/shot.png.record" ]]
  grep -qxF "cwd=$kai" "$run_dir/shot.png.record"
  grep -qxF "shot=$run_dir/shot.png" "$run_dir/shot.png.record"
  grep -qxF "store=$run_dir/store" "$run_dir/shot.png.record"
}

web_driver_places_output_across_its_cwd_change
android_driver_reexec_forwards_absolute_run_dir
desktop_driver_launches_client_with_caller_absolute_paths

echo "driver run-dir resolution: ok"
