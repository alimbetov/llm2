#!/usr/bin/env python3
"""Read-only FIX487B audit classifiers."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


HARD_GATE_KEYS = (
    "orphan_binding_count",
    "orphan_outbox_count",
    "duplicate_canonical_identity_count",
    "cross_zone_binding_anomaly_count",
    "data_corruption_count",
    "failed_outbox",
    "dead_letters",
    "missing_active_qdrant_points_after_cooldown",
    "foreign_points",
    "invalid_searchable_lifecycle_points",
)


def classify_integrity(counters: dict) -> dict:
    violations = {key: int(counters.get(key, 0)) for key in HARD_GATE_KEYS}
    status = "PASS" if all(value == 0 for value in violations.values()) else "FAIL"
    return {"status": status, "violations": violations}


def postgres_audit_sql() -> str:
    return """
SELECT 'orphan_binding_count' AS metric, COUNT(*)::bigint AS value
FROM vector_bindings_v004 vb
LEFT JOIN content_chunks_v004 c ON c.id = vb.chunk_id
WHERE c.id IS NULL
UNION ALL
SELECT 'orphan_outbox_count', COUNT(*)::bigint
FROM vector_outbox vo
LEFT JOIN vector_bindings_v004 vb ON vb.id = vo.binding_id
WHERE vb.id IS NULL
UNION ALL
SELECT 'failed_outbox', COUNT(*)::bigint
FROM vector_outbox
WHERE status IN ('FAILED', 'DEAD_LETTER');
""".strip()


def qdrant_payload_required_fields() -> tuple[str, ...]:
    return (
        "access_zone_id",
        "binding_id",
        "document_id",
        "document_version",
        "root_chunk_id",
        "source_chunk_id",
        "parent_chunk_id",
        "chunk_id",
        "chunk_granularity",
        "representation_type",
        "access_level",
        "lifecycle_status",
        "payload_version",
        "model_version",
        "tokenizer_version",
        "chunking_profile_version",
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--counters-json")
    parser.add_argument("--print-sql", action="store_true")
    args = parser.parse_args()
    if args.print_sql:
        print(postgres_audit_sql())
        return 0
    counters = json.loads(Path(args.counters_json).read_text(encoding="utf-8")) if args.counters_json else {}
    result = classify_integrity(counters)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
