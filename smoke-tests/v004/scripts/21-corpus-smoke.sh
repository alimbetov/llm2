#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env

die() { fail "$1"; exit "$FAIL_STATUS"; }
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
command -v psql >/dev/null 2>&1 || blocked "psql not found"
command -v file >/dev/null 2>&1 || blocked "file command not found"
[[ -f "$ASTRAVECTOR_CORPUS_DIR" ]] || blocked "corpus path is not a single file: $ASTRAVECTOR_CORPUS_DIR"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control"

mime="$(file -b --mime "$ASTRAVECTOR_CORPUS_DIR")" || die "file MIME check failed"
case "$mime" in
  *charset=utf-8*|*charset=us-ascii*) ;;
  *) die "corpus is not UTF-8 text: $mime" ;;
esac

unit_dir="$LOGS_DIR/corpus-units"
mkdir -p "$unit_dir"
manifest="$unit_dir/manifest.tsv"
units_jsonl="$unit_dir/source-units.jsonl"
meta_json="$unit_dir/corpus-meta.json"

python3 - "$ASTRAVECTOR_CORPUS_DIR" "$units_jsonl" "$manifest" "$meta_json" <<'PY'
import hashlib, json, pathlib, re, sys, uuid
path = pathlib.Path(sys.argv[1])
units_path = pathlib.Path(sys.argv[2])
manifest_path = pathlib.Path(sys.argv[3])
meta_path = pathlib.Path(sys.argv[4])
data = path.read_bytes()
text = data.decode("utf-8")
sha = hashlib.sha256(data).hexdigest()
doc_id = str(uuid.uuid5(uuid.NAMESPACE_URL, f"astravector:v004:corpus:{path}:{sha}"))
parts = re.split(r"(?m)(?=^Статья\s+\d+[\.-]?\s+)", text)
blocks = [p.strip() for p in parts if p.strip()]
if len(blocks) < 5:
    blocks = [p.strip() for p in re.split(r"\n\s*\n", text) if p.strip()]
units = []
buf = []
buf_len = 0
max_chars = 32000
for block in blocks:
    extra = len(block) + 2
    if buf and buf_len + extra > max_chars:
        units.append("\n\n".join(buf))
        buf = []
        buf_len = 0
    if len(block) > max_chars:
        for i in range(0, len(block), max_chars):
            chunk = block[i:i+max_chars].strip()
            if chunk:
                units.append(chunk)
        continue
    buf.append(block)
    buf_len += extra
if buf:
    units.append("\n\n".join(buf))
with units_path.open("w", encoding="utf-8") as f:
    for idx, unit in enumerate(units, 1):
        if len(unit.encode("utf-8")) > 2 * 1024 * 1024:
            raise SystemExit(f"source unit {idx} exceeds 2 MiB")
        f.write(json.dumps({"unit": idx, "source_text": unit}, ensure_ascii=False) + "\n")
with manifest_path.open("w", encoding="utf-8") as f:
    for idx, unit in enumerate(units, 1):
        f.write(f"{idx}\t{len(unit)}\t{hashlib.sha256(unit.encode('utf-8')).hexdigest()}\n")
meta_path.write_text(json.dumps({
    "file": str(path),
    "sha256": sha,
    "document_id": doc_id,
    "document_version": 1,
    "source_units": len(units),
    "bytes": len(data)
}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(doc_id)
PY

document_id="$(jq -r '.document_id' "$meta_json")"
content_hash="$(jq -r '.sha256' "$meta_json")"
unit_count="$(jq -r '.source_units' "$meta_json")"
[[ "$unit_count" -gt 0 ]] || die "corpus splitter produced no source units"

psql "$(postgres_url)" -v ON_ERROR_STOP=1 \
  -c "DELETE FROM astravector.vector_outbox o USING astravector.vector_bindings_v004 b WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND b.access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND b.document_id='${document_id}'::uuid" \
  -c "DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid" \
  -c "DELETE FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid" \
  -c "DELETE FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid" >/dev/null || die "failed to clear previous corpus document state"
curl -sS -X POST -H 'content-type: application/json' \
  --data "$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" '{filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}}]}}')" \
  "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/delete?wait=true" >/dev/null || die "failed to clear previous corpus Qdrant points"

register_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" --arg hash "$content_hash" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:1,
  contentHash:$hash,
  activationPolicy:"ACTIVE_LATEST_ONLY"
}')"
grpc_plain -d "$register_body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOGS_DIR/corpus-register-response.json" 2>"$LOGS_DIR/corpus-register.err" || die "RegisterDocumentVersion for corpus failed"
jq -e '.status == "REGISTERED"' "$LOGS_DIR/corpus-register-response.json" >/dev/null || die "corpus register response mismatch"

i=0
while IFS= read -r unit_json; do
  i=$((i+1))
  source_text="$(jq -r '.source_text' <<<"$unit_json")"
  body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" --arg text "$source_text" --arg unit "$i" --arg file "$ASTRAVECTOR_CORPUS_DIR" '{
    accessZoneId:$zone,
    documentId:$doc,
    documentVersion:1,
    sourceText:$text,
    accessLevel:"PUBLIC",
    profile:{
      preserveHeadings:true,
      preserveParagraphs:true,
      preserveSentences:true,
      profileVersion:"civil-code-corpus-v1",
      parent:{granularity:"PARENT_V004",targetTokens:100,minTokens:10,maxTokens:140,overlapTokens:0},
      granularities:[
        {granularity:"SUB_180_V004",targetTokens:70,minTokens:5,maxTokens:100,overlapTokens:0},
        {granularity:"SUB_260_V004",targetTokens:90,minTokens:5,maxTokens:120,overlapTokens:0}
      ]
    },
    metadata:{corpus_id:"civil-code-rk", source_file:$file, source_unit:$unit},
    idempotencyKey:("civil-code-rk-" + $unit),
    correlationId:"corpus-smoke"
  }')"
  grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >"$unit_dir/unit-${i}.response.json" 2>"$unit_dir/unit-${i}.err" || die "CreateMultiGranularityChunks failed for source unit $i"
  jq -e '.status == "INDEXING" and (.totalChunks >= 4)' "$unit_dir/unit-${i}.response.json" >/dev/null || die "chunking response mismatch for source unit $i"
done < "$units_jsonl"
[[ "$i" -eq "$unit_count" ]] || die "processed source units mismatch"

for _ in $(seq 1 120); do
  psql "$(postgres_url)" -At -F $'\t' -c "SELECT chunk_granularity,qdrant_sync_status,count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid GROUP BY chunk_granularity,qdrant_sync_status ORDER BY chunk_granularity,qdrant_sync_status" >"$LOGS_DIR/corpus-bindings.tsv" || die "corpus bindings SQL failed"
  psql "$(postgres_url)" -At -F $'\t' -c "SELECT operation,status,count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND b.document_id='${document_id}'::uuid GROUP BY operation,status ORDER BY operation,status" >"$LOGS_DIR/corpus-outbox.tsv" || die "corpus outbox SQL failed"
  searchable="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid AND chunk_granularity IN('PARENT','SUB_180','SUB_260')")" || searchable=0
  synced="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid AND chunk_granularity IN('PARENT','SUB_180','SUB_260') AND qdrant_sync_status='SYNCED'")" || synced=0
  completed="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND b.document_id='${document_id}'::uuid AND o.operation='UPSERT_POINT' AND o.status='COMPLETED'")" || completed=0
  [[ "$searchable" -gt 0 && "$synced" -eq "$searchable" && "$completed" -eq "$searchable" ]] && break
  sleep 2
done
[[ "${searchable:-0}" -gt 0 ]] || die "corpus produced no searchable bindings"
[[ "$synced" -eq "$searchable" ]] || die "not all corpus bindings are SYNCED"
[[ "$completed" -eq "$searchable" ]] || die "not all corpus outbox events are COMPLETED"

qdrant_count="$(curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" '{exact:true,filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}},{key:"chunk_granularity",match:{any:["PARENT","SUB_180","SUB_260"]}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0')" || die "Qdrant count failed"
[[ "$qdrant_count" -eq "$searchable" ]] || die "Qdrant point count mismatch expected=$searchable actual=$qdrant_count"

activate_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" '{accessZoneId:$zone, documentId:$doc, documentVersion:1}')"
grpc_plain -d "$activate_body" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$LOGS_DIR/corpus-activate-response.json" 2>"$LOGS_DIR/corpus-activate.err" || die "ActivateDocumentVersion for corpus failed"
jq -e '.status == "ACTIVE"' "$LOGS_DIR/corpus-activate-response.json" >/dev/null || die "corpus document did not activate"

psql "$(postgres_url)" -At -F $'\t' -c "SELECT granularity,count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid GROUP BY granularity ORDER BY granularity" >"$LOGS_DIR/corpus-chunks.tsv" || die "corpus chunk counts SQL failed"
grep -q $'SOURCE\t' "$LOGS_DIR/corpus-chunks.tsv" || die "corpus SOURCE chunks missing"
grep -q $'PARENT\t' "$LOGS_DIR/corpus-chunks.tsv" || die "corpus PARENT chunks missing"
grep -q $'SUB_180\t' "$LOGS_DIR/corpus-chunks.tsv" || die "corpus SUB_180 chunks missing"
grep -q $'SUB_260\t' "$LOGS_DIR/corpus-chunks.tsv" || die "corpus SUB_260 chunks missing"

jq -n --arg doc "$document_id" --arg hash "$content_hash" --argjson units "$unit_count" --argjson searchable "$searchable" --argjson qdrant "$qdrant_count" '{document_id:$doc, content_hash:$hash, source_units:$units, searchable_bindings:$searchable, qdrant_points:$qdrant, status:"ACTIVE"}' > "$LOGS_DIR/corpus-ingestion-summary.json"
