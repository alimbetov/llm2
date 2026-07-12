#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path

FIELDS = (
    "git_sha", "binary_sha256", "effective_config_sha256", "model_sha256",
    "tokenizer_sha256", "corpus_snapshot_sha256", "query_bank_sha256", "qrels_sha256",
)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("runs", nargs=3, type=Path)
    args = parser.parse_args()
    reports = [json.loads(path.read_text(encoding="utf-8")) for path in args.runs]
    identities = [report.get("release_identity", {}) for report in reports]
    mismatches = [field for field in FIELDS if len({identity.get(field) for identity in identities}) != 1]
    missing = [field for field in FIELDS if any(not identity.get(field) for identity in identities)]
    passed = [report.get("overall_verdict") == "PASS" for report in reports]
    verdict = "PASS" if all(passed) and not mismatches and not missing else "FAIL"
    report = {
        "schema_version": 1,
        "overall_verdict": verdict,
        "runs_required": 3,
        "runs_completed": len(reports),
        "runs_passed": sum(passed),
        "identity_consistent": not mismatches and not missing,
        **{field: identities[0].get(field) for field in FIELDS},
        "identity_mismatches": mismatches,
        "missing_identity_fields": missing,
        "run_reports": [str(path.resolve()) for path in args.runs],
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.output_dir / "fix480-repeatability-report.json"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (args.output_dir / "fix480-repeatability-report.md").write_text(
        f"# fix480 Repeatability\n\nVerdict: `{verdict}`\n\nRuns passed: `{sum(passed)}/3`.\n",
        encoding="utf-8",
    )
    files = [json_path, args.output_dir / "fix480-repeatability-report.md"]
    (args.output_dir / "fix480-repeatability-checksums.sha256").write_text(
        "".join(f"{sha(path)}  {path.name}\n" for path in files), encoding="utf-8"
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    raise SystemExit(0 if verdict == "PASS" else 1)


if __name__ == "__main__":
    main()
