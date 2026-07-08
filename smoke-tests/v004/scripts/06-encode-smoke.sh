#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env

command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
rc=0
emb_task_id="11111111-1111-4111-8111-$(printf '%012d' $((RANDOM+1)))"
correlation_id="33333333-3333-4333-8333-$(printf '%012d' $((RANDOM+1)))"
chunk_id="44444444-4444-4444-8444-$(printf '%012d' $((RANDOM+1)))"
idempotency_key="smoke-encode-${SMOKE_RUN_ID:-local}-${chunk_id}"
body="$(jq -n --arg emb "$emb_task_id" --arg corr "$correlation_id" --arg chunk "$chunk_id" --arg idem "$idempotency_key" '{
  embTaskId:$emb, correlationId:$corr, idempotencyKey:$idem,
  tenantId:"smoke-tenant", workspaceId:"smoke-workspace", callerService:"smoke",
  accessLevel:"PUBLIC", purpose:"QUERY", requestedRepresentations:["DENSE"],
  expectedContractVersion:"astravector_embedding_contract_v4_0",
  expectedTokenizerVersion:"bge_m3_tokenizer_v1",
  expectedEmbeddingVersion:"bge_m3_dense_cls_l2_v1_onnx_int8",
  persistenceMode:"NONE",
  item:{chunkId:$chunk, chunkType:"CHILD", parentChunkId:"55555555-5555-4555-8555-555555555555", text:"Срок исковой давности составляет три года.", accessLevel:"PUBLIC", representationType:"ORIGINAL"}
}')"
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorRuntime/Encode >"$LOGS_DIR/encode-response.json" 2>"$LOGS_DIR/encode.err" || { fail "Encode call failed"; rc=1; echo '{}' > "$LOGS_DIR/encode-response.json"; }
jq -e '.item.status == "ITEM_COMPLETED"' "$LOGS_DIR/encode-response.json" >/dev/null || { fail "Encode item is not ITEM_COMPLETED"; rc=1; }
jq -e '.item.dense.values | length == 1024' "$LOGS_DIR/encode-response.json" >/dev/null || { fail "dense vector dimension is not 1024"; rc=1; }
jq -e 'all(.item.dense.values[]; type=="number" and (. == .) and (. != infinite) and (. != -infinite))' "$LOGS_DIR/encode-response.json" >/dev/null || { fail "dense vector contains invalid number"; rc=1; }
exit "$rc"
