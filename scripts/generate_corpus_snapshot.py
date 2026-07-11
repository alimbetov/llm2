#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import subprocess
from pathlib import Path


def atomic_write(path: Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(body, encoding="utf-8")
    temporary.replace(path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database-url", default=os.environ.get("ASTRAVECTOR_DB_URL"))
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not args.database_url:
        raise SystemExit("ASTRAVECTOR_DB_URL or --database-url is required")
    query = """
SELECT json_build_object(
  'access_zone_id', access_zone_id::text,
  'document_id', document_id::text,
  'document_version', document_version,
  'content_hash', content_hash,
  'status', status
)::text
FROM astravector.document_versions
WHERE status = 'ACTIVE' AND delete_operation_id IS NULL
ORDER BY access_zone_id, document_id, document_version
"""
    result = subprocess.run(
        ["psql", args.database_url, "-XAt", "-v", "ON_ERROR_STOP=1", "-c", query],
        check=True,
        capture_output=True,
        text=True,
    )
    entries = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    if not entries:
        raise SystemExit("active corpus snapshot is empty")
    canonical = json.dumps(entries, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    snapshot_id = hashlib.sha256(canonical.encode()).hexdigest()
    document = {
        "schema_version": "1.0",
        "corpus_snapshot_id": snapshot_id,
        "entry_count": len(entries),
        "entries": entries,
    }
    atomic_write(args.output, json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    atomic_write(args.output.with_suffix(".sha256"), snapshot_id + "\n")
    print(snapshot_id)


if __name__ == "__main__":
    main()
