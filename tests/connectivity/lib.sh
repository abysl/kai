export PYTHONDONTWRITEBYTECODE=1
CONNECTIVITY_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
KAI_ROOT=$(cd "$CONNECTIVITY_DIR/../.." && pwd)
KAI_BIN=${KAI_BIN:-${CARGO_TARGET_DIR:-$KAI_ROOT/target}/debug/kai}
KAI_WEB_DIST=${KAI_WEB_DIST:-$KAI_ROOT/web/dist}
KAI_APK=${KAI_APK:-$KAI_ROOT/android/app/build/outputs/apk/debug/app-debug.apk}
RELAY_PROBE_URL=${RELAY_PROBE_URL:-https://use1-1.relay.iroh.network/}
GATEWAY_URL=${GATEWAY_URL:-https://dev1.dragon-pierce.ts.net}
GATEWAY_PROBE_URL=${GATEWAY_PROBE_URL:-$GATEWAY_URL/gateway/status}
GATEWAY_MODULES_URL=${GATEWAY_MODULES_URL:-$GATEWAY_URL/gateway/modules}
LAVAPIPE_ICD=${VK_DRIVER_FILES:-/run/opengl-driver/share/vulkan/icd.d/lvp_icd.x86_64.json}
HOSTING_WAIT_S=${HOSTING_WAIT_S:-120}
OUTCOME_GRACE_S=${OUTCOME_GRACE_S:-60}
STOP_GRACE_S=10
ANDROID_STOP_GRACE_S=${ANDROID_STOP_GRACE_S:-60}
EXIT_GRACE_S=${EXIT_GRACE_S:-20}
POLL_S=0.5
XVFB_TRIES=3

log() {
  printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" >&2
}

die() {
  log "$*"
  exit 2
}

now_ms() {
  date +%s%3N
}

now_iso() {
  date -u +%Y-%m-%dT%H:%M:%SZ
}

json_field() {
  python3 -c 'import json,sys; v=json.load(sys.stdin).get(sys.argv[1]); sys.stdout.write("" if v is None else str(v))' "$1"
}

http_answers() {
  python3 - "$1" <<'PY'
import sys, urllib.request
try:
    urllib.request.urlopen(sys.argv[1], timeout=6)
except urllib.error.HTTPError:
    pass
except Exception as error:
    sys.exit(1)
PY
}

kai_assets() {
  local beside
  beside=$(cd "$(dirname "$KAI_BIN")/.." 2>/dev/null && pwd)/share/kai/assets
  if [[ -d $beside ]]; then
    echo "$beside"
  else
    echo "$KAI_ROOT/assets"
  fi
}

probe_internet() {
  http_answers "$RELAY_PROBE_URL" || echo "relay $RELAY_PROBE_URL does not answer"
}

probe_gateway() {
  http_answers "$GATEWAY_PROBE_URL" || { echo "gateway $GATEWAY_PROBE_URL does not answer"; return 0; }
}

probe_web-plugin() {
  [[ -f $KAI_WEB_DIST/assets/plugins/riftbound.wasm ]] || echo "web/dist ships no assets/plugins/riftbound.wasm"
}

probe_kvm() {
  [[ -w /dev/kvm ]] || echo "/dev/kvm is not writable"
}

probe_xvfb() {
  command -v Xvfb >/dev/null 2>&1 || echo "no Xvfb on PATH"
  [[ -f $LAVAPIPE_ICD ]] || echo "no lavapipe icd at $LAVAPIPE_ICD"
}

probe_kai-bin() {
  local assets
  assets=$(kai_assets)
  [[ -x $KAI_BIN ]] || echo "no kai binary at $KAI_BIN"
  [[ -f ${AGNI_ENGINE_WASM:-$assets/engine/engine.wasm} ]] || echo "no engine.wasm under $assets"
  [[ -f ${AGNI_RIFTBOUND_WASM:-$assets/plugins/riftbound.wasm} ]] || echo "no riftbound.wasm under $assets"
}

probe_web-dist() {
  [[ -f $KAI_WEB_DIST/hand.js ]] || echo "no web bundle at $KAI_WEB_DIST"
  [[ -f $KAI_WEB_DIST/engine.wasm ]] || echo "no engine.wasm in $KAI_WEB_DIST"
  grep -q kai_autoplay "$KAI_WEB_DIST/hand.js" 2>/dev/null || echo "$KAI_WEB_DIST/hand.js lacks kai_autoplay"
  [[ -d $KAI_ROOT/tests/web/node_modules ]] || echo "tests/web/node_modules missing (npm ci)"
}

probe_apk() {
  [[ -f $KAI_APK ]] || echo "no debug apk at $KAI_APK"
  [[ -f $KAI_ROOT/android/devenv.nix ]] || echo "no android devenv (emulator, adb)"
}

probe_driver() {
  [[ -x $CONNECTIVITY_DIR/drivers/$1.sh ]] || echo "no driver at tests/connectivity/drivers/$1.sh"
}

preflight() {
  local host=$1 joiner=$2 needs=$3 probe reason
  for probe in driver:"$host" driver:"$joiner" ${needs//,/ }; do
    if [[ $probe == driver:* ]]; then
      reason=$(probe_driver "${probe#driver:}")
    else
      reason=$("probe_$probe" 2>&1 || true)
    fi
    if [[ -n $reason ]]; then
      echo "$probe: ${reason//$'\n'/; }"
      return 0
    fi
  done
}

free_display() {
  local n
  for n in $(seq "${1:-90}" 199); do
    if [[ ! -e /tmp/.X$n-lock && ! -S /tmp/.X11-unix/X$n ]]; then
      echo "$n"
      return 0
    fi
  done
  return 1
}

XVFB_PID=""
try_xvfb() {
  local out=$1 n=$2 waited=0
  Xvfb ":$n" -screen 0 1600x1000x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
  XVFB_PID=$!
  while [[ ! -S /tmp/.X11-unix/X$n ]]; do
    if ! kill -0 "$XVFB_PID" 2>/dev/null; then
      log "Xvfb :$n exited: $(tail -3 "$out/xvfb.log" | tr '\n' ' ')"
      XVFB_PID=""
      return 1
    fi
    if (( waited >= 40 )); then
      log "Xvfb :$n did not come up"
      stop_xvfb
      return 1
    fi
    sleep 0.25
    waited=$((waited + 1))
  done
}

start_xvfb() {
  local out=$1 n from=90 attempt
  if [[ -n ${KAI_DISPLAY:-} ]]; then
    export DISPLAY=$KAI_DISPLAY
    log "using display $DISPLAY"
    return 0
  fi
  for attempt in $(seq 1 "$XVFB_TRIES"); do
    n=$(free_display "$from") || die "no free X display number between :$from and :199"
    if try_xvfb "$out" "$n"; then
      export DISPLAY=":$n"
      log "Xvfb on $DISPLAY (pid $XVFB_PID)"
      return 0
    fi
    from=$((n + 1))
  done
  die "Xvfb did not start in $XVFB_TRIES tries"
}

stop_xvfb() {
  if [[ -n $XVFB_PID ]]; then
    kill "$XVFB_PID" 2>/dev/null || true
    XVFB_PID=""
  fi
}

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

WEB_SERVER_PID=""
start_web_server() {
  local out=$1 port
  if [[ -n ${KAI_WEB_URL:-} ]]; then
    log "serving nothing: KAI_WEB_URL=$KAI_WEB_URL"
    return 0
  fi
  port=$(free_port)
  python3 -m http.server "$port" --bind 127.0.0.1 -d "$KAI_WEB_DIST" >"$out/web-serve.log" 2>&1 &
  WEB_SERVER_PID=$!
  export KAI_WEB_URL="http://127.0.0.1:$port/"
  local waited=0
  until http_answers "$KAI_WEB_URL"; do
    kill -0 "$WEB_SERVER_PID" 2>/dev/null || die "web server exited: $(tail -3 "$out/web-serve.log")"
    (( waited < 40 )) || die "web server on $KAI_WEB_URL did not answer"
    sleep 0.25
    waited=$((waited + 1))
  done
  log "serving $KAI_WEB_DIST at $KAI_WEB_URL (pid $WEB_SERVER_PID)"
}

stop_web_server() {
  if [[ -n $WEB_SERVER_PID ]]; then
    kill "$WEB_SERVER_PID" 2>/dev/null || true
    WEB_SERVER_PID=""
  fi
}

start_driver() {
  local side=$1 platform=$2 dir=$3 plan=$4
  rm -rf "$dir"
  mkdir -p "$dir"
  printf '%s' "$plan" >"$dir/plan.json"
  : >"$dir/events.jsonl"
  now_iso >"$dir/started_at"
  (
    export KAI_SIDE=$side
    export KAI_RUN_DIR=$dir
    export KAI_PLAN=$plan
    export KAI_TIMEOUT_S
    export KAI_APK
    export KAI_ANDROID_APK=$KAI_APK
    export KAI_AVD_NAME="kai-$side"
    export KAI_ANDROID_AVD="kai-$side"
    export KAI_ANDROID_MODULES=${KAI_ANDROID_MODULES:-$(kai_assets)}
    export AGNI_ENGINE_WASM=${AGNI_ENGINE_WASM:-$(kai_assets)/engine/engine.wasm}
    export AGNI_RIFTBOUND_WASM=${AGNI_RIFTBOUND_WASM:-$(kai_assets)/plugins/riftbound.wasm}
    exec "$CONNECTIVITY_DIR/drivers/$platform.sh" "$dir" "$plan"
  ) >"$dir/driver.log" 2>&1 &
  echo $! >"$dir/driver.pid"
}

driver_alive() {
  local dir=$1 pid
  pid=$(cat "$dir/driver.pid" 2>/dev/null) || return 1
  kill -0 "$pid" 2>/dev/null
}

stop_driver() {
  local dir=$1 pid waited=0 grace=$STOP_GRACE_S
  [[ -d $dir ]] || return 0
  [[ -f $dir/emulator.pid ]] && grace=$ANDROID_STOP_GRACE_S
  for pid in $(cat "$dir/pid" "$dir/driver.pid" 2>/dev/null | sort -u); do
    kill -TERM "$pid" 2>/dev/null || true
  done
  while driver_alive "$dir" && (( waited < grace * 4 )); do
    sleep 0.25
    waited=$((waited + 1))
  done
  for pid in $(cat "$dir/pid" "$dir/driver.pid" "$dir/kai.pid" "$dir/events.pid" "$dir/emulator.pid" 2>/dev/null | sort -u); do
    kill -KILL "$pid" 2>/dev/null || true
  done
  if [[ -f $dir/driver.pid ]]; then
    wait "$(cat "$dir/driver.pid")" 2>/dev/null || true
  fi
  now_iso >"$dir/ended_at"
}

has_event() {
  python3 "$CONNECTIVITY_DIR/events.py" "$1/events.jsonl" has "$2"
}

event_field() {
  python3 "$CONNECTIVITY_DIR/events.py" "$1/events.jsonl" field "$2" "$3"
}

wait_event() {
  local dir=$1 event=$2 cap_s=$3 started
  started=$(now_ms)
  until has_event "$dir" "$event"; do
    if ! driver_alive "$dir"; then
      sleep 1
      has_event "$dir" "$event" && return 0
      echo "$(basename "$dir") driver exited (status $(cat "$dir/exit" 2>/dev/null || echo '?')) before $event"
      return 1
    fi
    if (( $(now_ms) - started > cap_s * 1000 )); then
      echo "$(basename "$dir") gave no $event within ${cap_s}s"
      return 1
    fi
    sleep "$POLL_S"
  done
}

wait_outcomes() {
  local host_dir=$1 joiner_dir=$2 cap_s=$3 started first_at="" dir
  started=$(now_ms)
  while true; do
    if has_event "$host_dir" outcome && has_event "$joiner_dir" outcome; then
      return 0
    fi
    if [[ -z $first_at ]] && { has_event "$host_dir" outcome || has_event "$joiner_dir" outcome; }; then
      first_at=$(now_ms)
    fi
    for dir in "$host_dir" "$joiner_dir"; do
      if ! driver_alive "$dir" && ! has_event "$dir" outcome; then
        sleep 1
        has_event "$dir" outcome && continue
        echo "$(basename "$dir") driver exited (status $(cat "$dir/exit" 2>/dev/null || echo '?')) without an outcome"
        return 1
      fi
    done
    if (( $(now_ms) - started > cap_s * 1000 )); then
      echo "no outcome on both sides within ${cap_s}s"
      return 1
    fi
    if [[ -n $first_at ]] && (( $(now_ms) - first_at > OUTCOME_GRACE_S * 1000 )); then
      echo "the other side gave no outcome within ${OUTCOME_GRACE_S}s of the first"
      return 1
    fi
    sleep "$POLL_S"
  done
}

let_drivers_exit() {
  local waited=0 dir alive
  while (( waited < EXIT_GRACE_S * 4 )); do
    alive=0
    for dir in "$@"; do
      driver_alive "$dir" && alive=1
    done
    (( alive )) || return 0
    sleep 0.25
    waited=$((waited + 1))
  done
}

build_identity() {
  python3 - "$KAI_ROOT" <<'PY'
import json, re, subprocess, sys
root = sys.argv[1]
def sh(*args):
    try:
        return subprocess.run(args, cwd=root, capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return None
version = re.search(r'^version\s*=\s*"([^"]+)"', open(f"{root}/Cargo.toml").read(), re.M)
wire = None
try:
    wire = re.search(r'WIRE_VERSION:\s*u32\s*=\s*(\d+)', open(f"{root}/../agni/net/src/proto.rs").read())
except OSError:
    pass
print(json.dumps({
    "commit": sh("git", "rev-parse", "HEAD"),
    "version": version.group(1) if version else None,
    "wire": int(wire.group(1)) if wire else None,
}))
PY
}
