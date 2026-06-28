#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env

REPORT="$REPORTS_DIR/OUTBOX_FENCING_REPORT.md"
EVIDENCE="$REPORTS_DIR/outbox-fencing-evidence.jsonl"
: > "$EVIDENCE"
die() { fail "$1"; exit "$FAIL_STATUS"; }
emit() {
  jq -nc --arg test_id "$1" --arg status "$2" --arg expected "$3" --arg actual "$4" --arg error "${5:-}" \
    '{test_id:$test_id,status:$status,document_id:null,access_zone_id:null,expected:$expected,actual:$actual,sql_evidence:{},qdrant_evidence:{},grpc_evidence:{},error:(if $error=="" then null else $error end)}' >> "$EVIDENCE"
}

cols="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM information_schema.columns WHERE table_schema='astravector' AND table_name='vector_outbox' AND column_name='lock_generation'")" || die "schema check failed"
[[ "$cols" -eq 1 ]] || die "lock_generation is missing"
emit "W3C_SCHEMA" "PASS" "lock_generation exists" "present"

event_id="$(psql "$(postgres_url)" -Atqc "SELECT id FROM astravector.vector_outbox ORDER BY updated_at DESC NULLS LAST, created_at DESC LIMIT 1")" || die "event lookup failed"
[[ -n "$event_id" ]] || die "no outbox event available for fencing test"

psql "$(postgres_url)" -v ON_ERROR_STOP=1 -c "UPDATE astravector.vector_outbox SET status='PENDING',locked_by=NULL,locked_until=NULL,lock_generation=0 WHERE id='${event_id}'::uuid" >/dev/null || die "event reset failed"

claim_a="$(psql "$(postgres_url)" -Atqc "UPDATE astravector.vector_outbox SET status='PROCESSING',locked_by='worker-a',locked_until=now()+interval '60 seconds',lock_generation=lock_generation+1,attempt_count=attempt_count+1 WHERE id='${event_id}'::uuid AND status IN('PENDING','RETRY_PENDING','PROCESSING') AND (locked_until IS NULL OR locked_until<now()) RETURNING lock_generation")" || die "claim A failed"
claim_b="$(psql "$(postgres_url)" -Atqc "UPDATE astravector.vector_outbox SET status='PROCESSING',locked_by='worker-b',locked_until=now()+interval '60 seconds',lock_generation=lock_generation+1,attempt_count=attempt_count+1 WHERE id='${event_id}'::uuid AND status IN('PENDING','RETRY_PENDING','PROCESSING') AND (locked_until IS NULL OR locked_until<now()) RETURNING lock_generation")" || die "claim B failed"
if [[ "$claim_a" == "1" && -z "$claim_b" ]]; then
  emit "W3C_DOUBLE_CLAIM" "PASS" "one claim succeeds" "claim_a=$claim_a claim_b=none"
else
  emit "W3C_DOUBLE_CLAIM" "FAIL" "one claim succeeds" "claim_a=${claim_a:-none} claim_b=${claim_b:-none}"
  die "double claim invariant failed"
fi

psql "$(postgres_url)" -v ON_ERROR_STOP=1 -c "UPDATE astravector.vector_outbox SET locked_until=now()-interval '1 second' WHERE id='${event_id}'::uuid" >/dev/null || die "expire lock failed"
claim_reclaim="$(psql "$(postgres_url)" -Atqc "UPDATE astravector.vector_outbox SET status='PROCESSING',locked_by='worker-b',locked_until=now()+interval '60 seconds',lock_generation=lock_generation+1,attempt_count=attempt_count+1,reclaim_count=reclaim_count+1 WHERE id='${event_id}'::uuid AND status='PROCESSING' AND locked_until<now() RETURNING lock_generation")" || die "reclaim failed"
stale_complete="$(psql "$(postgres_url)" -Atqc "WITH u AS (UPDATE astravector.vector_outbox SET status='COMPLETED' WHERE id='${event_id}'::uuid AND status='PROCESSING' AND locked_by='worker-a' AND lock_generation=1 RETURNING 1) SELECT count(*) FROM u")" || die "stale complete failed"
current_complete="$(psql "$(postgres_url)" -Atqc "WITH u AS (UPDATE astravector.vector_outbox SET status='COMPLETED',completed_at=now(),locked_by=NULL,locked_until=NULL WHERE id='${event_id}'::uuid AND status='PROCESSING' AND locked_by='worker-b' AND lock_generation=${claim_reclaim} RETURNING 1) SELECT count(*) FROM u")" || die "current complete failed"
final_generation="$(psql "$(postgres_url)" -Atqc "SELECT lock_generation FROM astravector.vector_outbox WHERE id='${event_id}'::uuid")"
if [[ "$claim_reclaim" == "2" && "$stale_complete" == "0" && "$current_complete" == "1" && "$final_generation" == "2" ]]; then
  emit "W3C_STALE_COMPLETION" "PASS" "stale generation rejected/current accepted" "reclaim=$claim_reclaim stale_rows=$stale_complete current_rows=$current_complete final_generation=$final_generation"
else
  emit "W3C_STALE_COMPLETION" "FAIL" "stale generation rejected/current accepted" "reclaim=$claim_reclaim stale_rows=$stale_complete current_rows=$current_complete final_generation=$final_generation"
  die "stale completion invariant failed"
fi

{
  echo "# AstraVector_v004 Outbox Fencing Report"
  echo
  echo "## Verdict"
  echo "OUTBOX_FENCING_PASS"
  echo
  echo "## Evidence"
  echo '```json'
  jq -s . "$EVIDENCE"
  echo '```'
} > "$REPORT"
exit "$PASS"
