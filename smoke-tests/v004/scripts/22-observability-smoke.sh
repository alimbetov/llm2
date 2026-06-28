#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
load_smoke_env
metrics_url="http://${ASTRAVECTOR_METRICS_HOST:-127.0.0.1}:${ASTRAVECTOR_METRICS_PORT:-59090}/metrics"
if ! assert_http_status "$metrics_url" 200; then
  blocked "metrics endpoint unavailable"
  exit "$BLOCKED_STATUS"
fi
metrics="$(curl -sS "$metrics_url")" || fail "metrics fetch failed"
grep -E 'astravector_' <<<"$metrics" >"$LOGS_DIR/metrics.txt" || fail "no astravector metrics exposed"
if grep -R -E "astravector_smoke_password|smoke-local-only|Срок исковой давности составляет три года" "$LOGS_DIR" >/dev/null 2>&1; then
  fail "logs contain secret or full fixture text"
fi
