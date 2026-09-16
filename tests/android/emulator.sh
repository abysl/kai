#!/usr/bin/env bash

KAI_ANDROID_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
KAI_ANDROID_IMAGE=${KAI_ANDROID_IMAGE:-system-images;android-34;google_apis_playstore;x86_64}
KAI_ANDROID_DEVICE=${KAI_ANDROID_DEVICE:-pixel_6}
KAI_ANDROID_PACKAGE=blue.rae.kai
KAI_ANDROID_ACTIVITY=.MainActivity
KAI_ANDROID_LOG_TAG=kai.autoplay
KAI_ANDROID_BOOT_CAP_S=${KAI_ANDROID_BOOT_CAP_S:-240}
KAI_ANDROID_ROTATE_CAP_S=${KAI_ANDROID_ROTATE_CAP_S:-30}
KAI_ANDROID_APK=${KAI_ANDROID_APK:-$KAI_ANDROID_DIR/android/app/build/outputs/apk/debug/app-debug.apk}
KAI_ANDROID_MODULES=${KAI_ANDROID_MODULES:-$KAI_ANDROID_DIR/assets}

die() {
  echo "$*" >&2
  exit 2
}

need_tool() {
  local tool
  for tool in "$@"; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool not on PATH; run under 'cd android && devenv shell'"
  done
}

sdk_root() {
  if [[ -n ${ANDROID_HOME:-} ]]; then
    printf '%s' "$ANDROID_HOME"
  elif [[ -n ${ANDROID_SDK_ROOT:-} ]]; then
    printf '%s' "$ANDROID_SDK_ROOT"
  else
    die "ANDROID_HOME unset; run under 'cd android && devenv shell'"
  fi
}

image_dir() {
  printf '%s/%s' "$(sdk_root)" "$(printf '%s' "$KAI_ANDROID_IMAGE" | tr ';' '/')"
}

require_image() {
  [[ -d $(image_dir) ]] || die "system image $KAI_ANDROID_IMAGE is not in the sdk at $(sdk_root); add it to android/devenv.nix (android.systemImageTypes / android.abis)"
}

avd_home() {
  [[ -n ${ANDROID_AVD_HOME:-} ]] || die "ANDROID_AVD_HOME unset; the avd belongs under the run dir"
  mkdir -p "$ANDROID_AVD_HOME"
  printf '%s' "$ANDROID_AVD_HOME"
}

avd_create() {
  local name=$1 home config
  need_tool avdmanager
  require_image
  home=$(avd_home)
  avdmanager --silent create avd -n "$name" -k "$KAI_ANDROID_IMAGE" -d "$KAI_ANDROID_DEVICE" --force </dev/null
  config="$home/$name.avd/config.ini"
  avd_set "$config" hw.keyboard yes
  avd_set "$config" hw.gpu.enabled yes
  avd_set "$config" hw.gpu.mode swiftshader_indirect
  avd_set "$config" hw.lcd.density 420
  avd_set "$config" disk.dataPartition.size 2G
  avd_set "$config" hw.audioInput no
  avd_set "$config" hw.audioOutput no
  avd_set "$config" hw.initialOrientation landscape
}

avd_set() {
  local config=$1 key=$2 value=$3
  if grep -q "^$key=" "$config"; then
    sed -i "s|^$key=.*|$key=$value|" "$config"
  else
    printf '%s=%s\n' "$key" "$value" >>"$config"
  fi
}

avd_delete() {
  local name=$1
  need_tool avdmanager
  avd_home >/dev/null
  avdmanager --silent delete avd -n "$name" </dev/null || true
}

serial_of() {
  printf 'emulator-%s' "$1"
}

emu_boot() {
  local name=$1 port=$2 dir=$3 serial pid
  need_tool emulator adb
  avd_home >/dev/null
  mkdir -p "$dir"
  serial=$(serial_of "$port")
  env -u LD_LIBRARY_PATH emulator -avd "$name" -port "$port" \
    -no-window -no-audio -no-boot-anim -no-snapshot -no-metrics \
    -gpu swiftshader_indirect -accel on -netdelay none -netspeed full \
    >"$dir/emulator.log" 2>&1 </dev/null &
  pid=$!
  printf '%s\n' "$pid" >"$dir/emulator.pid"
  printf '%s\n' "$serial" >"$dir/serial"
  emu_wait_boot "$serial" "$pid"
}

emu_wait_boot() {
  local serial=$1 pid=$2 waited=0 booted
  while true; do
    kill -0 "$pid" 2>/dev/null || die "emulator $serial exited before boot"
    booted=$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r' || true)
    if [[ $booted == 1 ]]; then
      break
    fi
    if (( waited >= KAI_ANDROID_BOOT_CAP_S )); then
      die "emulator $serial did not boot in ${KAI_ANDROID_BOOT_CAP_S}s"
    fi
    sleep 2
    waited=$((waited + 2))
  done
  adb -s "$serial" shell settings put global window_animation_scale 0 >/dev/null
  adb -s "$serial" shell settings put global transition_animation_scale 0 >/dev/null
  adb -s "$serial" shell settings put global animator_duration_scale 0 >/dev/null
  adb -s "$serial" shell settings put system screen_off_timeout 2147483647 >/dev/null
  adb -s "$serial" shell settings put system accelerometer_rotation 0 >/dev/null
  adb -s "$serial" shell settings put system user_rotation 1 >/dev/null
  adb -s "$serial" shell wm fixed-to-user-rotation enabled >/dev/null 2>&1 || true
  adb -s "$serial" shell wm user-rotation lock 1 >/dev/null 2>&1 || true
  adb -s "$serial" shell input keyevent KEYCODE_WAKEUP >/dev/null
  adb -s "$serial" shell wm dismiss-keyguard >/dev/null 2>&1 || true
  wait_landscape "$serial"
  echo "$serial booted in ${waited}s"
}

display_rotation() {
  local serial=$1
  adb -s "$serial" shell dumpsys window displays 2>/dev/null \
    | tr -d '\r' \
    | grep -o 'mRotation=[A-Z_0-9]*' \
    | head -1 \
    | cut -d= -f2
}

wait_landscape() {
  local serial=$1 waited=0 rotation
  while true; do
    rotation=$(display_rotation "$serial")
    case $rotation in
      ROTATION_90|ROTATION_270|1|3) echo "$serial display $rotation"; return 0 ;;
    esac
    if (( waited >= KAI_ANDROID_ROTATE_CAP_S )); then
      echo "$serial display still $rotation after ${KAI_ANDROID_ROTATE_CAP_S}s; the activity may relaunch" >&2
      return 0
    fi
    adb -s "$serial" shell wm user-rotation lock 1 >/dev/null 2>&1 || true
    sleep 1
    waited=$((waited + 1))
  done
}

apk_install() {
  local serial=$1 apk=${2:-$KAI_ANDROID_APK}
  need_tool adb
  [[ -f $apk ]] || die "no apk at $apk; build it with 'cd android && devenv shell -- android-build-emulator'"
  adb -s "$serial" install -r -g "$apk" >/dev/null
}

modules_push() {
  local serial=$1 dir=${2:-$KAI_ANDROID_MODULES} engine plugin
  need_tool adb
  engine="$dir/engine/engine.wasm"
  plugin="$dir/plugins/riftbound.wasm"
  [[ -f $engine ]] || die "no engine at $engine (devenv shell -- engine-build)"
  [[ -f $plugin ]] || die "no plugin at $plugin (devenv shell -- plugin-build)"
  adb -s "$serial" push "$engine" /data/local/tmp/engine.wasm >/dev/null
  adb -s "$serial" push "$plugin" /data/local/tmp/riftbound.wasm >/dev/null
  adb -s "$serial" shell run-as "$KAI_ANDROID_PACKAGE" sh -c \
    "'mkdir -p files/spirit-store && cp /data/local/tmp/engine.wasm /data/local/tmp/riftbound.wasm files/spirit-store/'"
}

plan_role() {
  python3 -c 'import json,sys; r=json.loads(sys.argv[1])["role"]; print(r if isinstance(r,str) else next(iter(r)))' "$1"
}

app_start() {
  local serial=$1 plan=$2 encoded
  need_tool adb
  encoded=$(printf '%s' "$plan" | base64 -w0)
  adb -s "$serial" shell am force-stop "$KAI_ANDROID_PACKAGE"
  adb -s "$serial" shell am start -W -n "$KAI_ANDROID_PACKAGE/$KAI_ANDROID_ACTIVITY" \
    --es "$(autoplay_extra_b64)" "$encoded"
}

autoplay_extra_b64() {
  printf 'autoplay_b64'
}

app_pid() {
  local serial=$1
  adb -s "$serial" shell pidof "$KAI_ANDROID_PACKAGE" 2>/dev/null | tr -d '\r '
}

stamp_events() {
  local line
  while IFS= read -r line; do
    printf '{"at":%s,%s\n' "$(date +%s%3N)" "${line#\{}"
  done
}

events_stream() {
  local serial=$1 dir=$2 pid
  need_tool adb
  mkdir -p "$dir"
  touch "$dir/events.jsonl"
  adb -s "$serial" logcat -c
  (
    adb -s "$serial" logcat -v raw -s "$KAI_ANDROID_LOG_TAG:I" \
      | tee -a "$dir/logcat.log" \
      | grep --line-buffered -o 'KAI_EVENT {.*}' \
      | sed -u 's/^KAI_EVENT //' \
      | stamp_events >>"$dir/events.jsonl"
  ) &
  pid=$!
  printf '%s\n' "$pid" >"$dir/events.pid"
}

events_stop() {
  local dir=$1 pid
  [[ -f $dir/events.pid ]] || return 0
  pid=$(cat "$dir/events.pid")
  pkill -P "$pid" 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  rm -f "$dir/events.pid"
}

event_lines() {
  local dir=$1 event=$2
  grep "\"event\":\"$event\"" "$dir/events.jsonl" 2>/dev/null || true
}

wait_for_event() {
  local dir=$1 event=$2 cap=$3 serial=${4:-} waited=0
  while ! event_lines "$dir" "$event" | grep -q .; do
    if (( waited >= cap )); then
      echo "no $event in $dir after ${cap}s" >&2
      return 1
    fi
    if [[ -n $serial && -z $(app_pid "$serial") && $waited -gt 5 ]]; then
      echo "$KAI_ANDROID_PACKAGE is no longer running on $serial before $event" >&2
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
}

event_field() {
  local dir=$1 event=$2 field=$3
  event_lines "$dir" "$event" | head -1 \
    | python3 -c 'import json,sys; print(json.loads(sys.stdin.read())[sys.argv[1]])' "$field"
}

app_screenshot() {
  local serial=$1 out=$2
  adb -s "$serial" exec-out screencap -p >"$out" 2>/dev/null || true
}

app_stop() {
  local serial=$1
  adb -s "$serial" shell am force-stop "$KAI_ANDROID_PACKAGE" 2>/dev/null || true
}

emu_kill() {
  local dir=$1 pid serial waited=0
  [[ -f $dir/emulator.pid ]] || return 0
  pid=$(cat "$dir/emulator.pid")
  serial=$(cat "$dir/serial" 2>/dev/null || true)
  if [[ -n $serial ]]; then
    adb -s "$serial" emu kill >/dev/null 2>&1 || true
  fi
  while kill -0 "$pid" 2>/dev/null && (( waited < 20 )); do
    sleep 1
    waited=$((waited + 1))
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    sleep 2
    kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$dir/emulator.pid"
}

free_port() {
  local port=${1:-5554}
  while adb devices 2>/dev/null | grep -q "^emulator-$port\b"; do
    port=$((port + 2))
  done
  printf '%s' "$port"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  set -euo pipefail
  command=${1:-}
  shift || true
  case $command in
    create) avd_create "$@" ;;
    delete) avd_delete "$@" ;;
    boot) emu_boot "$@" ;;
    install) apk_install "$@" ;;
    modules) modules_push "$@" ;;
    start) app_start "$@" ;;
    events) events_stream "$@" ;;
    wait) wait_for_event "$@" ;;
    field) event_field "$@" ;;
    kill) emu_kill "$@" ;;
    free-port) free_port "$@" ;;
    *) die "usage: emulator.sh create|delete|boot|install|modules|start|events|wait|field|kill|free-port ..." ;;
  esac
fi
