#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
"$SMOKE_ROOT/scripts/39-rag-retrieval-expert-smoke.sh" || fail "base RAG retrieval expert failed"
cp "$REPORTS_DIR/rag-retrieval-candidates.jsonl" "$REPORTS_DIR/full-power-rag-candidates.jsonl"
cp "$REPORTS_DIR/rag-retrieval-results.json" "$REPORTS_DIR/full-power-rag-quality.json"
