#!/usr/bin/env python3
"""Run the complete fix481 static/integration gate and persist its evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    args.evidence_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.setdefault(
        "DATABASE_URL",
        "postgres://astravector:astravector@127.0.0.1:55432/astravector",
    )
    started = datetime.now(timezone.utc)
    result = subprocess.run(
        ["make", "verify-fix481"],
        cwd=Path(__file__).resolve().parents[1],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    log = args.evidence_dir / "static-gate.log"
    log.write_bytes(result.stdout)
    report = {
        "schema_version": 1,
        "command": ["make", "verify-fix481"],
        "started_at": started.isoformat(),
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "exit_code": result.returncode,
        "status": "PASS" if result.returncode == 0 else "FAIL",
        "stdout_stderr_sha256": hashlib.sha256(result.stdout).hexdigest(),
    }
    (args.evidence_dir / "static-gate.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
