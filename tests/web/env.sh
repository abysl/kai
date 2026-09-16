#!/usr/bin/env bash
set -euo pipefail

playwright_browsers() {
  if [ -n "${PLAYWRIGHT_BROWSERS_PATH:-}" ]; then
    printf '%s\n' "$PLAYWRIGHT_BROWSERS_PATH"
    return
  fi
  nix build --inputs-from "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)" \
    nixpkgs#playwright-driver.browsers --no-link --print-out-paths
}

PLAYWRIGHT_BROWSERS_PATH="$(playwright_browsers)"
export PLAYWRIGHT_BROWSERS_PATH
export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
