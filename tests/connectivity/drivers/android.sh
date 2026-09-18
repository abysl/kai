#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
kai=$(cd "$here/../../.." && pwd)

if [[ $# -lt 2 ]]; then
  echo "usage: android.sh <run-dir> <plan-json>" >&2
  exit 2
fi
run_dir=$1
plan=$2

mkdir -p "$run_dir"
run_dir=$(cd "$run_dir" && pwd)

if [[ -n ${KAI_DRIVER_RESOLVE_ONLY:-} ]]; then
  printf '%s\n' "$run_dir"
  exit 0
fi

if ! command -v emulator >/dev/null 2>&1; then
  cd "$kai/android"
  exec devenv shell -- bash "$here/android.sh" "$run_dir" "$plan"
fi

. "$kai/tests/android/emulator.sh"

printf '%s\n' "$$" >"$run_dir/pid"
printf '%s\n' "$plan" >"$run_dir/plan.json"
export ANDROID_AVD_HOME=$run_dir/avd
port=${KAI_ANDROID_PORT:-$(free_port 5554)}
avd_name=${KAI_ANDROID_AVD:-kai-$port}
serial=$(serial_of "$port")

finish() {
  local code=$?
  set +e
  trap '' INT TERM
  events_stop "$run_dir"
  app_screenshot "$serial" "$run_dir/shot-final.png"
  app_stop "$serial"
  adb -s "$serial" logcat -d >"$run_dir/logcat-full.log" 2>/dev/null
  emu_kill "$run_dir"
  if [[ -z ${KAI_KEEP_AVD:-} ]]; then
    avd_delete "$avd_name"
  fi
  printf '%s\n' "$code" >"$run_dir/exit"
}
trap finish EXIT
trap 'exit 130' INT TERM

timeout_s=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1]).get("timeout_s", 600))' "$plan")
role=$(plan_role "$plan")

avd_create "$avd_name"
emu_boot "$avd_name" "$port" "$run_dir"
apk_install "$serial" "$KAI_ANDROID_APK"
if [[ $role == host ]]; then
  modules_push "$serial" "$KAI_ANDROID_MODULES"
fi
events_stream "$serial" "$run_dir"
app_start "$serial" "$plan" >"$run_dir/am-start.log" 2>&1
echo "$role on $serial (pid $(app_pid "$serial")), events in $run_dir/events.jsonl"

shot_taken=""
cap=$((timeout_s + 30))
waited=0
while ! event_lines "$run_dir" outcome | grep -q .; do
  if [[ -z $shot_taken ]] && event_lines "$run_dir" seated | grep -q .; then
    app_screenshot "$serial" "$run_dir/shot-seated.png"
    shot_taken=1
  fi
  if (( waited >= cap )); then
    echo "no outcome after ${cap}s" >&2
    exit 1
  fi
  if (( waited > 10 )) && [[ -z $(app_pid "$serial") ]]; then
    echo "$KAI_ANDROID_PACKAGE died before an outcome" >&2
    exit 1
  fi
  sleep 1
  waited=$((waited + 1))
done
app_screenshot "$serial" "$run_dir/shot-outcome.png"
result=$(event_field "$run_dir" outcome result)
echo "outcome: $(event_lines "$run_dir" outcome | head -1)"
[[ $result != failed ]]
