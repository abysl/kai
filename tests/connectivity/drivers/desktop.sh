#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
kai_root=$(cd "$here/../../.." && pwd)
run_dir=${1:?usage: desktop.sh <run-dir> <plan-json>}
plan=${2:?usage: desktop.sh <run-dir> <plan-json>}

kai_bin=${KAI_BIN:-${CARGO_TARGET_DIR:-$kai_root/target}/debug/kai}
name=$(printf '%s' "$plan" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("name", "autoplay"))')

refuse() {
  echo "desktop driver: $*" >&2
  mkdir -p "$run_dir"
  echo 2 >"$run_dir/exit"
  exit 2
}

assets_dir() {
  local beside
  beside=$(cd "$(dirname "$kai_bin")/.." 2>/dev/null && pwd)/share/kai/assets
  if [[ -d $beside ]]; then
    echo "$beside"
  else
    echo "$kai_root/assets"
  fi
}

library_path() {
  local parts=("${LD_LIBRARY_PATH:-}" "$(dirname "$kai_bin")/deps")
  if command -v rustc >/dev/null 2>&1; then
    parts+=("$(rustc --print sysroot)/lib")
  fi
  local joined
  joined=$(printf '%s:' "${parts[@]}")
  echo "${joined%:}"
}

[[ -x $kai_bin ]] || refuse "no kai binary at $kai_bin (build it, or set KAI_BIN)"
[[ -n ${DISPLAY:-} ]] || refuse "DISPLAY is not set (the runner owns the Xvfb)"
assets=$(assets_dir)
engine=${AGNI_ENGINE_WASM:-$assets/engine/engine.wasm}
riftbound=${AGNI_RIFTBOUND_WASM:-$assets/plugins/riftbound.wasm}
[[ -f $engine ]] || refuse "no engine module at $engine (engine-build, or set AGNI_ENGINE_WASM)"
[[ -f $riftbound ]] || refuse "no riftbound plugin at $riftbound (plugin-build, or set AGNI_RIFTBOUND_WASM)"

mkdir -p "$run_dir/store" "$run_dir/xdg"
echo $$ >"$run_dir/pid"
: >"$run_dir/events.jsonl"
fifo=$run_dir/stdout.fifo
rm -f "$fifo"
mkfifo "$fifo"

tee "$run_dir/stdout.log" <"$fifo" \
  | grep --line-buffered '^KAI_EVENT ' \
  | sed -u 's/^KAI_EVENT //' \
  | python3 -u "$here/../stamp.py" "$run_dir/events.bad" >>"$run_dir/events.jsonl" &
reader_pid=$!

(
  export DISPLAY
  export VK_DRIVER_FILES=${VK_DRIVER_FILES:-/run/opengl-driver/share/vulkan/icd.d/lvp_icd.x86_64.json}
  export WGPU_BACKEND=${WGPU_BACKEND:-vulkan}
  export KAI_WINDOW=${KAI_WINDOW:-1280x800}
  export KAI_SHOT=$run_dir/shot.png
  export KAI_AUTOPLAY=$plan
  export USER=$name
  export SPIRIT_STORE=$run_dir/store
  export XDG_CONFIG_HOME=$run_dir/xdg
  export AGNI_ENGINE_WASM=$engine
  export AGNI_RIFTBOUND_WASM=$riftbound
  export RUST_LOG=${RUST_LOG:-info,kai=debug,agni_net=debug}
  export LD_LIBRARY_PATH
  LD_LIBRARY_PATH=$(library_path)
  unset AGNI_TUNING KAI_INGEST_TOKEN
  cd "$kai_root"
  exec "$kai_bin" >"$fifo" 2>"$run_dir/stderr.log"
) &
kai_pid=$!
echo "$kai_pid" >"$run_dir/kai.pid"

stop() {
  kill "$kai_pid" 2>/dev/null || true
}
trap stop TERM INT

set +e
wait "$kai_pid"
status=$?
while kill -0 "$kai_pid" 2>/dev/null; do
  wait "$kai_pid"
  status=$?
done
set -e
trap - TERM INT
wait "$reader_pid" || true
rm -f "$fifo"
echo "$status" >"$run_dir/exit"
exit "$status"
