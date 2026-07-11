#!/usr/bin/env bash
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="${ASTRAVECTOR_QUALITY_OUTPUT_DIR:-$ROOT_DIR/target/quality-reports}"
export ASTRAVECTOR_QUALITY_OUTPUT_DIR="$REPORT_DIR"
BASELINE_FILE="$ROOT_DIR/benchmarks/quality/baseline/hard-negative-baseline.json"
RUNTIME_REPORT="$REPORT_DIR/runtime-quality-report.json"
CONFIDENCE_JSON="$REPORT_DIR/runtime-confidence-report.json"
CONFIDENCE_MD="$REPORT_DIR/runtime-confidence-report.md"
FINAL_READINESS_JSON="$REPORT_DIR/final-readiness-report.json"
ENDPOINT="${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051}"
GRPC_HOST="${ASTRAVECTOR_QUALITY_GRPC_HOST:-127.0.0.1:50051}"
TIMEOUT_SECONDS="${CONFIDENCE_GATE_TIMEOUT_SECONDS:-300}"
DIAGNOSTIC_ONLY="${ASTRAVECTOR_QUALITY_CONFIDENCE_DIAGNOSTIC_ONLY:-false}"
QUALITY_RUN_ID="${ASTRAVECTOR_QUALITY_RUN_ID:-fix474e-$(date +%Y%m%d-%H%M%S)}"
STARTED_AT="$(date -Iseconds)"
START_EPOCH="$(date +%s)"

mkdir -p "$REPORT_DIR"
rm -f \
  "$REPORT_DIR/runtime-quality-report.dense.json" \
  "$REPORT_DIR/runtime-quality-report.sparse.json" \
  "$REPORT_DIR/runtime-quality-report.hybrid.json" \
  "$REPORT_DIR/runtime-quality-report.graph.json" \
  "$REPORT_DIR/runtime-quality-report.full-capability.json"

export ASTRAVECTOR_QUALITY_ENDPOINT="$ENDPOINT"
export ASTRAVECTOR_QUALITY_RUN_ID="$QUALITY_RUN_ID"
export ASTRAVECTOR_QUALITY_DEBUG_CANDIDATES="${ASTRAVECTOR_QUALITY_DEBUG_CANDIDATES:-true}"
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION="${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION:-true}"
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH="${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH:-false}"

reasons=()
warnings=()
PREFLIGHT_ENDPOINT_AVAILABLE=false
PREFLIGHT_POSTGRES_AVAILABLE=false
PREFLIGHT_QDRANT_AVAILABLE=false
PREFLIGHT_QDRANT_COLLECTION_AVAILABLE=false
PREFLIGHT_QDRANT_VECTOR_SCHEMA_AVAILABLE=false
PREFLIGHT_MODEL_FILE_FOUND=false
PREFLIGHT_TOKENIZER_FILE_FOUND=false
PREFLIGHT_MODEL_INFERENCE_VERIFIED=false
PREFLIGHT_MODEL_INFERENCE_REASON="MODEL_INFERENCE_NOT_VERIFIED_PRE_RUNTIME"
FIXTURES_CHECKSUM=""
FIXTURES_CHECKSUM_STATUS="NOT_COMPUTED"

bool_true() {
  case "$1" in
    1|true|TRUE|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

json_get() {
  local file="$1"
  local expr="$2"
  local fallback="${3-UNKNOWN}"
  if [[ -f "$file" ]]; then
    jq -r "$expr // \"$fallback\"" "$file" 2>/dev/null || printf '%s\n' "$fallback"
  else
    printf '%s\n' "$fallback"
  fi
}

json_num() {
  local file="$1"
  local expr="$2"
  if [[ -f "$file" ]]; then
    jq -r "$expr // 0" "$file" 2>/dev/null || printf '0\n'
  else
    printf '0\n'
  fi
}

append_reason() {
  reasons+=("$1")
}

append_warning() {
  warnings+=("$1")
}

redact_url() {
  printf '%s\n' "$1" | sed -E 's#(postgres://[^:/@]+):([^@]+)@#\1:***@#'
}

compute_fixtures_checksum() {
  local checksum_roots=(
    "$ROOT_DIR/benchmarks/quality/baseline"
    "$ROOT_DIR/benchmarks/quality/corpora"
    "$ROOT_DIR/benchmarks/quality/profiles"
    "$ROOT_DIR/benchmarks/quality/queries"
    "$ROOT_DIR/benchmarks/quality/schemas"
  )
  local existing=()
  local root
  for root in "${checksum_roots[@]}"; do
    if [[ -d "$root" ]]; then
      existing+=("$root")
    fi
  done
  if [[ "${#existing[@]}" -eq 0 ]]; then
    FIXTURES_CHECKSUM_STATUS="MISSING"
    return
  fi
  FIXTURES_CHECKSUM="$(
    find "${existing[@]}" -type f \
      ! -path '*/reports/*' \
      -print0 \
      | sort -z \
      | xargs -0 shasum -a 256 \
      | shasum -a 256 \
      | awk '{print $1}'
  )"
  if [[ -n "$FIXTURES_CHECKSUM" ]]; then
    FIXTURES_CHECKSUM_STATUS="COMPUTED"
  else
    FIXTURES_CHECKSUM_STATUS="FAILED"
  fi
}

timed_out() {
  local now
  now="$(date +%s)"
  [[ $((now - START_EPOCH)) -gt "$TIMEOUT_SECONDS" ]]
}

snapshot_report() {
  local name="$1"
  if [[ -f "$RUNTIME_REPORT" ]]; then
    cp "$RUNTIME_REPORT" "$REPORT_DIR/runtime-quality-report.${name}.json"
  else
    append_reason "${name^^}_REPORT_MISSING"
  fi
}

run_profile() {
  local name="$1"
  local target="$2"
  if timed_out; then
    append_reason "CONFIDENCE_GATE_TIMEOUT"
    return 124
  fi
  (cd "$ROOT_DIR" && make "$target")
  local status=$?
  snapshot_report "$name"
  return "$status"
}

profile_skipped() {
  local file="$1"
  local runtime_execution verdict
  runtime_execution="$(json_get "$file" '.runtime_execution' '')"
  verdict="$(json_get "$file" '.verdict' '')"
  [[ "$runtime_execution" == "SKIPPED_ENDPOINT_NOT_SET" \
    || "$runtime_execution" == "SKIPPED_RUNTIME_REQUIRED" \
    || "$runtime_execution" == "MODEL_BACKED_E2E_SKIPPED" \
    || "$verdict" == "SKIPPED" ]]
}

write_reports() {
  local runtime_execution="$1"
  local verdict="$2"
  local production_pass="$3"
  local not_production_pass="$4"
  local finished_at
  finished_at="$(date -Iseconds)"

  local dense_file="$REPORT_DIR/runtime-quality-report.dense.json"
  local sparse_file="$REPORT_DIR/runtime-quality-report.sparse.json"
  local hybrid_file="$REPORT_DIR/runtime-quality-report.hybrid.json"
  local graph_file="$REPORT_DIR/runtime-quality-report.graph.json"
  local full_file="$REPORT_DIR/runtime-quality-report.full-capability.json"
  for snapshot in "$dense_file" "$sparse_file" "$hybrid_file" "$graph_file" "$full_file"; do
    if [[ ! -f "$snapshot" ]]; then
      printf '{}\n' > "$snapshot"
    fi
  done

  local baseline_forbidden_document baseline_forbidden_phrase baseline_forbidden_total baseline_failed
  baseline_forbidden_document="$(json_num "$BASELINE_FILE" '.forbidden_document_returned')"
  baseline_forbidden_phrase="$(json_num "$BASELINE_FILE" '.forbidden_phrase_returned')"
  baseline_forbidden_total="$(json_num "$BASELINE_FILE" '.forbidden_total')"
  baseline_failed="$(json_num "$BASELINE_FILE" '.hard_negative_failed')"

  local after_forbidden_document after_forbidden_phrase after_forbidden_total after_failed
  after_forbidden_document="$(json_num "$hybrid_file" '.by_reason.FORBIDDEN_DOCUMENT_RETURNED')"
  after_forbidden_phrase="$(json_num "$hybrid_file" '.by_reason.FORBIDDEN_PHRASE_RETURNED')"
  after_forbidden_total=$((after_forbidden_document + after_forbidden_phrase))
  after_failed="$(json_num "$hybrid_file" '.by_category.hard_negative.failed')"

  local reduction_rate
  reduction_rate="$(jq -n \
    --argjson before "$baseline_forbidden_total" \
    --argjson after "$after_forbidden_total" \
    'if $before == 0 then 0 else (($before - $after) / $before) end')"

  local target_met
  target_met="$(jq -n --argjson rate "$reduction_rate" '$rate >= 0.5')"

  local pre_mmr post_mmr no_answer_enabled
  pre_mmr="$(json_num "$hybrid_file" '.no_answer.pre_mmr_filtered_candidate_count')"
  post_mmr="$(json_num "$hybrid_file" '.no_answer.post_mmr_no_answer_triggered_count')"
  no_answer_enabled="$(json_get "$hybrid_file" '.no_answer.enabled' 'true')"

  local hybrid_executed=false
  if [[ "$(json_get "$hybrid_file" '.runtime_execution' '')" != "" ]]; then
    hybrid_executed=true
  fi
  if [[ "$hybrid_executed" == "true" && "$after_failed" != "0" && "$pre_mmr" == "0" && "$post_mmr" == "0" ]]; then
    append_warning "NO_ANSWER_NOT_TRIGGERED_FOR_HARD_NEGATIVE"
  fi
  if [[ "$hybrid_executed" == "true" && "$after_failed" == "0" && "$pre_mmr" == "0" && "$post_mmr" == "0" ]]; then
    append_warning "HARD_NEGATIVE_FILTERING_MECHANISM_UNKNOWN"
  fi

  local reasons_json warnings_json
  if [[ "${#reasons[@]}" -eq 0 ]]; then
    reasons_json="[]"
  else
    reasons_json="$(printf '%s\n' "${reasons[@]}" | jq -Rsc 'split("\n") | map(select(length > 0))')"
  fi
  if [[ "${#warnings[@]}" -eq 0 ]]; then
    warnings_json="[]"
  else
    warnings_json="$(printf '%s\n' "${warnings[@]}" | jq -Rsc 'split("\n") | map(select(length > 0))')"
  fi

  jq -n \
    --arg runtime_execution "$runtime_execution" \
    --arg verdict "$verdict" \
    --argjson production_pass "$production_pass" \
    --argjson not_production_pass "$not_production_pass" \
    --arg quality_run_id "$QUALITY_RUN_ID" \
    --arg started_at "$STARTED_AT" \
    --arg finished_at "$finished_at" \
    --argjson timeout_triggered "$(if timed_out; then echo true; else echo false; fi)" \
    --arg data_isolation_mode "quality_run_id_namespace" \
    --arg astra_version "$(json_get "$ROOT_DIR/Cargo.toml" '.package.version' 'UNKNOWN')" \
    --arg git_commit "$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf 'UNKNOWN')" \
    --arg model_version "$(json_get "$hybrid_file" '.versions.model_version' "$(json_get "$hybrid_file" '.model_version' 'UNKNOWN')")" \
    --arg sparse_mode "$(json_get "$hybrid_file" '.sparse.sparse_mode' 'UNKNOWN')" \
    --arg sparse_encoder_version "$(json_get "$hybrid_file" '.sparse.encoder_version' 'UNKNOWN')" \
    --arg fusion_strategy "$(json_get "$hybrid_file" '.hybrid.fusion_strategy' 'UNKNOWN')" \
    --arg baseline_file "benchmarks/quality/baseline/hard-negative-baseline.json" \
    --arg baseline_fix "$(json_get "$BASELINE_FILE" '.fix_version' 'UNKNOWN')" \
    --arg fixtures_checksum "$FIXTURES_CHECKSUM" \
    --arg fixtures_checksum_status "$FIXTURES_CHECKSUM_STATUS" \
    --argjson baseline_failed "$baseline_failed" \
    --argjson baseline_forbidden_document "$baseline_forbidden_document" \
    --argjson baseline_forbidden_phrase "$baseline_forbidden_phrase" \
    --argjson baseline_forbidden_total "$baseline_forbidden_total" \
    --argjson after_failed "$after_failed" \
    --argjson after_forbidden_document "$after_forbidden_document" \
    --argjson after_forbidden_phrase "$after_forbidden_phrase" \
    --argjson after_forbidden_total "$after_forbidden_total" \
    --argjson reduction_rate "$reduction_rate" \
    --argjson target_met "$target_met" \
    --argjson max_allowed_forbidden_total 0 \
    --argjson pre_mmr "$pre_mmr" \
    --argjson post_mmr "$post_mmr" \
    --arg no_answer_enabled "$no_answer_enabled" \
    --argjson reasons "$reasons_json" \
    --argjson warnings "$warnings_json" \
    --argjson endpoint_available "$PREFLIGHT_ENDPOINT_AVAILABLE" \
    --argjson postgres_available "$PREFLIGHT_POSTGRES_AVAILABLE" \
    --argjson qdrant_available "$PREFLIGHT_QDRANT_AVAILABLE" \
    --argjson qdrant_collection_available "$PREFLIGHT_QDRANT_COLLECTION_AVAILABLE" \
    --argjson qdrant_vector_schema_available "$PREFLIGHT_QDRANT_VECTOR_SCHEMA_AVAILABLE" \
    --argjson model_file_found "$PREFLIGHT_MODEL_FILE_FOUND" \
    --argjson tokenizer_file_found "$PREFLIGHT_TOKENIZER_FILE_FOUND" \
    --argjson model_inference_verified "$PREFLIGHT_MODEL_INFERENCE_VERIFIED" \
    --arg model_inference_reason "$PREFLIGHT_MODEL_INFERENCE_REASON" \
    --slurpfile dense "$dense_file" \
    --slurpfile sparse "$sparse_file" \
    --slurpfile hybrid "$hybrid_file" \
    --slurpfile graph "$graph_file" \
    --slurpfile full "$full_file" \
    '{
      runtime_execution: $runtime_execution,
      verdict: $verdict,
      production_pass: $production_pass,
      runtime_ready: ($verdict == "PASS" and $runtime_execution == "CONFIDENCE_GATE_CONFIRMED"),
      blockers: $reasons,
      preflight: {
        endpoint_available: $endpoint_available,
        postgres_available: $postgres_available,
        qdrant_available: $qdrant_available,
        qdrant_collection_available: $qdrant_collection_available,
        qdrant_vector_schema_available: $qdrant_vector_schema_available,
        model_file_found: $model_file_found,
        tokenizer_file_found: $tokenizer_file_found,
        model_inference_verified: $model_inference_verified,
        model_inference_reason: $model_inference_reason
      },
      confidence_gate: {
        runtime_execution: $runtime_execution,
        verdict: $verdict,
        production_pass: $production_pass,
        not_production_pass: $not_production_pass,
        quality_run_id: $quality_run_id,
        started_at: $started_at,
        finished_at: $finished_at,
        timeout_triggered: $timeout_triggered,
        skipped_profiles_count: ([($dense[0]?, $sparse[0]?, $hybrid[0]?) | select(.runtime_execution == "SKIPPED_ENDPOINT_NOT_SET" or .runtime_execution == "SKIPPED_RUNTIME_REQUIRED" or .verdict == "SKIPPED")] | length),
        data_isolation_mode: $data_isolation_mode,
        cleanup_executed: false,
        reasons: $reasons,
        warnings: $warnings
      },
      versions: {
        astravector_version: $astra_version,
        git_commit: $git_commit,
        model_version: $model_version,
        sparse_mode: $sparse_mode,
        sparse_encoder_version: $sparse_encoder_version,
        fusion_strategy: $fusion_strategy
      },
      baseline: {
        source_file: $baseline_file,
        fix_version: $baseline_fix,
        forbidden_total: $baseline_forbidden_total,
        fixtures_profile: "hybrid-quick",
        fixtures_checksum: (if $fixtures_checksum == "" then null else $fixtures_checksum end),
        fixtures_checksum_status: $fixtures_checksum_status
      },
      profiles: {
        dense: {
          verdict: ($dense[0].verdict // "MISSING"),
          runtime_execution: ($dense[0].runtime_execution // "MISSING"),
          blocked: (($dense[0].retrieve_context_queries_blocked // 0) > 0)
        },
        sparse: {
          verdict: ($sparse[0].verdict // "MISSING"),
          runtime_execution: ($sparse[0].runtime_execution // "MISSING"),
          blocked: (($sparse[0].retrieval.queries_blocked // 0) > 0),
          sparse_available: ($sparse[0].capabilities.sparse_available // false)
        },
        hybrid: {
          verdict: ($hybrid[0].verdict // "MISSING"),
          runtime_execution: ($hybrid[0].runtime_execution // "MISSING"),
          blocked: (($hybrid[0].retrieval.queries_blocked // 0) > 0),
          hybrid_available: ($hybrid[0].capabilities.hybrid_available // false)
        }
      },
      hard_negative: {
        baseline_failed: $baseline_failed,
        baseline_forbidden_document_returned: $baseline_forbidden_document,
        baseline_forbidden_phrase_returned: $baseline_forbidden_phrase,
        baseline_forbidden_total: $baseline_forbidden_total,
        after_failed: $after_failed,
        after_forbidden_document_returned: $after_forbidden_document,
        after_forbidden_phrase_returned: $after_forbidden_phrase,
        after_forbidden_total: $after_forbidden_total,
        false_positive_reduction_rate: $reduction_rate,
        target_reduction_rate: 0.5,
        target_met: $target_met,
        max_allowed_forbidden_total: $max_allowed_forbidden_total,
        no_answer_triggered_count: $post_mmr,
        pre_mmr_filtered_candidate_count: $pre_mmr,
        post_mmr_no_answer_triggered_count: $post_mmr,
        hard_negative_filtered_by: (if $post_mmr > 0 then "no_answer" elif $pre_mmr > 0 then "candidate_filter" elif $after_failed == 0 then "unknown" else "not_filtered" end)
      },
      no_answer: ($hybrid[0].no_answer // {
        enabled: ($no_answer_enabled == "true"),
        min_dense_score: 0.25,
        min_sparse_score: 0.10,
        min_hybrid_score: 0.30,
        exact_technical_boost: 0.5
      }),
      security: {
        cross_zone_leakage_count: ($hybrid[0].retrieval.cross_zone_leakage_count // 0),
        access_level_violation_count: ($hybrid[0].retrieval.access_level_violation_count // 0)
      },
      graph: {
        available: (($graph[0].runtime_execution // "") == "MODEL_BACKED_E2E_CONFIRMED" and ($graph[0].verdict // "") == "PASS"),
        quick_verdict: ($graph[0].verdict // "MISSING"),
        runtime_execution: ($graph[0].runtime_execution // "MISSING"),
        required_for_runtime_ready: false,
        required_for_production_candidate: true,
        graph_expansion_used_count: ($graph[0].graph.graph_expansion_used_count // 0),
        graph_expanded_contexts_count: ($graph[0].graph.graph_expanded_contexts_count // 0),
        graph_expected_related_hit_rate: ($graph[0].graph.graph_expected_related_hit_rate // 0),
        graph_fp_rate: ($graph[0].graph.graph_fp_rate // null),
        access_violation_count: ($graph[0].graph.graph_access_violation_count // 0)
      },
      optional_profiles: {
        graph: ($graph[0]? // null),
        full_capability: ($full[0]? // null)
      }
    }' > "$CONFIDENCE_JSON"

  jq '
    {
      final_status: (if .runtime_ready then "RUNTIME_READY" else "NOT_READY_WITH_BLOCKERS" end),
      runtime_ready: .runtime_ready,
      runtime_confidence_pass: (.runtime_execution == "CONFIDENCE_GATE_CONFIRMED" and .verdict == "PASS"),
      production_pass: .production_pass,
      production_pass_meaning: "confidence gate passed, not PRODUCTION_CANDIDATE",
      production_candidate: false,
      production_ready: false,
      generated_at: .confidence_gate.finished_at,
      quality_run_id: .confidence_gate.quality_run_id,
      runtime_execution: .runtime_execution,
      verdict: .verdict,
      profiles: {
        dense: (.profiles.dense.verdict // "MISSING"),
        sparse: (.profiles.sparse.verdict // "MISSING"),
        hybrid: (.profiles.hybrid.verdict // "MISSING")
      },
      profile_runtime_execution: {
        dense: (.profiles.dense.runtime_execution // "MISSING"),
        sparse: (.profiles.sparse.runtime_execution // "MISSING"),
        hybrid: (.profiles.hybrid.runtime_execution // "MISSING")
      },
      graph: {
        available: (.graph.available // false),
        quick_verdict: (.graph.quick_verdict // "MISSING"),
        runtime_execution: (.graph.runtime_execution // "MISSING"),
        graph_expansion_used_count: (.graph.graph_expansion_used_count // 0),
        graph_expanded_contexts_count: (.graph.graph_expanded_contexts_count // 0),
        graph_expected_related_hit_rate: (.graph.graph_expected_related_hit_rate // 0),
        graph_fp_rate: (.graph.graph_fp_rate // null),
        access_violation_count: (.graph.access_violation_count // 0),
        required_for_runtime_ready: false,
        required_for_production_candidate: true,
        policy: (if (.graph.available // false)
          then "GraphRAG quick profile passed; full-capability and production-candidate gates remain required for PRODUCTION_CANDIDATE."
          else "GraphRAG remains a blocker for PRODUCTION_CANDIDATE, not for RUNTIME_READY."
          end)
      },
      security: {
        cross_zone_leakage_count: (.security.cross_zone_leakage_count // 0),
        access_level_violation_count: (.security.access_level_violation_count // 0)
      },
      hard_negative: {
        forbidden_total_after: (.hard_negative.after_forbidden_total // 0),
        false_positive_rate: (if (.hard_negative.after_forbidden_total // 0) == 0 then 0.0 else 1.0 end),
        pre_mmr_filtered_candidate_count: (.hard_negative.pre_mmr_filtered_candidate_count // 0),
        post_mmr_no_answer_triggered_count: (.hard_negative.post_mmr_no_answer_triggered_count // 0)
      },
      fixtures: {
        checksum_status: (.baseline.fixtures_checksum_status // "UNKNOWN"),
        checksum: (.baseline.fixtures_checksum // null)
      },
      blockers: .blockers,
      warnings: .confidence_gate.warnings,
      remaining_to_production_candidate: [
        "GraphRAG production-candidate proof",
        "full all-target test suite proof",
        "deployment and operational readiness gates",
        "load/soak/recovery/backup/restore/rollback proof"
      ]
    }
  ' "$CONFIDENCE_JSON" > "$FINAL_READINESS_JSON"

  {
    printf '# Runtime Confidence Report\n\n'
    printf '%s\n' "- started_at: \`$STARTED_AT\`"
    printf '%s\n' "- finished_at: \`$finished_at\`"
    printf '%s\n' "- quality_run_id: \`$QUALITY_RUN_ID\`"
    printf '%s\n' "- verdict: \`$verdict\`"
    printf '%s\n' "- runtime_execution: \`$runtime_execution\`"
    printf '%s\n' "- production_pass: \`$production_pass\`"
    printf '%s\n' "- astraVector version: \`$(jq -r '.versions.astravector_version' "$CONFIDENCE_JSON")\`"
    printf '%s\n' "- git commit: \`$(jq -r '.versions.git_commit' "$CONFIDENCE_JSON")\`"
    printf '%s\n\n' "- model version: \`$(jq -r '.versions.model_version' "$CONFIDENCE_JSON")\`"
    if [[ "$verdict" != "PASS" ]]; then
      printf '**This is not a production PASS.**\n\n'
    fi
    printf '## Mandatory Profiles\n\n| Profile | Verdict | Runtime Execution | Available/Blocked |\n|---|---:|---|---|\n'
    jq -r '.profiles | to_entries[] | "| \(.key) | \(.value.verdict) | \(.value.runtime_execution) | sparse=\(.value.sparse_available // "n/a"), hybrid=\(.value.hybrid_available // "n/a"), blocked=\(.value.blocked // false) |"' "$CONFIDENCE_JSON"
    printf '\n## Hard-Negative Before/After\n\n| Metric | Before | After |\n|---|---:|---:|\n'
    jq -r '.hard_negative | "| forbidden_document_returned | \(.baseline_forbidden_document_returned) | \(.after_forbidden_document_returned) |\n| forbidden_phrase_returned | \(.baseline_forbidden_phrase_returned) | \(.after_forbidden_phrase_returned) |\n| forbidden_total | \(.baseline_forbidden_total) | \(.after_forbidden_total) |\n| failed | \(.baseline_failed) | \(.after_failed) |"' "$CONFIDENCE_JSON"
    printf '\n## No-Answer Thresholds\n\n'
    jq -r '.no_answer | "- enabled: `\(.enabled)`\n- min_dense_score: `\(.min_dense_score)`\n- min_sparse_score: `\(.min_sparse_score)`\n- min_hybrid_score: `\(.min_hybrid_score)`\n- exact_technical_boost: `\(.exact_technical_boost)`"' "$CONFIDENCE_JSON"
    printf '\n## Security Gates\n\n'
    jq -r '.security | "- cross_zone_leakage_count: `\(.cross_zone_leakage_count)`\n- access_level_violation_count: `\(.access_level_violation_count)`"' "$CONFIDENCE_JSON"
    printf '\n## Reasons\n\n'
    jq -r '.confidence_gate.reasons | if length == 0 then "- none" else .[] | "- \(.)" end' "$CONFIDENCE_JSON"
    printf '\n## Preflight\n\n'
    jq -r '.preflight | to_entries[] | "- \(.key): `\(.value)`"' "$CONFIDENCE_JSON"
    printf '\n## Warnings\n\n'
    jq -r '.confidence_gate.warnings | if length == 0 then "- none" else .[] | "- \(.)" end' "$CONFIDENCE_JSON"
    if [[ "$verdict" != "PASS" ]]; then
      printf '\n## Recommendations\n\n- Fix the listed reasons, rerun `make quality-runtime-confidence-remote`, and do not treat diagnostic-only output as production evidence.\n'
    fi
  } > "$CONFIDENCE_MD"
}

preflight() {
  local db_url="${ASTRAVECTOR_DB_URL:-${DATABASE_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector}}"
  local qdrant_url="${ASTRAVECTOR_QDRANT_URL:-http://127.0.0.1:6333}"
  local qdrant_collection="${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_v004}"

  printf 'confidence preflight:\n' >&2
  printf '  endpoint_url: %s\n' "$ENDPOINT" >&2
  printf '  grpc_host: %s\n' "$GRPC_HOST" >&2
  printf '  postgres_url: %s\n' "$(redact_url "$db_url")" >&2
  printf '  qdrant_url: %s\n' "$qdrant_url" >&2
  printf '  qdrant_collection: %s\n' "$qdrant_collection" >&2
  printf '  model_path: %s\n' "${ASTRAVECTOR_MODEL_PATH:-}" >&2
  printf '  tokenizer_path: %s\n' "${ASTRAVECTOR_TOKENIZER_PATH:-}" >&2

  if grpcurl -plaintext "$GRPC_HOST" list >/dev/null 2>&1; then
    PREFLIGHT_ENDPOINT_AVAILABLE=true
    printf '  endpoint: OK\n' >&2
  else
    printf '  endpoint: FAIL\n' >&2
    append_reason "ENDPOINT_UNAVAILABLE"
  fi
  if psql "$db_url" -c 'select 1' >/dev/null 2>&1; then
    PREFLIGHT_POSTGRES_AVAILABLE=true
    printf '  postgres: OK\n' >&2
  else
    printf '  postgres: FAIL\n' >&2
    append_reason "POSTGRES_UNAVAILABLE"
  fi
  if curl -fsS "$qdrant_url/collections" >/dev/null 2>&1; then
    PREFLIGHT_QDRANT_AVAILABLE=true
    printf '  qdrant: OK\n' >&2
  else
    printf '  qdrant: FAIL\n' >&2
    append_reason "QDRANT_UNAVAILABLE"
  fi
  if [[ ! -f "$BASELINE_FILE" ]]; then
    append_reason "BASELINE_FILE_MISSING"
  elif ! jq -e '.fix_version and .forbidden_total and .forbidden_document_returned and .forbidden_phrase_returned' "$BASELINE_FILE" >/dev/null 2>&1; then
    append_reason "BASELINE_FILE_INVALID"
  fi
  if [[ -z "$QUALITY_RUN_ID" ]]; then
    append_reason "QUALITY_RUN_ID_MISSING"
  fi
  if [[ "${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION}" != "true" ]]; then
    append_reason "AUTO_CREATE_ON_INGESTION_NOT_ENABLED"
  fi
  if [[ "${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH}" != "false" ]]; then
    append_reason "AUTO_CREATE_ON_SEARCH_MUST_BE_FALSE"
  fi
  if [[ -n "${ASTRAVECTOR_MODEL_PATH:-}" && -f "${ASTRAVECTOR_MODEL_PATH}" ]]; then
    PREFLIGHT_MODEL_FILE_FOUND=true
  elif [[ -n "${ASTRAVECTOR_MODEL_DIR:-}" ]] && find "${ASTRAVECTOR_MODEL_DIR}" -maxdepth 5 -name '*.onnx' -type f 2>/dev/null | grep -q .; then
    PREFLIGHT_MODEL_FILE_FOUND=true
  fi
  if [[ "$PREFLIGHT_MODEL_FILE_FOUND" == "true" ]]; then
    printf '  model_file: OK\n' >&2
  else
    printf '  model_file: FAIL\n' >&2
    append_reason "MODEL_FILES_NOT_FOUND"
  fi
  if [[ -n "${ASTRAVECTOR_TOKENIZER_PATH:-}" && -e "${ASTRAVECTOR_TOKENIZER_PATH}" ]]; then
    PREFLIGHT_TOKENIZER_FILE_FOUND=true
  elif [[ -n "${ASTRAVECTOR_MODEL_DIR:-}" ]] && find "${ASTRAVECTOR_MODEL_DIR}" -maxdepth 5 \( -name 'tokenizer.json' -o -name 'tokenizer.model' \) -type f 2>/dev/null | grep -q .; then
    PREFLIGHT_TOKENIZER_FILE_FOUND=true
  fi
  if [[ "$PREFLIGHT_TOKENIZER_FILE_FOUND" == "true" ]]; then
    printf '  tokenizer_file: OK\n' >&2
  else
    printf '  tokenizer_file: FAIL\n' >&2
    append_reason "TOKENIZER_FILES_NOT_FOUND"
  fi

  local qdrant_collection_json
  qdrant_collection_json="$(curl -fsS "$qdrant_url/collections/$qdrant_collection" 2>/dev/null || true)"
  if [[ -n "$qdrant_collection_json" ]]; then
    PREFLIGHT_QDRANT_COLLECTION_AVAILABLE=true
    printf '  qdrant_collection: OK\n' >&2
    if jq -e '
      .result.status == "green"
      and (
        (.result.config.params.vectors.size? // .result.config.params.vectors.default.size? // 0) > 0
        or ((.result.config.params.vectors // {}) | type == "object" and length > 0)
      )
    ' >/dev/null 2>&1 <<<"$qdrant_collection_json"; then
      PREFLIGHT_QDRANT_VECTOR_SCHEMA_AVAILABLE=true
      printf '  qdrant_vector_schema: OK\n' >&2
    else
      printf '  qdrant_vector_schema: FAIL\n' >&2
      append_reason "QDRANT_COLLECTION_MISMATCH"
    fi
  else
    printf '  qdrant_collection: FAIL\n' >&2
    append_reason "QDRANT_COLLECTION_MISSING"
  fi
  printf '  model_inference: %s\n' "$PREFLIGHT_MODEL_INFERENCE_REASON" >&2
  compute_fixtures_checksum
  printf '  fixtures_checksum: %s\n' "$FIXTURES_CHECKSUM_STATUS" >&2
  if [[ "${ASTRAVECTOR_RETRIEVAL_NO_ANSWER_ENABLED:-true}" != "true" ]]; then
    append_reason "NO_ANSWER_DISABLED"
  fi
}

main() {
  preflight
  if [[ "${#reasons[@]}" -gt 0 ]] && ! bool_true "$DIAGNOSTIC_ONLY"; then
    write_reports "CONFIDENCE_GATE_FAILED" "FAIL" false true
    return 1
  fi

  run_profile "dense" "quality-runtime-dense-quick-remote" || append_reason "DENSE_PROFILE_FAILED"
  local dense_file="$REPORT_DIR/runtime-quality-report.dense.json"
  if [[ "$(json_get "$dense_file" '.runtime_execution' '')" == "MODEL_BACKED_E2E_CONFIRMED" ]]; then
    PREFLIGHT_MODEL_INFERENCE_VERIFIED=true
    PREFLIGHT_MODEL_INFERENCE_REASON="MODEL_INFERENCE_VERIFIED_BY_DENSE_RUNTIME_PROFILE"
  else
    append_warning "$PREFLIGHT_MODEL_INFERENCE_REASON"
  fi
  run_profile "sparse" "quality-runtime-sparse-quick-remote" || append_reason "SPARSE_PROFILE_FAILED"
  run_profile "hybrid" "quality-runtime-hybrid-quick-remote" || append_reason "HYBRID_PROFILE_FAILED"

  if bool_true "${ASTRAVECTOR_QUALITY_CONFIDENCE_RUN_OPTIONAL:-false}"; then
    run_profile "graph" "quality-runtime-graph-quick-remote" || append_warning "OPTIONAL_GRAPH_PROFILE_FAILED"
    run_profile "full-capability" "quality-runtime-full-capability-quick-remote" || append_warning "OPTIONAL_FULL_CAPABILITY_PROFILE_FAILED"
  fi

  local dense_file="$REPORT_DIR/runtime-quality-report.dense.json"
  local sparse_file="$REPORT_DIR/runtime-quality-report.sparse.json"
  local hybrid_file="$REPORT_DIR/runtime-quality-report.hybrid.json"

  for name in dense sparse hybrid; do
    local file="$REPORT_DIR/runtime-quality-report.${name}.json"
    if [[ ! -f "$file" ]]; then
      append_reason "${name^^}_REPORT_MISSING"
    elif profile_skipped "$file"; then
      append_reason "${name^^}_PROFILE_SKIPPED"
    fi
  done

  [[ "$(json_get "$dense_file" '.verdict' '')" == "PASS" ]] || append_reason "DENSE_PROFILE_NOT_PASS"
  [[ "$(json_get "$sparse_file" '.capabilities.sparse_available' 'false')" == "true" ]] || append_reason "SPARSE_UNAVAILABLE"
  [[ "$(json_get "$hybrid_file" '.capabilities.hybrid_available' 'false')" == "true" ]] || append_reason "HYBRID_UNAVAILABLE"
  [[ "$(json_get "$hybrid_file" '.no_answer.enabled' 'false')" == "true" ]] || append_reason "NO_ANSWER_DISABLED"
  [[ "$(json_get "$hybrid_file" '.sparse.sparse_mode' '')" == "LEXICAL_BASELINE_TECHNICAL" ]] || append_reason "SPARSE_MODE_MISMATCH"

  local sparse_blocked hybrid_blocked forbidden_after reduction_ok cross_zone access_violation
  sparse_blocked="$(json_num "$sparse_file" '.retrieval.queries_blocked')"
  hybrid_blocked="$(json_num "$hybrid_file" '.retrieval.queries_blocked')"
  [[ "$sparse_blocked" == "0" ]] || append_reason "SPARSE_PROFILE_BLOCKED"
  [[ "$hybrid_blocked" == "0" ]] || append_reason "HYBRID_PROFILE_BLOCKED"

  forbidden_after=$(( $(json_num "$hybrid_file" '.by_reason.FORBIDDEN_DOCUMENT_RETURNED') + $(json_num "$hybrid_file" '.by_reason.FORBIDDEN_PHRASE_RETURNED') ))
  [[ "$forbidden_after" == "0" ]] || append_reason "FORBIDDEN_TOTAL_AFTER_NON_ZERO"

  reduction_ok="$(jq -n \
    --argjson before "$(json_num "$BASELINE_FILE" '.forbidden_total')" \
    --argjson after "$forbidden_after" \
    'if $before == 0 then false else (($before - $after) / $before) >= 0.5 end')"
  [[ "$reduction_ok" == "true" ]] || append_reason "HARD_NEGATIVE_TARGET_NOT_MET"

  cross_zone="$(json_num "$hybrid_file" '.retrieval.cross_zone_leakage_count')"
  access_violation="$(json_num "$hybrid_file" '.retrieval.access_level_violation_count')"
  [[ "$cross_zone" == "0" ]] || append_reason "CROSS_ZONE_LEAKAGE_FOUND"
  [[ "$access_violation" == "0" ]] || append_reason "ACCESS_LEVEL_VIOLATION_FOUND"

  if timed_out; then
    append_reason "CONFIDENCE_GATE_TIMEOUT"
  fi

  if bool_true "$DIAGNOSTIC_ONLY"; then
    write_reports "CONFIDENCE_GATE_DIAGNOSTIC_ONLY" "DIAGNOSTIC_ONLY" false true
    return 0
  fi

  if [[ "${#reasons[@]}" -eq 0 ]]; then
    write_reports "CONFIDENCE_GATE_CONFIRMED" "PASS" true false
    return 0
  fi
  write_reports "CONFIDENCE_GATE_FAILED" "FAIL" false true
  return 1
}

main "$@"
