#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/processes.sh"
load_smoke_env
if [[ -f "$RUNTIME_DIR/runtime.pid" ]]; then
  pid="$(cat "$RUNTIME_DIR/runtime.pid")"
  stop_process runtime
  if kill -0 "$pid" >/dev/null 2>&1; then fail "runtime did not stop"; fi
else
  blocked "runtime was not started"
fi
