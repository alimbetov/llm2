#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir observability

metrics_url="${ASTRAVECTOR_METRICS_URL:-http://127.0.0.1:9090/metrics}"
if ! curl -fsS "$metrics_url" >"$FIX485_EVIDENCE_DIR/metrics.prom"; then
  fix485_write_summary BLOCKED METRICS_ENDPOINT_UNAVAILABLE
  exit "$BLOCKED_STATUS"
fi

required=(
  astravector_query_processing_total
  astravector_query_segment_count
  astravector_retrieval_branch_total
  astravector_admission_wait_seconds
  astravector_admission_in_flight
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
