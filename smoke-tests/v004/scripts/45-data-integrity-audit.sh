#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
die() { fail "$1"; exit "$FAIL_STATUS"; }
doc="${CIVIL_CODE_DOCUMENT_ID:-72fd8953-9f11-5eef-a03c-ef47c3d40daa}"
zone="${SMOKE_ACCESS_ZONE_A}"
out="$REPORTS_DIR/full-power-data-integrity.tsv"
psql "$(postgres_url)" -At -F $'\t' -f "$SMOKE_ROOT/sql/data-integrity-audit-v004.sql" > "$out" || die "data integrity SQL failed"
violations="$(awk -F $'\t' '{s+=$2} END{print s+0}' "$out")"
bindings="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid AND qdrant_sync_status='SYNCED' AND chunk_granularity IN('PARENT','SUB_180','SUB_260')")" || die "binding count failed"
qdrant="$(curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$zone" --arg doc "$doc" '{exact:true,filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}},{key:"lifecycle_status",match:{value:"ACTIVE"}},{key:"chunk_granularity",match:{any:["PARENT","SUB_180","SUB_260"]}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0')" || die "qdrant count failed"
sample_missing="$(curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$zone" --arg doc "$doc" '{limit:10,with_payload:true,with_vector:false,filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/scroll" | jq '[.result.points[].payload | select((has("access_zone_id") and has("binding_id") and has("document_id") and has("document_version") and has("root_chunk_id") and has("source_chunk_id") and has("chunk_id") and has("chunk_granularity") and has("representation_type") and has("access_level") and has("lifecycle_status") and has("payload_version") and has("model_version") and has("tokenizer_version") and has("chunking_profile_version")) | not)] | length')" || die "qdrant payload sample failed"
jq -n --argjson pg "$violations" --argjson bindings "$bindings" --argjson qdrant "$qdrant" --argjson missing "$sample_missing" \
  '{postgres_integrity_violations:$pg,synced_searchable_bindings:$bindings,qdrant_points_for_civil_code:$qdrant,qdrant_count_mismatch:(if $bindings==$qdrant then 0 else 1 end),qdrant_payload_missing_required_fields:$missing}' > "$REPORTS_DIR/full-power-data-integrity.json"
{
  echo "# Data Integrity Audit v004"
  echo
  echo "| Check | Violations |"
  echo "|---|---:|"
  awk -F $'\t' '{print "| `" $1 "` | " $2 " |"}' "$out"
  echo
  echo "- Synced searchable bindings: $bindings"
  echo "- Qdrant points for Civil Code: $qdrant"
  echo "- Qdrant payload missing required fields: $sample_missing"
} > "$REPORTS_DIR/data-integrity-audit-report.md"
[[ "$violations" -eq 0 ]] || die "PostgreSQL integrity violations: $violations"
[[ "$bindings" -eq "$qdrant" ]] || die "Qdrant count mismatch bindings=$bindings qdrant=$qdrant"
[[ "$sample_missing" -eq 0 ]] || die "Qdrant payload missing required fields: $sample_missing"
