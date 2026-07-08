#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/processes.sh"
load_smoke_env
stop_all_processes
if command -v psql >/dev/null 2>&1; then
  psql "$(postgres_url)" -f "$SMOKE_ROOT/sql/cleanup.sql" >"$LOGS_DIR/cleanup-postgres.log" 2>&1 || true
fi
if command -v curl >/dev/null 2>&1; then
  curl -sS -X DELETE "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}" >/dev/null 2>&1 || true
fi
if command -v docker >/dev/null 2>&1; then
  compose_cmd down -v >"$LOGS_DIR/cleanup-docker.log" 2>&1 || true
fi
rm -f "$RUNTIME_DIR"/*.pid
