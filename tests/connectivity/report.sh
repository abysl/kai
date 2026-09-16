#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
out=${1:?usage: report.sh <out-dir> [report.json]}
python3 "$here/report.py" "$out" "${2:-$out/report.json}"
