#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
load_smoke_env
command -v psql >/dev/null 2>&1 || blocked "psql not found"
for _ in $(seq 1 45); do
  psql "$(postgres_url)" -At -F $'\t' -c "SELECT status,count(*)::text FROM astravector.vector_outbox WHERE binding_access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid GROUP BY status ORDER BY status" >"$LOGS_DIR/outbox.tsv" || fail "outbox SQL failed"
  psql "$(postgres_url)" -At -F $'\t' -c "SELECT qdrant_sync_status,count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid GROUP BY qdrant_sync_status ORDER BY qdrant_sync_status" >"$LOGS_DIR/binding-sync-status.tsv" || fail "binding sync SQL failed"

  if [[ -s "$LOGS_DIR/outbox.tsv" ]]; then
    completed="$(awk -F $'\t' '$1=="COMPLETED"{print $2}' "$LOGS_DIR/outbox.tsv" | head -n 1)"
    if [[ "${completed:-0}" -gt 0 ]] \
      && ! grep -Eq '^(PENDING|PROCESSING|RETRY_PENDING|DEAD_LETTER)\t[1-9]' "$LOGS_DIR/outbox.tsv" \
      && grep -q $'SYNCED\t' "$LOGS_DIR/binding-sync-status.tsv" \
      && ! grep -Eq '^(PENDING|UPDATE_PENDING|DELETE_PENDING|FAILED)\t[1-9]' "$LOGS_DIR/binding-sync-status.tsv"; then
      exit 0
    fi
  fi

  sleep 1
done

if [[ ! -s "$LOGS_DIR/outbox.tsv" ]]; then
  blocked "no outbox events exist; persistence/chunking scenario did not create bindings"
fi
completed="$(awk -F $'\t' '$1=="COMPLETED"{print $2}' "$LOGS_DIR/outbox.tsv" | head -n 1)"
[[ "${completed:-0}" -gt 0 ]] || fail "no completed outbox events"
if grep -Eq '^(PENDING|PROCESSING|RETRY_PENDING|DEAD_LETTER)\t[1-9]' "$LOGS_DIR/outbox.tsv"; then
  fail "outbox has unfinished or failed events"
fi
grep -q $'SYNCED\t' "$LOGS_DIR/binding-sync-status.tsv" || fail "no synced vector bindings"
if grep -Eq '^(PENDING|UPDATE_PENDING|DELETE_PENDING|FAILED)\t[1-9]' "$LOGS_DIR/binding-sync-status.tsv"; then
  fail "bindings contain unsynced rows"
fi
