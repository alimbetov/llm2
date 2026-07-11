#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


IDENTITY_FIELDS = (
    "git_sha", "binary_sha256", "effective_config_sha256", "model_sha256",
    "tokenizer_sha256", "migration_head", "corpus_snapshot_sha256",
    "query_bank_sha256", "machine_identity", "rust_version",
    "load_driver_version", "report_schema_version",
)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("runs", nargs=3, type=Path)
    args = parser.parse_args()
    reports = [json.loads(path.read_text(encoding="utf-8")) for path in args.runs]
    identities = [{key: report.get("release_identity", {}).get(key) for key in IDENTITY_FIELDS} for report in reports]
    incomplete = [key for key in IDENTITY_FIELDS if any(identity.get(key) in (None, "") for identity in identities)]
    mismatches = [key for key in IDENTITY_FIELDS if len({identity.get(key) for identity in identities}) != 1]
    passed = [report.get("overall_verdict") == "PASS" for report in reports]
    verdict = "PASS" if all(passed) and not incomplete and not mismatches else "FAIL"
    output = {
        "schema_version": "1.0",
        "completed_runs": len(reports),
        "passed_runs": sum(passed),
        "failed_runs": len(reports) - sum(passed),
        "identity_mismatches": mismatches,
        "incomplete_identity_fields": incomplete,
        "release_identity": identities[0] if not mismatches and not incomplete else None,
        "verdict": verdict,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_suffix(".tmp")
    temporary.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(args.output)
    print(json.dumps(output, indent=2, sort_keys=True))
    if verdict != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
