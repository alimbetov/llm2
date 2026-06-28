#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/qdrant.sh"
load_smoke_env

command -v docker >/dev/null 2>&1 || fail "docker not found"
compose_cmd up -d postgres qdrant
wait_for_postgres 90
wait_for_qdrant 90
ensure_qdrant_collection || fail "failed to create/check Qdrant collection"
