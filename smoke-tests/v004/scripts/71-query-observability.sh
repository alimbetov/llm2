#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir observability

metrics_url="${ASTRAVECTOR_METRICS_URL:-http://127.0.0.1:9090/metrics}"
endpoint="${ASTRAVECTOR_QUALITY_ENDPOINT:-http://127.0.0.1:50051}"
grpc_target="${endpoint#http://}"
grpc_target="${grpc_target#https://}"
if ! curl -fsS "$metrics_url" >"$FIX485_EVIDENCE_DIR/metrics-before.prom"; then
  fix485_write_summary BLOCKED METRICS_ENDPOINT_UNAVAILABLE
  exit "$BLOCKED_STATUS"
fi

before_total="$(awk '/^astravector_query_total\{/ {sum += $NF} END {print sum + 0}' "$FIX485_EVIDENCE_DIR/metrics-before.prom")"
request='{"correlationId":"fix485-observability","accessZoneCode":"1700","callerAccessLevel":"PUBLIC","query":"How are missing Qdrant points repaired from PostgreSQL?","topK":3,"candidateLimit":30,"parentLimit":3,"timeoutMs":10000,"searchMode":"SEARCH_MODE_V005_DENSE","embeddingMode":"EMBEDDING_MODE_V005_DENSE_ONLY","includeDebug":true}'
if ! grpcurl -plaintext -d "$request" "$grpc_target" astravector.embedding.v1.AstraVectorV004Control/Search >"$FIX485_EVIDENCE_DIR/search.json" 2>"$FIX485_EVIDENCE_DIR/search.err"; then
  fix485_write_summary FAIL OBSERVABILITY_PROBE_SEARCH_FAILED
  exit "$FAIL_STATUS"
fi
if ! curl -fsS "$metrics_url" >"$FIX485_EVIDENCE_DIR/metrics.prom"; then
  fix485_write_summary FAIL METRICS_ENDPOINT_LOST_AFTER_QUERY
  exit "$FAIL_STATUS"
fi
after_total="$(awk '/^astravector_query_total\{/ {sum += $NF} END {print sum + 0}' "$FIX485_EVIDENCE_DIR/metrics.prom")"
if ! awk -v before="$before_total" -v after="$after_total" 'BEGIN { exit !(after > before) }'; then
  fix485_write_summary FAIL QUERY_COUNTER_DID_NOT_INCREASE
  exit "$FAIL_STATUS"
fi

required=(
  astravector_query_total
  astravector_query_duration_seconds
  astravector_query_planning_duration_seconds
  astravector_query_processing_total
  astravector_query_segment_count
  astravector_query_intent_count
  astravector_retrieval_branch_total
  astravector_admission_wait_seconds
  astravector_admission_in_flight
  astravector_admission_rejected_total
  astravector_work_units_in_flight
  astravector_intent_coverage_ratio
  astravector_graph_seed_count
  astravector_mmr_skipped_total
  astravector_long_query_coverage_after_direct
  astravector_query_degraded_total
  astravector_optional_stage_skipped_total
  graph_mmr_selected_total
)
missing=()
for metric in "${required[@]}"; do
  grep -q "^${metric}" "$FIX485_EVIDENCE_DIR/metrics.prom" || missing+=("$metric")
done
printf '%s\n' "${missing[@]:-}" >"$FIX485_EVIDENCE_DIR/missing-metrics.txt"
if (( ${#missing[@]} > 0 )); then
  fix485_write_summary FAIL REQUIRED_QUERY_METRICS_MISSING
  exit "$FAIL_STATUS"
fi
if rg -i '(api[_-]?key|password|bearer |select .+ from|raw_query|document_text)' "$FIX485_EVIDENCE_DIR/metrics.prom" >"$FIX485_EVIDENCE_DIR/privacy-findings.log"; then
  fix485_write_summary FAIL METRICS_PRIVACY_VIOLATION
  exit "$FAIL_STATUS"
fi
fix485_write_summary PASS QUERY_OBSERVABILITY_CONTRACT_PASSED
exit "$PASS"
