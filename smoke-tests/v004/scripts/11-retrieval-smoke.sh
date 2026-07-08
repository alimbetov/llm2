#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env
die() { fail "$1"; exit "$FAIL_STATUS"; }
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
command -v psql >/dev/null 2>&1 || blocked "psql not found"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control"

active_doc="$(psql "$(postgres_url)" -At -F $'\t' -c "SELECT document_id,document_version FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND status='ACTIVE' ORDER BY activated_at DESC NULLS LAST, updated_at DESC LIMIT 1")" || die "active document SQL failed"
[[ -n "$active_doc" ]] || blocked "no ACTIVE document version exists; run chunking/outbox/document-version first"
query="$(psql "$(postgres_url)" -Atqc "SELECT content FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='$(cut -f1 <<<"$active_doc")'::uuid AND document_version=$(cut -f2 <<<"$active_doc") AND granularity='PARENT' AND representation_type='ORIGINAL' LIMIT 1")" || die "parent query fixture SQL failed"
[[ -n "$query" ]] || die "active document has no original parent text"

body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg query "$query" '{
  correlationId:"retrieval-smoke",
  accessZoneId:$zone,
  callerAccessLevel:"PUBLIC",
  query:$query,
  topK:3,
  candidateLimit:12,
  parentLimit:3,
  timeoutMs:10000
}')"
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/Search >"$LOGS_DIR/retrieval-search-response.json" 2>"$LOGS_DIR/retrieval-search.err" || die "Search call failed"
jq -e '.results|length >= 1' "$LOGS_DIR/retrieval-search-response.json" >/dev/null || die "Search returned no results"
jq -e 'all(.results[]; (.parentText|length > 0) and .matchedChunkId != "" and (.scores.finalScore >= 0) and .matchedGranularity != "SOURCE_V004")' "$LOGS_DIR/retrieval-search-response.json" >/dev/null || die "Search result missing parent evidence or returned SOURCE"
jq -e '.diagnostics.queryEmbeddingMs >= 0 and .diagnostics.qdrantSearchMs >= 0 and .diagnostics.parentFetchMs >= 0 and .diagnostics.totalMs >= 0 and .diagnostics.candidateCount >= 1 and .diagnostics.parentGroupCount >= 1' "$LOGS_DIR/retrieval-search-response.json" >/dev/null || die "Search diagnostics missing"

parent_id="$(jq -r '.results[0].parentChunkId' "$LOGS_DIR/retrieval-search-response.json")"
resolve_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg parent "$parent_id" '{accessZoneId:$zone, chunkIds:[$parent], maxContextTokens:800}')"
grpc_plain -d "$resolve_body" astravector.embedding.v1.AstraVectorV004Control/ResolveParentContext >"$LOGS_DIR/retrieval-parent-response.json" 2>"$LOGS_DIR/retrieval-parent.err" || die "ResolveParentContext call failed"
jq -e --arg parent "$parent_id" '.contexts|length == 1 and .[0].chunk.chunkId == $parent and (.[0].content|length > 0) and .[0].representationType == "ORIGINAL"' "$LOGS_DIR/retrieval-parent-response.json" >/dev/null || die "ResolveParentContext did not return original parent context"

empty_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" '{accessZoneId:$zone, callerAccessLevel:"PUBLIC", query:"", topK:1, candidateLimit:1, parentLimit:1}')"
if grpc_plain -d "$empty_body" astravector.embedding.v1.AstraVectorV004Control/Search >"$LOGS_DIR/retrieval-empty-response.json" 2>"$LOGS_DIR/retrieval-empty.err"; then
  die "empty Search unexpectedly succeeded"
fi
grep -q "Code: InvalidArgument" "$LOGS_DIR/retrieval-empty.err" || die "empty Search did not return INVALID_ARGUMENT"

foreign_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_B" --arg query "$query" '{accessZoneId:$zone, callerAccessLevel:"PUBLIC", query:$query, topK:3, candidateLimit:12, parentLimit:3, timeoutMs:10000}')"
grpc_plain -d "$foreign_body" astravector.embedding.v1.AstraVectorV004Control/Search >"$LOGS_DIR/retrieval-foreign-response.json" 2>"$LOGS_DIR/retrieval-foreign.err" || die "foreign access zone Search failed"
jq -e '(.results // [])|length == 0' "$LOGS_DIR/retrieval-foreign-response.json" >/dev/null || die "foreign access zone returned data"

psql "$(postgres_url)" -v ON_ERROR_STOP=1 -c "UPDATE astravector.content_chunks_v004 SET access_level=2 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND id='${parent_id}'::uuid" >/dev/null || die "failed to raise parent access level"
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/Search >"$LOGS_DIR/retrieval-access-response.json" 2>"$LOGS_DIR/retrieval-access.err" || die "access-filter Search failed"
jq -e --arg parent "$parent_id" 'all((.results // [])[]; .parentChunkId != $parent)' "$LOGS_DIR/retrieval-access-response.json" >/dev/null || die "caller_access_level filter did not exclude raised parent"
psql "$(postgres_url)" -v ON_ERROR_STOP=1 -c "UPDATE astravector.content_chunks_v004 SET access_level=1 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND id='${parent_id}'::uuid" >/dev/null || die "failed to restore parent access level"
