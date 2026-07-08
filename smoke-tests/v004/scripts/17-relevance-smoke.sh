#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
body='{"tenantId":"smoke-tenant","workspaceId":"smoke-workspace","question":"Каков общий срок исковой давности?","candidateText":"Срок исковой давности составляет три года.","answer":"три года","sourceTexts":["Срок исковой давности составляет три года."]}'
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorRuntime/EvaluateRelevance >"$LOGS_DIR/relevance-response.json" 2>"$LOGS_DIR/relevance.err" || fail "EvaluateRelevance call failed"
jq -e '.finalScore >= 0 and .finalScore <= 1 and (.evaluationId|length > 0)' "$LOGS_DIR/relevance-response.json" >/dev/null || fail "relevance scores are invalid"
