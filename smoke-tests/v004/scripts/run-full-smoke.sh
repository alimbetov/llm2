#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/processes.sh"
load_smoke_env

KEEP_RUNNING=false
NO_CLEANUP=false
SKIP_BUILD=false
SKIP_CORPUS=false
ONLY=""
FROM=""
STRICT=false
PROFILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-running) KEEP_RUNNING=true ;;
    --no-cleanup) NO_CLEANUP=true ;;
    --skip-build) SKIP_BUILD=true ;;
    --skip-corpus) SKIP_CORPUS=true ;;
    --only) ONLY="${2:-}"; shift ;;
    --profile) PROFILE="${2:-}"; shift ;;
    --from) FROM="${2:-}"; shift ;;
    --strict) STRICT=true ;;
    *) log_error "unknown argument: $1"; exit 1 ;;
  esac
  shift
done

steps=(
  "preflight:00-preflight.sh"
  "build:01-build.sh"
  "infra:02-start-infra.sh"
  "migrations:03-apply-migrations.sh"
  "services:04-start-services.sh"
  "health:05-health-smoke.sh"
  "encode:06-encode-smoke.sh"
  "document-version:07-document-version-smoke.sh"
  "chunking:08-chunking-smoke.sh"
  "persistence:09-persistence-smoke.sh"
  "outbox:10-outbox-smoke.sh"
  "retrieval:11-retrieval-smoke.sh"
  "access-isolation:12-access-isolation-smoke.sh"
  "ttl:13-ttl-smoke.sh"
  "legal-hold:14-legal-hold-smoke.sh"
  "delete:15-delete-smoke.sh"
  "enrichment:16-enrichment-smoke.sh"
  "relevance:17-relevance-smoke.sh"
  "reconciliation:18-reconciliation-smoke.sh"
  "recovery:19-recovery-smoke.sh"
  "corpus:21-corpus-smoke.sh"
  "rag-retrieval:39-rag-retrieval-expert-smoke.sh"
  "rag-quality-full-power:41-rag-quality-full-power.sh"
  "bm25-hybrid-retrieval:42-bm25-hybrid-retrieval.sh"
  "data-integrity-audit:45-data-integrity-audit.sh"
  "access-security:50-access-security-full-power.sh"
  "full-power-wave1:40-full-power-wave1.sh"
  "full-power-wave2:51-full-power-wave2.sh"
  "consistency:60-consistency-full-power.sh"
  "atomicity-failpoints:62-required-atomicity-failpoints.sh"
  "outbox-fencing:63-outbox-fencing.sh"
  "full-power-wave3:64-full-power-wave3.sh"
  "dead-letter-qdrant-failure:66-dead-letter-qdrant-failure.sh"
  "observability:22-observability-smoke.sh"
  "shutdown:20-shutdown-smoke.sh"
)

if [[ "$PROFILE" == "full-power" ]]; then
  steps=(
    "build:01-build.sh"
    "migrations:03-apply-migrations.sh"
    "services:04-start-services.sh"
    "chunking:08-chunking-smoke.sh"
    "outbox:10-outbox-smoke.sh"
    "document-version:07-document-version-smoke.sh"
    "corpus:21-corpus-smoke.sh"
    "retrieval:11-retrieval-smoke.sh"
    "rag-retrieval:39-rag-retrieval-expert-smoke.sh"
    "data-integrity-audit:45-data-integrity-audit.sh"
    "full-power-wave1:40-full-power-wave1.sh"
  )
elif [[ "$PROFILE" == "secure-rag-core" ]]; then
  steps=(
    "full-power-wave1:40-full-power-wave1.sh"
    "access-security:50-access-security-full-power.sh"
  )
elif [[ "$PROFILE" == "consistency-core" ]]; then
  steps=(
    "build:01-build.sh"
    "migrations:03-apply-migrations.sh"
    "full-power-wave1:40-full-power-wave1.sh"
    "access-security:50-access-security-full-power.sh"
    "consistency:60-consistency-full-power.sh"
    "data-integrity-audit:45-data-integrity-audit.sh"
  )
elif [[ "$PROFILE" == "reliability-closing" ]]; then
  steps=(
    "build:01-build.sh"
    "migrations:03-apply-migrations.sh"
    "full-power-wave1:40-full-power-wave1.sh"
    "access-security:50-access-security-full-power.sh"
    "consistency:60-consistency-full-power.sh"
    "atomicity-failpoints:62-required-atomicity-failpoints.sh"
    "outbox-fencing:63-outbox-fencing.sh"
    "dead-letter-qdrant-failure:66-dead-letter-qdrant-failure.sh"
    "data-integrity-audit:45-data-integrity-audit.sh"
  )
elif [[ -n "$PROFILE" ]]; then
  log_error "unknown profile: $PROFILE"
  exit 1
fi

cleanup() {
  if [[ "$KEEP_RUNNING" == false && "$NO_CLEANUP" == false ]]; then
    "$SMOKE_ROOT/scripts/23-cleanup.sh" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

rm -f "$RESULTS_DIR"/*.json "$REPORTS_DIR/smoke-results.json" "$REPORTS_DIR/smoke-junit.xml"

should_run=false
[[ -z "$FROM" ]] && should_run=true
overall=0
critical_failed=false

for entry in "${steps[@]}"; do
  name="${entry%%:*}"
  script="${entry#*:}"
  [[ "$SKIP_BUILD" == true && "$name" == "build" ]] && continue
  [[ "$SKIP_CORPUS" == true && "$name" == "corpus" ]] && continue
  if [[ -n "$ONLY" && "$ONLY" != "$name" ]]; then
    continue
  fi
  if [[ -n "$FROM" && "$FROM" == "$name" ]]; then
    should_run=true
  fi
  [[ "$should_run" == true ]] || continue

  run_smoke_step "$name" "$SMOKE_ROOT/scripts/$script"
  rc=$?
  if [[ "$rc" -eq 1 ]]; then
    overall=1
    critical_failed=true
    log_error "critical FAIL in $name; stopping"
    break
  fi
  if [[ "$rc" -eq 2 && "$STRICT" == true ]]; then
    overall=1
    critical_failed=true
    log_error "BLOCKED in strict mode at $name; stopping"
    break
  fi
done

generate_reports() {
  local summary="$REPORTS_DIR/smoke-results.json"
  jq -s '{
    generated_at: now | todate,
    results: .,
    counts: {
      PASS: map(select(.status=="PASS")) | length,
      FAIL: map(select(.status=="FAIL")) | length,
      BLOCKED: map(select(.status=="BLOCKED")) | length,
      SKIPPED: map(select(.status=="SKIPPED")) | length
    }
  }' "$RESULTS_DIR"/*.json > "$summary" 2>/dev/null || jq -n '{results:[],counts:{PASS:0,FAIL:0,BLOCKED:0,SKIPPED:0}}' > "$summary"

  {
    echo "# AstraVector v004 Smoke Report"
    echo
    echo "- Generated: $(now_iso)"
    echo "- Project: $PROJECT_DIR"
    echo "- Git commit: $(git -C "$PROJECT_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "- Rust: $(rustc --version 2>/dev/null || echo 'not found')"
    echo "- Cargo: $(cargo --version 2>/dev/null || echo 'not found')"
    echo "- PostgreSQL: $(psql "$(postgres_url)" -Atqc 'select version()' 2>/dev/null | head -n 1 || echo 'not checked')"
    echo "- Qdrant: $(curl -sS "$QDRANT_HTTP_URL" 2>/dev/null | jq -r '.title // "not checked"' 2>/dev/null || echo 'not checked')"
    echo
    echo "| ID | Test | Status | Duration ms | Evidence |"
    echo "|---|---|---:|---:|---|"
    jq -r '.results | to_entries[] | "| \(.key+1) | \(.value.test) | \(.value.status) | \(.value.duration_ms) | smoke-tests/v004/results/\(.value.test).json |"' "$summary"
    echo
    echo "## Counts"
    jq -r '.counts | to_entries[] | "- \(.key): \(.value)"' "$summary"
    echo
    echo "## Production Blockers"
    echo
    echo "- Wave 1 validates indexing, retrieval, corpus ingestion, RAG quality, and integrity only."
    echo "- Wave 2+ remains required for access-security, TTL/legal-hold/delete semantics, reconciliation/rebuild, failpoints, overload, and observability."
    echo "- See FULL_POWER_SMOKE_REPORT.md for the current candidate verdict."
  } > "$REPORTS_DIR/SMOKE_REPORT.md"

  {
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    total="$(jq '.results | length' "$summary")"
    failures="$(jq '.counts.FAIL' "$summary")"
    skipped="$(jq '.counts.BLOCKED + .counts.SKIPPED' "$summary")"
    echo "<testsuite name=\"astravector-v004-smoke\" tests=\"$total\" failures=\"$failures\" skipped=\"$skipped\">"
    jq -r '.results[] | @base64' "$summary" | while read -r row; do
      item="$(printf '%s' "$row" | base64 --decode)"
      name="$(jq -r '.test' <<<"$item")"
      status="$(jq -r '.status' <<<"$item")"
      duration="$(jq -r '.duration_ms / 1000' <<<"$item")"
      echo "  <testcase name=\"$name\" time=\"$duration\">"
      [[ "$status" == "FAIL" ]] && echo "    <failure message=\"FAIL\"/>"
      [[ "$status" == "BLOCKED" || "$status" == "SKIPPED" ]] && echo "    <skipped message=\"$status\"/>"
      echo "  </testcase>"
    done
    echo '</testsuite>'
  } > "$REPORTS_DIR/smoke-junit.xml"
}

generate_reports
cat "$REPORTS_DIR/SMOKE_REPORT.md"
[[ "$critical_failed" == true ]] && exit 1
exit "$overall"
