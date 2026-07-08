#!/usr/bin/env bash
set -uo pipefail

SMOKE_ROOT="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SMOKE_ROOT/../.." && pwd)"
source "$SMOKE_ROOT/lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env

REPORT="$REPORTS_DIR/BM25_HYBRID_RETRIEVAL_REPORT.md"
RESULTS="$REPORTS_DIR/bm25-hybrid-results.json"
CANDIDATES="$REPORTS_DIR/bm25-hybrid-candidates.jsonl"
RAW_DIR="$LOGS_DIR/bm25-hybrid-retrieval"
mkdir -p "$RAW_DIR"
: > "$CANDIDATES"

ZONE_A="${SMOKE_ACCESS_ZONE_A:-11111111-1111-4111-8111-111111111111}"
ZONE_B="${SMOKE_ACCESS_ZONE_B:-22222222-2222-4222-8222-222222222222}"
CALLER_ACCESS_LEVEL="PUBLIC"

queries=(
  "article-223|статья 223|Статья 223"
  "spouses-property|общая собственность супругов|общая собственность супругов"
  "article-143|статья 143|Статья 143"
  "honor-dignity|защита чести достоинства и деловой репутации|чести, достоинства и деловой репутации"
  "article-167|статья 167|Статья 167"
  "power-of-attorney|доверенность письменное уполномочие|письменное уполномочие"
  "article-246|статья 246|Статья 246"
  "stray-animals|безнадзорные животные|безнадзорные животные"
)

json_emit() {
  jq -nc "$@" >> "$CANDIDATES"
}

sql_available=1
run_sql_count() {
  local sql="$1"
  psql "$(postgres_url)" -Atqc "$sql" 2>/dev/null
}

proto_has_mode=0
proto_has_sparse_scores=0
grpc_uses_sparse_query=0
qdrant_has_sparse_search=0
qdrant_has_hybrid_search=0

rg -n "SearchRequestV004.*mode|retrieval_mode|search_mode|bm25_only|hybrid|dense_only" "$PROJECT_DIR/proto" "$PROJECT_DIR/src" >/dev/null 2>&1 && proto_has_mode=1
rg -n "message SearchScoresV004 .*sparse_score|message SearchScoresV004 .*lexical_score|message SearchScoresV004 .*bm25" "$PROJECT_DIR/proto/astravector_embedding.proto" >/dev/null 2>&1 && proto_has_sparse_scores=1
rg -n "QueueKind::Query[\s\S]*want_sparse:\s*true|want_sparse:\s*self\.engine\.sparse_available\(\)" "$PROJECT_DIR/src/grpc" >/dev/null 2>&1 && grpc_uses_sparse_query=1
rg -n "search_sparse|sparse.*points/search|using.*sparse|vector.*sparse" "$PROJECT_DIR/src/qdrant" >/dev/null 2>&1 && qdrant_has_sparse_search=1
rg -n "hybrid|rrf|reciprocal|fusion|bm25" "$PROJECT_DIR/src" >/dev/null 2>&1 && qdrant_has_hybrid_search=1

if ! sparse_rows="$(run_sql_count "SELECT count(*) FROM astravector.embedding_sparse")"; then
  sparse_rows=0
  sql_available=0
fi
if ! active_civil_versions="$(run_sql_count "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${ZONE_A}'::uuid AND status='ACTIVE'")"; then
  active_civil_versions=0
  sql_available=0
fi

blocked_reasons=()
[[ "$proto_has_mode" -eq 1 ]] || blocked_reasons+=("SearchRequestV004 has no retrieval/search mode field")
[[ "$proto_has_sparse_scores" -eq 1 ]] || blocked_reasons+=("SearchResponseV004 has no sparse/lexical/BM25 score fields")
[[ "$grpc_uses_sparse_query" -eq 1 ]] || blocked_reasons+=("Search query embedding uses want_sparse=false")
[[ "$qdrant_has_sparse_search" -eq 1 ]] || blocked_reasons+=("Qdrant client has no sparse/BM25 search method")
[[ "$qdrant_has_hybrid_search" -eq 1 ]] || blocked_reasons+=("No hybrid fusion path found in src")
if [[ "$sql_available" -eq 1 ]]; then
  [[ "${sparse_rows:-0}" -gt 0 ]] || blocked_reasons+=("embedding_sparse has no indexed rows")
  [[ "${active_civil_versions:-0}" -gt 0 ]] || blocked_reasons+=("Civil Code ACTIVE document version missing in Zone A")
else
  blocked_reasons+=("PostgreSQL smoke DB check unavailable")
fi

dense_pass=0
dense_fail=0
if grpc_plain list >/dev/null 2>&1; then
  for item in "${queries[@]}"; do
    IFS='|' read -r qid query expected <<<"$item"
    body="$(jq -n --arg zone "$ZONE_A" --arg q "$query" '{correlationId:"bm25-dense-baseline",accessZoneId:$zone,callerAccessLevel:"PUBLIC",query:$q,topK:3,candidateLimit:50,parentLimit:3,timeoutMs:15000}')"
    out="$RAW_DIR/dense-${qid}.json"
    err="$RAW_DIR/dense-${qid}.err"
    if grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/Search >"$out" 2>"$err"; then
      status="$(jq -r --arg expected "$expected" --arg zone "$ZONE_A" '
        [ .results[]? | {
          access_zone_ok:(.accessZoneId == $zone),
          access_level_ok:((.accessLevel // "PUBLIC") == "PUBLIC"),
          parent_text_original:(.parentText != null and (.parentText|length) > 0),
          expected_match:(.parentText | contains($expected))
        }] as $rows |
        if ($rows|length) == 0 then "FAIL"
        elif any($rows[]; (.access_zone_ok|not) or (.access_level_ok|not) or (.parent_text_original|not)) then "FAIL"
        elif any($rows[]; .expected_match) then "PASS"
        else "FAIL" end' "$out")"
      [[ "$status" == "PASS" ]] && dense_pass=$((dense_pass+1)) || dense_fail=$((dense_fail+1))
      jq -c --arg mode "dense_only" --arg qid "$qid" --arg q "$query" --arg expected "$expected" --arg status "$status" '
        {
          mode:$mode,
          question_id:$qid,
          query:$q,
          expected_parent_hint:$expected,
          status:$status,
          reason:(if $status == "PASS" then "expected parent hint appears in top-3 dense baseline" else "expected parent hint missing or validation failed in dense baseline" end),
          results:(.results | to_entries | map({
            rank:(.key+1),
            document_id:.value.documentId,
            document_version:.value.documentVersion,
            parent_chunk_id:.value.parentChunkId,
            matched_chunk_id:.value.matchedChunkId,
            matched_granularity:.value.matchedGranularity,
            access_zone_id:.value.accessZoneId,
            access_level:.value.accessLevel,
            lifecycle_status:"ACTIVE",
            dense_score:(.value.scores.denseScore // null),
            sparse_score:null,
            lexical_score:null,
            final_score:(.value.scores.finalScore // null),
            contains_expected_hint:(.value.parentText | contains($expected)),
            parent_text_preview:(.value.parentText[0:240])
          }))
        }' "$out" >> "$CANDIDATES"
    else
      dense_fail=$((dense_fail+1))
      json_emit --arg mode "dense_only" --arg qid "$qid" --arg q "$query" --rawfile err "$err" \
        '{mode:$mode,question_id:$qid,query:$q,status:"FAIL",reason:"Search gRPC failed",error:$err}'
    fi
  done
else
  for item in "${queries[@]}"; do
    IFS='|' read -r qid query expected <<<"$item"
    json_emit --arg mode "dense_only" --arg qid "$qid" --arg q "$query" --arg expected "$expected" \
      '{mode:$mode,question_id:$qid,query:$q,expected_parent_hint:$expected,status:"BLOCKED",reason:"AstraVectorV004Control gRPC service is not reachable"}'
  done
fi

for mode in bm25_only hybrid; do
  for item in "${queries[@]}"; do
    IFS='|' read -r qid query expected <<<"$item"
    if [[ "${#blocked_reasons[@]}" -gt 0 ]]; then
      jq -nc --arg mode "$mode" --arg qid "$qid" --arg q "$query" --arg expected "$expected" \
        --argjson reasons "$(printf '%s\n' "${blocked_reasons[@]}" | jq -R . | jq -s .)" \
        '{mode:$mode,question_id:$qid,query:$q,expected_parent_hint:$expected,status:"BLOCKED",reason:"BM25/sparse/hybrid retrieval path is absent",blocked_reasons:$reasons}' >> "$CANDIDATES"
    fi
  done
done

for mode in bm25_only hybrid; do
  jq -nc --arg mode "$mode" --arg zone_a "$ZONE_A" --arg zone_b "$ZONE_B" \
    --argjson reasons "$(printf '%s\n' "${blocked_reasons[@]}" | jq -R . | jq -s .)" \
    '{mode:$mode,test_id:"zone_b_lexical_secret_leakage",status:"BLOCKED",access_zone_a:$zone_a,access_zone_b:$zone_b,reason:"Zone B lexical leakage test requires BM25/hybrid retrieval execution path",blocked_reasons:$reasons}' >> "$CANDIDATES"
done

if [[ "${#blocked_reasons[@]}" -gt 0 ]]; then
  verdict="BM25_RETRIEVAL_BLOCKED"
  exit_code="$BLOCKED_STATUS"
else
  verdict="BM25_HYBRID_FAIL"
  exit_code="$FAIL_STATUS"
fi

jq -s \
  --arg verdict "$verdict" \
  --argjson dense_pass "$dense_pass" \
  --argjson dense_fail "$dense_fail" \
  --argjson sparse_rows "${sparse_rows:-0}" \
  --argjson active_civil_versions "${active_civil_versions:-0}" \
  --argjson proto_has_mode "$proto_has_mode" \
  --argjson proto_has_sparse_scores "$proto_has_sparse_scores" \
  --argjson grpc_uses_sparse_query "$grpc_uses_sparse_query" \
  --argjson qdrant_has_sparse_search "$qdrant_has_sparse_search" \
  --argjson qdrant_has_hybrid_search "$qdrant_has_hybrid_search" \
  --argjson sql_available "$sql_available" \
  --argjson blocked_reasons "$(printf '%s\n' "${blocked_reasons[@]}" | jq -R . | jq -s .)" \
  '{
    verdict:$verdict,
    detection:{
      proto_has_retrieval_mode:($proto_has_mode == 1),
      proto_has_sparse_or_lexical_scores:($proto_has_sparse_scores == 1),
      search_query_requests_sparse_embedding:($grpc_uses_sparse_query == 1),
      qdrant_sparse_search_implemented:($qdrant_has_sparse_search == 1),
      hybrid_fusion_implemented:($qdrant_has_hybrid_search == 1),
      sql_available:($sql_available == 1),
      embedding_sparse_rows:$sparse_rows,
      active_civil_versions:$active_civil_versions
    },
    dense_only:{passed:$dense_pass,failed:$dense_fail},
    bm25_only:{status:(if $verdict == "BM25_RETRIEVAL_BLOCKED" then "BLOCKED" else "FAIL" end)},
    hybrid:{status:(if $verdict == "BM25_RETRIEVAL_BLOCKED" then "BLOCKED" else "FAIL" end)},
    zone_b_leakage:{status:(if $verdict == "BM25_RETRIEVAL_BLOCKED" then "BLOCKED" else "FAIL" end)},
    blocked_reasons:$blocked_reasons,
    candidates:.
  }' "$CANDIDATES" > "$RESULTS"

{
  echo "# AstraVector_v004 BM25 / Sparse / Hybrid Retrieval Report"
  echo
  echo "## Verdict"
  echo "$verdict"
  echo
  echo "## Detection"
  echo
  echo "| Check | Value |"
  echo "|---|---:|"
  echo "| SearchRequest retrieval mode | $([[ "$proto_has_mode" -eq 1 ]] && echo yes || echo no) |"
  echo "| SearchResponse sparse/lexical scores | $([[ "$proto_has_sparse_scores" -eq 1 ]] && echo yes || echo no) |"
  echo "| Query sparse embedding requested | $([[ "$grpc_uses_sparse_query" -eq 1 ]] && echo yes || echo no) |"
  echo "| Qdrant sparse/BM25 search method | $([[ "$qdrant_has_sparse_search" -eq 1 ]] && echo yes || echo no) |"
  echo "| Hybrid fusion path | $([[ "$qdrant_has_hybrid_search" -eq 1 ]] && echo yes || echo no) |"
  echo "| PostgreSQL smoke DB check | $([[ "$sql_available" -eq 1 ]] && echo available || echo unavailable) |"
  echo "| embedding_sparse rows | ${sparse_rows:-0} |"
  echo "| ACTIVE Civil Code versions in Zone A | ${active_civil_versions:-0} |"
  echo
  echo "## Dense Baseline"
  echo
  echo "- dense_only passed: $dense_pass"
  echo "- dense_only failed: $dense_fail"
  echo
  echo "## BM25 / Hybrid"
  echo
  if [[ "${#blocked_reasons[@]}" -gt 0 ]]; then
    echo "BM25/sparse/hybrid retrieval is blocked, not passed."
    echo
    echo "Blocked reasons:"
    printf -- "- %s\n" "${blocked_reasons[@]}"
  else
    echo "BM25/hybrid path detected but this smoke has not received executable mode wiring."
  fi
  echo
  echo "## Artifacts"
  echo
  echo "- Results JSON: $RESULTS"
  echo "- Candidates JSONL: $CANDIDATES"
} > "$REPORT"

exit "$exit_code"
