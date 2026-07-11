#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", required=True, type=Path)
    parser.add_argument("--queries-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    profile = json.loads(args.profile.read_text(encoding="utf-8"))
    lines = []
    seen = set()
    for name in profile.get("queries", []):
        source = args.queries_dir / f"{name}.jsonl"
        if not source.is_file():
            raise SystemExit(f"query set does not exist: {source}")
        for raw in source.read_text(encoding="utf-8").splitlines():
            if not raw.strip():
                continue
            query = json.loads(raw)
            query_id = query.get("id")
            if not query_id or query_id in seen:
                raise SystemExit(f"missing or duplicate query id: {query_id}")
            seen.add(query_id)
            lines.append(json.dumps(query, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
    if not lines:
        raise SystemExit("profile produced an empty query bank")
    body = "\n".join(lines) + "\n"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_suffix(".tmp")
    temporary.write_text(body, encoding="utf-8")
    temporary.replace(args.output)
    digest = hashlib.sha256(body.encode()).hexdigest()
    args.output.with_suffix(".sha256").write_text(digest + "\n", encoding="utf-8")
    print(json.dumps({"queries": len(lines), "sha256": digest}))


if __name__ == "__main__":
    main()
