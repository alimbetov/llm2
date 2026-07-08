#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
load_smoke_env
command -v psql >/dev/null 2>&1 || blocked "psql not found"
psql "$(postgres_url)" -At -F $'\t' -f "$SMOKE_ROOT/sql/assert-no-orphans.sql" >"$LOGS_DIR/no-orphans.tsv" || fail "orphan SQL failed"
grep -q $'bindings_without_chunks\t0' "$LOGS_DIR/no-orphans.tsv" || fail "orphan chunk bindings found"
grep -q $'bindings_without_cache\t0' "$LOGS_DIR/no-orphans.tsv" || fail "orphan cache bindings found"
