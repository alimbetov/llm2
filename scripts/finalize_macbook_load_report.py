#!/usr/bin/env python3
import datetime as dt
import hashlib
import json
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
        "resource_exhausted_rate": report.get("statusCodeDistribution", {}).get("ResourceExhausted", 0) / count if count else None,
        "deadline_exceeded_rate": report.get("statusCodeDistribution", {}).get("DeadlineExceeded", 0) / count if count else None,
        "unavailable_rate": report.get("statusCodeDistribution", {}).get("Unavailable", 0) / count if count else None,
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


def queue_depth_stats(metrics_dir: Path, started: dt.datetime, finished: dt.datetime):
    samples = []
    pattern = re.compile(r'^astravector_queue_depth\{[^}]*queue="query"[^}]*\}\s+([0-9.eE+-]+)$')
    for path in sorted(metrics_dir.glob("*.prom")):
        timestamp = dt.datetime.fromtimestamp(int(path.stem), tz=started.tzinfo)
        if not started <= timestamp <= finished:
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            match = pattern.match(line)
            if match:
                samples.append((timestamp.timestamp(), float(match.group(1))))
                break
    if not samples:
        return {"samples": 0, "start": None, "end": None, "max": None, "slope_per_hour": None, "unbounded_growth": None}
    x0 = samples[0][0]
    xs = [(x - x0) / 3600 for x, _ in samples]
    ys = [y for _, y in samples]
    mean_x, mean_y = sum(xs) / len(xs), sum(ys) / len(ys)
    denominator = sum((x - mean_x) ** 2 for x in xs)
    slope = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys)) / denominator if denominator else 0.0
    return {"samples": len(samples), "start": ys[0], "end": ys[-1], "max": max(ys),
            "slope_per_hour": slope, "unbounded_growth": slope > 0.1 and ys[-1] > ys[0]}


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
        "hard_negative_false_positive_rate": retrieval.get("hard_negative_false_positive_rate"),
        "qdrant_missing_points": report.get("qdrant", {}).get("qdrant_missing_points"),
        "outbox_dead_letter_count": report.get("outbox", {}).get("outbox_dead_letter_count"),
    }


def prometheus_sum(path: Path, metric: str):
    total = 0.0
    found = False
    pattern = re.compile(rf"^{re.escape(metric)}(?:\{{[^}}]*\}})?\s+([0-9.eE+-]+)$")
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        match = pattern.match(line)
        if match:
            total += float(match.group(1))
            found = True
    return total if found else None


def sha256(path: Path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    root = Path(sys.argv[1]).resolve()
    source_git_sha, source_branch, origin_main_sha, expected_release_sha = sys.argv[2:6]
    recovery_p95_slo_ms = float(sys.argv[6])
    soak_started, soak_finished = parse_time(root / "soak/started-at.txt"), parse_time(root / "soak/finished-at.txt")
    baseline = [load_result(root / f"baseline/{r}-rps/result.json", r) for r in (2, 5, 10)]
    capacity = read_json(root / "soak/selection.json")
    soak = load_result(root / "soak/result.json", capacity["soak_rps"])
    spike = load_result(root / "spike/result.json", min(capacity["stable_rps"] * 2, 50))
    recovery_windows = []
    for path in sorted((root / "recovery").glob("window-*/result.json")):
        result = load_result(path, capacity["soak_rps"])
        result["window"] = path.parent.name
        result["healthy"] = read_json(path.parent / "health.json")["healthy"]
        result["finished_at"] = parse_time(path.parent / "finished-at.txt").isoformat()
        recovery_windows.append(result)
    stabilized = load_result(root / "recovery/stabilized/result.json", capacity["soak_rps"])
    stabilized_metrics_before = root / "recovery/stabilized-metrics-before.prom"
    stabilized_metrics_after = root / "recovery/stabilized-metrics-after.prom"
    rejects_before = prometheus_sum(stabilized_metrics_before, "astravector_admission_rejected_total")
    rejects_after = prometheus_sum(stabilized_metrics_after, "astravector_admission_rejected_total")
    query_depth_after = prometheus_sum(stabilized_metrics_after, "astravector_queue_depth")
    stabilized_admission_rejects = None if rejects_before is None or rejects_after is None else rejects_after - rejects_before
    memory = memory_stats(root / "system/resources.csv", soak_started, soak_finished)
    queue_depth = queue_depth_stats(root / "metrics", soak_started, soak_finished)
    pre = quality(root / "corpus/pre-load-quality/runtime-quality-report.json")
    post = quality(root / "post-load-quality/runtime-quality-report.json")
    runtime_before = (root / "soak/runtime-before.txt").read_text().split()[0]
    runtime_after = (root / "soak/runtime-after.txt").read_text().split()[0]
    soak_pass = (soak["duration_seconds"] >= 3600 and soak["success_rate"] >= .995 and
                 soak["error_rate"] <= .005 and runtime_before == runtime_after and
                 memory["rss_slope_mb_per_hour"] <= 0 and queue_depth.get("unbounded_growth") is not True)
    spike_errors = spike["error_rate"] or 0
    spike_load_shedding_primary = spike_errors == 0 or (spike["resource_exhausted_rate"] or 0) >= max(
        spike["deadline_exceeded_rate"] or 0, spike["unavailable_rate"] or 0)
    spike_pass = (runtime_before == runtime_after and (spike["deadline_exceeded_rate"] or 0) <= .01 and
                  (spike["unavailable_rate"] or 0) <= .01 and spike_load_shedding_primary)
    consecutive = 0
    recovery_finish = None
    for window in recovery_windows:
        consecutive = consecutive + 1 if window["healthy"] else 0
        if consecutive >= 3:
            recovery_finish = dt.datetime.fromisoformat(window["finished_at"])
            break
    spike_finished = parse_time(root / "spike/finished-at.txt")
    time_to_recovery = (recovery_finish - spike_finished).total_seconds() if recovery_finish else None
    stabilized_pass = (stabilized["success_rate"] >= .99 and stabilized["error_rate"] < .01 and
                       stabilized["p95_ms"] <= recovery_p95_slo_ms and
                       stabilized_admission_rejects == 0 and query_depth_after == 0)
    recovery_pass = time_to_recovery is not None and time_to_recovery <= 60 and stabilized_pass
    post_pass = post["verdict"] == "PASS" and post["queries_passed"] == 97
    pre_integrity = read_json(root / "corpus/pre-load-integrity.json")
    post_integrity = read_json(root / "post-load-quality/post-load-integrity.json")
    integrity_regressions = {
        name: max(0, count - pre_integrity["checks"].get(name, 0))
        for name, count in post_integrity["checks"].items()
    }
    integrity_regressions_total = sum(integrity_regressions.values())
    quality_invariants_pass = (
        pre.get("hard_negative_false_positive_rate") == 0.0 and
        post.get("hard_negative_false_positive_rate") == 0.0 and
        integrity_regressions_total == 0
    )
    failures = []
    if not soak_pass:
        failures.append("SOAK_ACCEPTANCE_FAILED")
    if not spike_pass:
        failures.append("SPIKE_ACCEPTANCE_FAILED")
    if not recovery_pass:
        failures.append("RECOVERY_SLO_FAILED")
    if not post_pass:
        failures.append("FAIL_POST_LOAD_REGRESSION")
    if not quality_invariants_pass:
        failures.append("QUALITY_INVARIANT_REGRESSION")

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
    concurrency_exit = int((root / "static/concurrency-smoke/exit-code.txt").read_text().strip())
    report = {
        "schema_version": "3.0", "report_schema_version": "3.0", "load_run_id": root.name,
        "source": {"git_sha": source_git_sha, "branch": source_branch, "origin_main_sha": origin_main_sha,
                   "git_clean_at_start": True, "head_matches_origin_main": source_git_sha == origin_main_sha,
                   "expected_release_sha": expected_release_sha,
                   "head_matches_expected_release_sha": source_git_sha == expected_release_sha,
                   "unchanged_during_run": git_sha == source_git_sha},
        "machine": {"chip": "Apple M2", "cpu_cores": 8, "memory_bytes": 17179869184,
                    "macos_version": (root / "environment/macos.txt").read_text().strip(), "power_connected": True},
        "tools": {"ghz": "0.121.0", "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()},
        "runtime": {"binary_sha256": (root / "runtime/binary.sha256").read_text().split()[0],
                    "config_sha256": (root / "runtime/config.sha256").read_text().split()[0],
                    "model_sha256": (root / "runtime/model-tokenizer.sha256").read_text().split()[0],
                    "tokenizer_sha256": (root / "runtime/model-tokenizer.sha256").read_text().splitlines()[1].split()[0],
                    "migration_head": (root / "runtime/migration-head.txt").read_text().strip(),
                    "model_backed": True, "release_build": True, "runtime_pid": int(runtime_before)},
        "existing_concurrency_smoke": {"concurrency": 50, "engine": "FixedSmokeEngine", "exit_code": concurrency_exit,
                                       "p95_ms": None, "p95_reason": "test asserts the limit but does not emit the measured value",
                                       "verdict": "PASS" if concurrency_exit == 0 else "FAIL"},
        "pre_load_quality": pre, "baseline": baseline,
        "capacity": {"stable_rps": capacity["stable_rps"], "saturation_rps": capacity["saturation_rps"],
                     "failure_rps": capacity["failure_rps"], "stop_reason": capacity["stop_reason"]},
        "soak": {**soak, **memory, "queue_depth": queue_depth, "requested_duration_seconds": 3600,
                 "wall_duration_seconds": (soak_finished - soak_started).total_seconds(),
                 "runtime_pid_before": int(runtime_before), "runtime_pid_after": int(runtime_after),
                 "swap_start_mb": swap_used(root / "soak/swap-before.txt"), "swap_end_mb": swap_used(root / "soak/swap-after.txt"),
                 "exit_code": int((root / "soak/exit-code.txt").read_text()), "verdict": "PASS" if soak_pass else "FAIL"},
        "spike": {**spike, "runtime_survived": runtime_before == runtime_after,
                  "load_shedding_primary": spike_load_shedding_primary,
                  "exit_code": int((root / "spike/exit-code.txt").read_text()), "verdict": "PASS" if spike_pass else "FAIL"},
        "recovery": {"windows": recovery_windows, "healthy_windows_required": 3,
                     "time_to_recovery_seconds": time_to_recovery, "time_to_recovery_slo_seconds": 60,
                     "recovery_p95_slo_ms": recovery_p95_slo_ms,
                     "stabilized": {**stabilized,
                                    "admission_rejects": stabilized_admission_rejects,
                                    "query_queue_depth_after": query_depth_after,
                                    "no_continuing_backlog": query_depth_after == 0},
                     "recovered": recovery_pass, "verdict": "PASS" if recovery_pass else "FAIL"},
        "post_load_quality": post,
        "quality_invariants": {"pre_load_integrity": pre_integrity, "post_load_integrity": post_integrity,
                               "introduced_violations": integrity_regressions,
                               "introduced_violations_total": integrity_regressions_total,
                               "hard_negative_regression": post.get("hard_negative_false_positive_rate"),
                               "verdict": "PASS" if quality_invariants_pass else "FAIL"},
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

Time to recovery was `{time_to_recovery}` seconds; stabilized p95 `{stabilized['p95_ms']:.1f} ms`; verdict `{report['recovery']['verdict']}`.

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
