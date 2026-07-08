#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
load_smoke_env

command -v cargo >/dev/null 2>&1 || blocked "cargo not found"
cargo run --bin astravector-runtime -- migrate >"$LOGS_DIR/migrations-1.log" 2>&1 || fail "first migration run failed"
cargo run --bin astravector-runtime -- migrate >"$LOGS_DIR/migrations-2.log" 2>&1 || fail "second migration run is not idempotent"

schema="$(psql "$(postgres_url)" -Atf "$SMOKE_ROOT/sql/assert-schema.sql")" || fail "schema assertions failed"
echo "$schema" > "$RESULTS_DIR/schema.tsv"
grep -q '^pgvector_extension|1$' "$RESULTS_DIR/schema.tsv" || fail "pgvector extension missing"
grep -q '^content_chunks_v004|1$' "$RESULTS_DIR/schema.tsv" || fail "content_chunks_v004 missing"

parts="$(psql "$(postgres_url)" -Atf "$SMOKE_ROOT/sql/assert-partitions.sql")" || fail "partition assertions failed"
echo "$parts" > "$RESULTS_DIR/partitions.tsv"
grep -q '^content_chunks_v004_partitions|32$' "$RESULTS_DIR/partitions.tsv" || fail "content_chunks_v004 partition count is not 32"
grep -q '^vector_bindings_v004_partitions|32$' "$RESULTS_DIR/partitions.tsv" || fail "vector_bindings_v004 partition count is not 32"
