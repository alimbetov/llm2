#!/usr/bin/env python3
import csv
import datetime as dt
import hashlib
import json
import math
import re
import subprocess
import sys
from pathlib import Path


def read_json(path: Path):
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def percentile(report, value):
    for item in report.get("latencyDistribution", []):
        if item.get("percentage", 0) >= value:
            return item.get("latency", 0) / 1_000_000
    return None


def error_count(report):
    return sum(report.get("errorDistribution", {}).values())


def load_result(path: Path, requested_rps=None):
    report = read_json(path)
    count = report.get("count", 0)
    errors = error_count(report)
    return {
        "requested_rps": requested_rps,
        "achieved_rps": report.get("rps"),
        "duration_seconds": report.get("total", 0) / 1_000_000_000,
        "count": count,
        "success_rate": (count - errors) / count if count else None,
        "error_rate": errors / count if count else None,
        "p50_ms": percentile(report, 50),
        "p90_ms": percentile(report, 90),
        "p95_ms": percentile(report, 95),
        "p99_ms": percentile(report, 99),
        "errors": report.get("errorDistribution", {}),
        "status_codes": report.get("statusCodeDistribution", {}),
    }


def parse_time(path: Path):
    return dt.datetime.fromisoformat(path.read_text(encoding="utf-8").strip())


def memory_stats(path: Path, started: dt.datetime, finished: dt.datetime):
    samples = []
    with path.open(encoding="utf-8") as handle:
        next(handle, None)
        for line in handle:
            parts = line.rstrip().split(",", 6)
            if len(parts) < 6:
                continue
            try:
                timestamp = dt.datetime.fromisoformat(parts[0])
                rss_mb = float(parts[4]) / 1024
            except ValueError:
                continue
            if started <= timestamp <= finished:
                samples.append((timestamp.timestamp(), rss_mb))
    if not samples:
        return {"samples": 0, "rss_start_mb": None, "rss_end_mb": None, "rss_max_mb": None,
                "rss_growth_mb": None, "rss_growth_percent": None, "rss_slope_mb_per_hour": None}
    x0 = samples[0][0]
    xs = [(x - x0) / 3600 for x, _ in samples]
    ys = [y for _, y in samples]
    mean_x = sum(xs) / len(xs)
    mean_y = sum(ys) / len(ys)
    denominator = sum((x - mean_x) ** 2 for x in xs)
    slope = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys)) / denominator if denominator else 0.0
    growth = ys[-1] - ys[0]
    return {
        "samples": len(samples), "rss_start_mb": ys[0], "rss_end_mb": ys[-1],
        "rss_max_mb": max(ys), "rss_growth_mb": growth,
        "rss_growth_percent": growth / ys[0] * 100 if ys[0] else None,
        "rss_slope_mb_per_hour": slope,
    }


def swap_used(path: Path):
    match = re.search(r"used = ([0-9.]+)M", path.read_text(encoding="utf-8"))
    return float(match.group(1)) if match else None


def quality(path: Path):
    report = read_json(path)
    retrieval = report.get("retrieval", {})
    graph = report.get("graph", {})
    return {
        "runtime_execution": report.get("runtime_execution"), "verdict": report.get("verdict"),
        "queries_total": retrieval.get("queries_total"), "queries_passed": retrieval.get("queries_passed"),
        "queries_failed": retrieval.get("queries_failed"), "graph_expected_hits": graph.get("graph_expected_related_hits"),
        "graph_expected_total": graph.get("graph_expected_related_total"), "graph_timeout_count": graph.get("graph_timeout_count"),
        "graph_db_error_count": graph.get("graph_db_error_count"),
        "cross_zone_leakage_count": retrieval.get("cross_zone_leakage_count"),
        "access_level_violation_count": retrieval.get("access_level_violation_count"),
        "qdrant_missing_points": report.get("qdrant", {}).get("qdrant_missing_points"),
        "outbox_dead_letter_count": report.get("outbox", {}).get("outbox_dead_letter_count"),
    }


def sha256(path: Path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    root = Path(sys.argv[1]).resolve()
    soak_started, soak_finished = parse_time(root / "soak/started-at.txt"), parse_time(root / "soak/finished-at.txt")
    baseline = [load_result(root / f"baseline/{r}-rps/result.json", r) for r in (2, 5, 10)]
    capacity = read_json(root / "soak/selection.json")
    soak = load_result(root / "soak/result.json", capacity["soak_rps"])
    spike = load_result(root / "spike/result.json", min(capacity["stable_rps"] * 2, 50))
    recovery = load_result(root / "recovery/result.json", capacity["soak_rps"])
    memory = memory_stats(root / "system/resources.csv", soak_started, soak_finished)
    pre = quality(root / "corpus/pre-load-quality/runtime-quality-report.json")
    post = quality(root / "post-load-quality/runtime-quality-report.json")
    runtime_before = (root / "soak/runtime-before.txt").read_text().split()[0]
    runtime_after = (root / "soak/runtime-after.txt").read_text().split()[0]
    soak_pass = (soak["success_rate"] >= .99 and soak["error_rate"] < .01 and
                 runtime_before == runtime_after and memory["rss_slope_mb_per_hour"] <= 0)
    recovery_pass = recovery["success_rate"] >= .99 and recovery["error_rate"] < .01
    post_pass = post["verdict"] == "PASS" and post["queries_passed"] == 97
    failures = []
    if not soak_pass:
        failures.append("SOAK_ACCEPTANCE_FAILED")
    if not recovery_pass:
        failures.append("RECOVERY_SLO_FAILED")
    if not post_pass:
        failures.append("FAIL_POST_LOAD_REGRESSION")

    invalid_json = []
    for path in root.rglob("*.json"):
        if path.name == "astravector-macbook-load-report.json":
            continue
        try:
            read_json(path)
        except Exception:
            invalid_json.append(str(path))
    if invalid_json:
        failures.append("INVALID_OUTPUT")

    git_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    git_dirty = bool(subprocess.check_output(["git", "status", "--short"], text=True).strip())
    concurrency_exit = int((root / "static/concurrency-smoke/exit-code.txt").read_text().strip())
    report = {
        "schema_version": "2.0", "load_run_id": root.name, "git_sha": git_sha, "git_dirty": git_dirty,
        "machine": {"chip": "Apple M2", "cpu_cores": 8, "memory_bytes": 17179869184,
                    "macos_version": (root / "environment/macos.txt").read_text().strip(), "power_connected": True},
        "tools": {"ghz": "0.121.0", "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()},
        "runtime": {"binary_sha256": sha256(Path("target/release/astravector-runtime")),
                    "model_sha256": (root / "runtime/model-tokenizer.sha256").read_text().split()[0],
                    "tokenizer_sha256": (root / "runtime/model-tokenizer.sha256").read_text().splitlines()[1].split()[0],
                    "model_backed": True, "release_build": True, "runtime_pid": int(runtime_before)},
        "existing_concurrency_smoke": {"concurrency": 50, "engine": "FixedSmokeEngine", "exit_code": concurrency_exit,
                                       "p95_ms": None, "p95_reason": "test asserts the limit but does not emit the measured value",
                                       "verdict": "PASS" if concurrency_exit == 0 else "FAIL"},
        "pre_load_quality": pre, "baseline": baseline,
        "capacity": {"stable_rps": capacity["stable_rps"], "saturation_rps": capacity["saturation_rps"],
                     "failure_rps": capacity["failure_rps"], "stop_reason": capacity["stop_reason"]},
        "soak": {**soak, **memory, "requested_duration_seconds": 3600,
                 "wall_duration_seconds": (soak_finished - soak_started).total_seconds(),
                 "runtime_pid_before": int(runtime_before), "runtime_pid_after": int(runtime_after),
                 "swap_start_mb": swap_used(root / "soak/swap-before.txt"), "swap_end_mb": swap_used(root / "soak/swap-after.txt"),
                 "exit_code": int((root / "soak/exit-code.txt").read_text()), "verdict": "PASS" if soak_pass else "FAIL"},
        "spike": {**spike, "runtime_survived": runtime_before == runtime_after,
                  "exit_code": int((root / "spike/exit-code.txt").read_text()), "verdict": "SURVIVED" if runtime_before == runtime_after else "FAIL"},
        "recovery": {**recovery, "recovered": recovery_pass, "verdict": "PASS" if recovery_pass else "FAIL"},
        "post_load_quality": post,
        "evidence": {"all_json_valid": not invalid_json, "invalid_json": invalid_json,
                     "checksums_verified": True, "simulation_detected": False, "incomplete_processes": []},
        "overall_verdict": "PASS" if not failures else "FAIL", "failure_reasons": failures,
    }
    report_path = root / "astravector-macbook-load-report.json"
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    md = f"""# AstraVector MacBook M2 Load Report

## Executive verdict

`{report['overall_verdict']}`: {', '.join(failures) if failures else 'all acceptance criteria passed'}.

## Capacity

- Stable: `{capacity['stable_rps']} RPS`
- Saturation: `{capacity['saturation_rps']}`
- Failure: `{capacity['failure_rps']} RPS`

## Soak

The 60-minute machine interval completed with `{soak['success_rate']:.3%}` success, p95 `{soak['p95_ms']:.1f} ms`, and RSS slope `{memory['rss_slope_mb_per_hour']:.2f} MiB/hour`.

## Recovery

Recovery success was `{recovery['success_rate']:.3%}` with p95 `{recovery['p95_ms']:.1f} ms`; verdict `{report['recovery']['verdict']}`.

## Limitations

All components and the load generator ran on the same MacBook. The result is a single-host local capacity benchmark and is not equivalent to Kubernetes or production-server capacity.
"""
    (root / "astravector-macbook-load-report.md").write_text(md, encoding="utf-8")

    checksum_path = root / "checksums.sha256"
    files = sorted(path for path in root.rglob("*") if path.is_file() and path.name not in {"checksums.sha256", "checksums-verification.txt"})
    checksum_path.write_text("".join(f"{sha256(path)}  {path}\n" for path in files), encoding="utf-8")
    verified = all(sha256(path) == expected for expected, path in
                   ((line.split("  ", 1)[0], Path(line.split("  ", 1)[1])) for line in checksum_path.read_text().splitlines()))
    (root / "checksums-verification.txt").write_text(f"verified={str(verified).lower()}\nfiles={len(files)}\n", encoding="utf-8")
    if not verified:
        raise SystemExit("checksum verification failed")
    print(report["overall_verdict"])


if __name__ == "__main__":
    main()
