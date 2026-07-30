#!/usr/bin/env python3
"""Fail-closed retrieval freeze guard for FIX487 Phase A."""

from __future__ import annotations

import argparse
import fnmatch
import json
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable


BASE_SHA = "4843ce624724eceb865f64c6282d2841a69fcb88"

REQUIRED_MANIFEST_FILES = (
    "docs/fix487/phase-a-retrieval-freeze/TECHNICAL_SPECIFICATION.md",
    "docs/fix487/phase-a-retrieval-freeze/RETRIEVAL_FREEZE_MANIFEST.md",
    "docs/fix487/phase-a-retrieval-freeze/RESULT.md",
)

ALLOWED_PATH_PATTERNS = (
    "docs/fix487/**",
    "scripts/fix487_*.py",
    "scripts/fix487-*.sh",
    "tests/test_fix487_*.py",
    "config/application-fix487*.yaml",
    "docker-compose.fix487*.yml",
)

PROTECTED_FIXTURE_PATTERNS = (
    "benchmarks/**/fixtures/**",
    "benchmarks/**/corpus/**",
    "benchmarks/**/corpora/**",
    "benchmarks/hierarchical/fix486/**",
    "benchmarks/hierarchical/fix486g-supplemental/**",
)

PROTECTED_QREL_PATTERNS = (
    "benchmarks/**/qrels/**",
    "benchmarks/**/queries/**",
    "benchmarks/**/profiles/**",
    "benchmarks/**/judgments/**",
    "docs/fix486/**",
)

PROTECTED_CONFIG_PATTERNS = (
    "config/application.yaml",
    "config/application-fix486*.yaml",
    "src/config/mod.rs",
)

PROTECTED_RETRIEVAL_SOURCE_PATTERNS = (
    "src/grpc/mod.rs",
    "src/graph/mod.rs",
    "src/retrieval/**",
    "src/chunking/**",
    "src/qdrant/mod.rs",
)

MAKEFILE_ALLOWED_LINES = (
    "verify-fix487a-retrieval-freeze",
    "verify-fix487b-contracts",
    "verify-fix487b-mixed-load-pilot",
    "verify-fix487b-existing-evidence",
    "fix487b-cleanup",
    "verify-fix487bc-capacity-contracts",
    "verify-fix487bc-capacity-campaign",
    "verify-fix487bc-existing-capacity-evidence",
    "verify-fix487c-soak-contracts",
    "verify-fix487c-soak-60m",
    "verify-fix487c-existing-soak-evidence",
    "fix487bc-cleanup",
    "fix487_retrieval_freeze_guard.py",
    "test_fix487_retrieval_freeze_guard.py",
    "fix487b_dataset.py",
    "fix487b_mixed_load.py",
    "fix487b_evidence.py",
    "fix487b_audit.py",
    "fix487b-mixed-load-pilot.sh",
    "test_fix487b_",
    "ASTRAVECTOR_FIX487B_EXECUTE_PILOT",
    "FIX487B_BLOCKED=EXPLICIT_PILOT_OPT_IN_REQUIRED",
    "fix487bc_capacity_campaign.py",
    "fix487bc_capacity_evidence.py",
    "fix487bc-capacity-campaign.sh",
    "fix487c_soak.py",
    "fix487c-soak-60m.sh",
    "test_fix487bc_",
    "test_fix487c_",
    "ASTRAVECTOR_FIX487BC_EXECUTE_CAPACITY",
    "ASTRAVECTOR_FIX487C_EXECUTE_SOAK",
    "FIX487BC_BLOCKED=EXPLICIT_CAPACITY_OPT_IN_REQUIRED",
    "FIX487C_BLOCKED=EXPLICIT_SOAK_OPT_IN_REQUIRED",
)

OPERATIONAL_TOKENS = (
    "metric",
    "metrics",
    "tracing",
    "trace",
    "debug",
    "warn",
    "info",
    "timeout",
    "deadline",
    "cancel",
    "cancellation",
    "concurrency",
    "semaphore",
    "cleanup",
    "clean-up",
    "shutdown",
)

PROTECTED_SYMBOL_TOKENS = (
    "apply_pre_mmr_no_answer_filter",
    "apply_segmented_pre_mmr_no_answer_filter",
    "apply_post_mmr_technical_no_answer_filter",
    "final_no_answer_should_trigger",
    "restore_graph_supported_direct_survivors",
    "graph_seed_candidate_passes",
    "query_has_graph_recovery_intent",
    "graph_survivor_fallback_intent",
    "graph_expanded_relation_evidence_passes",
    "select_results_with_strategy_aware_mmr",
    "apply_mmr_rerank",
    "select_graph_seed_candidates",
    "compare_graph_seed_candidates",
    "stable_result_rank",
    "violates_query_exclusion_terms",
    "is_negative_mention_evidence",
    "strong_technical_query_tokens",
    "no_answer_candidate_passes",
    "apply_no_answer_exact_technical_boost",
    "hybrid_fusion_method",
    "hybrid_dense_weight",
    "hybrid_sparse_weight",
    "rrf_k",
    "segment_rrf_k",
    "no_answer",
    "mmr_lambda",
    "graph_rag",
    "chunking",
    "tokenizer_safety_margin",
    "dense_candidate_limit",
    "sparse_candidate_limit",
    "lexical_candidate_limit",
)


@dataclass
class Violation:
    path: str
    category: str
    reason: str


@dataclass
class GuardResult:
    baseline_sha: str
    head_sha: str
    changed_files: list[str]
    retrieval_freeze_manifest_complete: bool
    protected_config_changed: int = 0
    protected_fixture_changed: int = 0
    protected_qrel_changed: int = 0
    unapproved_retrieval_symbol_changed: int = 0
    violations: list[Violation] = field(default_factory=list)

    @property
    def status(self) -> str:
        if (
            self.retrieval_freeze_manifest_complete
            and self.protected_config_changed == 0
            and self.protected_fixture_changed == 0
            and self.protected_qrel_changed == 0
            and self.unapproved_retrieval_symbol_changed == 0
        ):
            return "PASS"
        return "FAIL"

    def to_json(self) -> str:
        payload = {
            "baseline_sha": self.baseline_sha,
            "head_sha": self.head_sha,
            "changed_files": self.changed_files,
            "retrieval_freeze_manifest_complete": self.retrieval_freeze_manifest_complete,
            "protected_config_changed": self.protected_config_changed,
            "protected_fixture_changed": self.protected_fixture_changed,
            "protected_qrel_changed": self.protected_qrel_changed,
            "unapproved_retrieval_symbol_changed": self.unapproved_retrieval_symbol_changed,
            "status": self.status,
            "violations": [v.__dict__ for v in self.violations],
        }
        return json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True)


def matches_any(path: str, patterns: Iterable[str]) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def run_git(repo: Path, args: list[str], check: bool = True) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=repo,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and completed.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed: {completed.stderr.strip() or completed.stdout.strip()}"
        )
    return completed.stdout


def changed_files(repo: Path, base_sha: str) -> list[str]:
    names: set[str] = set()
    for args in (
        ["diff", "--name-only", f"{base_sha}...HEAD"],
        ["diff", "--name-only"],
        ["diff", "--cached", "--name-only"],
        ["ls-files", "--others", "--exclude-standard"],
    ):
        output = run_git(repo, args)
        names.update(line.strip() for line in output.splitlines() if line.strip())
    return sorted(names)


def diff_for_path(repo: Path, base_sha: str, path: str) -> str:
    parts = [
        run_git(repo, ["diff", "--unified=0", f"{base_sha}...HEAD", "--", path], check=False),
        run_git(repo, ["diff", "--unified=0", "--", path], check=False),
        run_git(repo, ["diff", "--cached", "--unified=0", "--", path], check=False),
    ]
    return "\n".join(part for part in parts if part)


def changed_lines(diff_text: str) -> list[str]:
    lines: list[str] = []
    for line in diff_text.splitlines():
        if line.startswith(("+++", "---", "@@")):
            continue
        if line.startswith(("+", "-")):
            lines.append(line[1:].strip())
    return lines


def makefile_change_is_allowed(diff_text: str) -> bool:
    lines = changed_lines(diff_text)
    return bool(lines) and all(
        not line or any(token in line for token in MAKEFILE_ALLOWED_LINES) for line in lines
    )


def retrieval_source_change_is_allowed(diff_text: str) -> bool:
    lines = changed_lines(diff_text)
    if not lines:
        return True
    lowered = "\n".join(lines).lower()
    if any(token.lower() in lowered for token in PROTECTED_SYMBOL_TOKENS):
        return False
    return all(
        (not line)
        or line.startswith(("//", "#", "use "))
        or any(token in line.lower() for token in OPERATIONAL_TOKENS)
        for line in lines
    )


def manifest_complete(repo: Path) -> bool:
    return all((repo / path).is_file() for path in REQUIRED_MANIFEST_FILES)


def classify_path(repo: Path, base_sha: str, path: str) -> list[Violation]:
    if matches_any(path, ALLOWED_PATH_PATTERNS):
        return []

    diff_text = diff_for_path(repo, base_sha, path)
    if path == "Makefile" and makefile_change_is_allowed(diff_text):
        return []

    violations: list[Violation] = []
    if matches_any(path, PROTECTED_CONFIG_PATTERNS):
        violations.append(Violation(path, "protected_config_changed", "retrieval configuration changed"))
    if matches_any(path, PROTECTED_FIXTURE_PATTERNS):
        violations.append(Violation(path, "protected_fixture_changed", "frozen fixture/corpus changed"))
    if matches_any(path, PROTECTED_QREL_PATTERNS):
        violations.append(Violation(path, "protected_qrel_changed", "frozen query/qrel/profile changed"))
    if matches_any(path, PROTECTED_RETRIEVAL_SOURCE_PATTERNS) and not retrieval_source_change_is_allowed(
        diff_text
    ):
        violations.append(
            Violation(
                path,
                "unapproved_retrieval_symbol_changed",
                "protected retrieval symbol or semantic hunk changed",
            )
        )
    elif matches_any(path, PROTECTED_RETRIEVAL_SOURCE_PATTERNS) and not changed_lines(diff_text):
        return violations
    elif matches_any(path, PROTECTED_RETRIEVAL_SOURCE_PATTERNS):
        return violations

    return violations


def evaluate(repo: Path, base_sha: str) -> GuardResult:
    run_git(repo, ["cat-file", "-e", f"{base_sha}^{{commit}}"])
    head_sha = run_git(repo, ["rev-parse", "HEAD"]).strip()
    files = changed_files(repo, base_sha)
    result = GuardResult(
        baseline_sha=base_sha,
        head_sha=head_sha,
        changed_files=files,
        retrieval_freeze_manifest_complete=manifest_complete(repo),
    )
    if not result.retrieval_freeze_manifest_complete:
        result.violations.append(
            Violation(
                "docs/fix487/phase-a-retrieval-freeze",
                "retrieval_freeze_manifest_complete",
                "required Phase A manifest files are missing",
            )
        )

    for path in files:
        for violation in classify_path(repo, base_sha, path):
            result.violations.append(violation)
            if violation.category == "protected_config_changed":
                result.protected_config_changed += 1
            elif violation.category == "protected_fixture_changed":
                result.protected_fixture_changed += 1
            elif violation.category == "protected_qrel_changed":
                result.protected_qrel_changed += 1
            elif violation.category == "unapproved_retrieval_symbol_changed":
                result.unapproved_retrieval_symbol_changed += 1
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="repository root")
    parser.add_argument("--base-sha", default=BASE_SHA, help="retrieval freeze baseline SHA")
    args = parser.parse_args()

    try:
        result = evaluate(Path(args.repo).resolve(), args.base_sha)
    except Exception as exc:  # pragma: no cover - defensive CLI boundary
        print(json.dumps({"status": "FAIL", "error": str(exc)}, indent=2), file=sys.stderr)
        return 2

    print(result.to_json())
    return 0 if result.status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
