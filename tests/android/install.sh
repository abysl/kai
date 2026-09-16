#!/usr/bin/env bash
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/emulator.sh"
[[ $# -ge 1 ]] || die "usage: install.sh <serial> [apk] [--modules [assets-dir]]"
serial=$1
shift
apk=$KAI_ANDROID_APK
if [[ $# -ge 1 && $1 != --modules ]]; then
  apk=$1
  shift
fi
apk_install "$serial" "$apk"
if [[ ${1:-} == --modules ]]; then
  modules_push "$serial" "${2:-$KAI_ANDROID_MODULES}"
fi
