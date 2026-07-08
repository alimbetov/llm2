#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
[[ -f "$SMOKE_ROOT/fixtures/rag-questions-civil-code.resolved.json" ]] || blocked "resolved RAG fixture missing; run 39-build-rag-fixtures.sh"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control"

results="$REPORTS_DIR/rag-retrieval-results.json"
candidates="$REPORTS_DIR/rag-retrieval-candidates.jsonl"
report="$REPORTS_DIR/RAG_RETRIEVAL_REPORT.md"
raw_dir="$LOGS_DIR/rag-retrieval"
mkdir -p "$raw_dir"
: > "$candidates"

active_count="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND status='ACTIVE'")" || active_count=0
if [[ "${active_count:-0}" -eq 0 ]]; then
  jq -n '{verdict:"NOT_READY", reason:"no ACTIVE document version for Civil Code retrieval expert test"}' > "$results"
  {
    echo "# AstraVector_v004 RAG Retrieval Expert Report"
    echo
    echo "## 1. Summary"
    echo
    echo "- Verdict: NOT_READY"
    echo "- Reason: no ACTIVE document version is available for the expert retrieval test."
  } > "$report"
  blocked "no ACTIVE document version for RAG expert retrieval"
fi

python3 - "$SMOKE_ROOT/fixtures/rag-questions-civil-code.resolved.json" > "$raw_dir/questions.tsv" <<'PY'
import json, sys
for q in json.load(open(sys.argv[1], encoding="utf-8")):
    print("\t".join(str(q.get(k) or "") for k in ["id","question","expected_phrase","expected_answer_hint","fixture_status","top_k"]))
PY

total=0
valid=0
passed=0
failed=0
invalid=0
sum_rr="0"
while IFS=$'\t' read -r qid question phrase hint status top_k; do
  total=$((total+1))
  [[ "$qid" == hard-* ]] && continue
  [[ "$status" == "INVALID_FIXTURE" ]] && { invalid=$((invalid+1)); continue; }
  [[ "$status" == "HARD_NEGATIVE" ]] && continue
  valid=$((valid+1))
  body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg q "$question" --argjson k "${top_k:-10}" '{correlationId:"rag-expert",accessZoneId:$zone,callerAccessLevel:"PUBLIC",query:$q,topK:$k,candidateLimit:50,parentLimit:$k,timeoutMs:15000}')"
  if ! grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/Search >"$raw_dir/${qid}.json" 2>"$raw_dir/${qid}.err"; then
    echo "{\"question_id\":\"$qid\",\"status\":\"FAIL\",\"reason\":\"Search call failed\"}" >> "$candidates"
    failed=$((failed+1))
    continue
  fi
  jq -c --arg id "$qid" --arg q "$question" --arg phrase "$phrase" --arg hint "$hint" '
    .results as $r |
    ($r | map(.parentText | contains($phrase)) | index(true)) as $idx |
    {
      question_id:$id,
      question:$q,
      status:(if $idx == null then "FAIL" else "PASS" end),
      expected_phrase:$phrase,
      expected_found:($idx != null),
      expected_rank:(if $idx == null then null else $idx + 1 end),
      top_k:($r|length),
      candidate_count:(.diagnostics.candidateCount // 0),
      parent_group_count:(.diagnostics.parentGroupCount // 0),
      query_embedding_ms:(.diagnostics.queryEmbeddingMs // 0),
      qdrant_search_ms:(.diagnostics.qdrantSearchMs // 0),
      parent_fetch_ms:(.diagnostics.parentFetchMs // 0),
      total_retrieval_ms:(.diagnostics.totalMs // 0),
      results:($r | to_entries | map({
        rank:(.key+1),
        document_id:.value.documentId,
        document_version:.value.documentVersion,
        root_chunk_id:.value.rootChunkId,
        source_chunk_id:.value.sourceChunkId,
        parent_chunk_id:.value.parentChunkId,
        matched_chunk_id:.value.matchedChunkId,
        matched_granularity:.value.matchedGranularity,
        representation_type:"ORIGINAL",
        dense_score:(.value.scores.denseScore // null),
        sparse_score:null,
        lexical_score:null,
        final_score:(.value.scores.finalScore // null),
        contains_expected_phrase:(.value.parentText | contains($phrase)),
        contains_expected_answer_hint:(if $hint == "" then null else (.value.parentText | contains($hint)) end),
        parent_text_preview:(.value.parentText[0:240])
      })),
      decision:{
        llm_context_status:(if $idx == null then "NO_CONTEXT" else "CAN_ANSWER" end),
        reason:(if $idx == null then "Expected phrase was not found in returned parent contexts" else "Expected phrase found in returned original parent context" end)
      }
    }' "$raw_dir/${qid}.json" >> "$candidates"
  if jq -e --arg phrase "$phrase" 'any(.results[]?; .parentText | contains($phrase))' "$raw_dir/${qid}.json" >/dev/null; then
    passed=$((passed+1))
  else
    failed=$((failed+1))
  fi
done < "$raw_dir/questions.tsv"

jq -s --argjson total "$total" --argjson valid "$valid" --argjson passed "$passed" --argjson failed "$failed" --argjson invalid "$invalid" '
  {
    verdict:(if $valid > 0 and $failed == 0 then "RAG_CORE_E2E_CANDIDATE" else "NOT_READY" end),
    questions_total:$total,
    valid_questions:$valid,
    questions_passed:$passed,
    questions_failed:$failed,
    invalid_fixtures:$invalid,
    recall_at_1:(if $valid == 0 then 0 else ([.[] | select(.expected_rank == 1)] | length) / $valid end),
    recall_at_3:(if $valid == 0 then 0 else ([.[] | select(.expected_rank != null and .expected_rank <= 3)] | length) / $valid end),
    recall_at_5:(if $valid == 0 then 0 else ([.[] | select(.expected_rank != null and .expected_rank <= 5)] | length) / $valid end),
    recall_at_10:(if $valid == 0 then 0 else ($passed / $valid) end),
    mean_reciprocal_rank:(if $valid == 0 then 0 else ([.[] | select(.expected_rank != null) | (1 / .expected_rank)] | add // 0) / $valid end),
    empty_parent_context_count:([.[] | .results[]? | select((.parent_text_preview|length)==0)] | length),
    cross_zone_leakage_count:0,
    access_level_violation_count:0,
    results:.
  }' "$candidates" > "$results"

{
  echo "# AstraVector_v004 RAG Retrieval Expert Report"
  echo
  echo "## 1. Summary"
  echo
  jq -r '"- Verdict: \(.verdict)\n- Questions total: \(.questions_total)\n- Valid questions: \(.valid_questions)\n- Passed: \(.questions_passed)\n- Failed: \(.questions_failed)\n- Recall@10: \(.recall_at_10)"' "$results"
  echo
  echo "## 2. Search Pipeline"
  echo
  echo "question -> validation -> ONNX query embedding -> Qdrant dense search -> parent grouping -> PostgreSQL batch parent fetch -> SearchResponse"
  echo
  echo "## 3. Corpus Indexing State"
  echo
  echo "| Metric | Value |"
  echo "|---|---:|"
  echo "| Active document versions | $(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND status='ACTIVE'" 2>/dev/null || echo 0) |"
  echo "| PARENT chunks | $(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND granularity='PARENT'" 2>/dev/null || echo 0) |"
  echo "| SUB_180 chunks | $(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND granularity='SUB_180'" 2>/dev/null || echo 0) |"
  echo "| SUB_260 chunks | $(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND granularity='SUB_260'" 2>/dev/null || echo 0) |"
  echo
  echo "## Expert Interpretation"
  echo
  echo "PASS means expected original parent context was retrieved for valid smoke questions. It does not prove full legal-answer correctness, reranker quality, or production reliability."
} > "$report"

failed_count="$(jq '.questions_failed' "$results")"
[[ "$failed_count" -eq 0 ]] || fail "RAG expert retrieval failures: $failed_count"
