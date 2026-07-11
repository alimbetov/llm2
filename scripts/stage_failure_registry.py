#!/usr/bin/env python3
import argparse
import datetime as dt
import json
import os
import tempfile
from pathlib import Path


CLASSIFICATIONS = {
    "BLOCKED",
    "STATIC_FAILURE",
    "RUNTIME_FAILURE",
    "QUALITY_FAILURE",
    "INTEGRITY_FAILURE",
    "EVIDENCE_FAILURE",
}


def read_registry(path: Path) -> dict:
    if not path.exists():
        return {"schema_version": "1.0", "failures": []}
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    if value.get("schema_version") != "1.0" or not isinstance(value.get("failures"), list):
        raise SystemExit("invalid stage failure registry")
    return value


def write_atomic(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(value, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("registry", type=Path)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("init")
    add = subparsers.add_parser("add")
    add.add_argument("--stage", required=True)
    add.add_argument("--code", required=True)
    add.add_argument("--classification", required=True, choices=sorted(CLASSIFICATIONS))
    add.add_argument("--details-json", default="{}")
    args = parser.parse_args()

    if args.command == "init":
        write_atomic(args.registry, {"schema_version": "1.0", "failures": []})
        return

    registry = read_registry(args.registry)
    details = json.loads(args.details_json)
    if not isinstance(details, dict):
        raise SystemExit("details-json must be an object")
    registry["failures"].append(
        {
            "stage": args.stage,
            "code": args.code,
            "classification": args.classification,
            "timestamp": dt.datetime.now().astimezone().isoformat(),
            "details": details,
        }
    )
    write_atomic(args.registry, registry)


if __name__ == "__main__":
    main()
