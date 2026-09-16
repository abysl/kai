#!/usr/bin/env bash
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/emulator.sh"
case ${1:-} in
  create) avd_create "$2" ;;
  delete) avd_delete "$2" ;;
  *) die "usage: avd.sh create|delete <name>   (ANDROID_AVD_HOME must point under the run dir)" ;;
esac
