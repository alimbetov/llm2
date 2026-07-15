#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


STAGES = [
    "DENSE_RETRIEVAL", "SPARSE_RETRIEVAL", "LEXICAL_RETRIEVAL",
    "FUSION_ADMISSION", "POST_FUSION_DEDUP", "POSTGRES_HYDRATION",
    "PRE_MMR_NO_ANSWER", "GRAPH_SEED", "GRAPH_EXPANSION", "GRAPH_MERGE",
    "MMR_INPUT", "MMR_SELECTED", "POST_MMR_NO_ANSWER", "TOKEN_BUDGET",
    "VISIBILITY_RECHECK", "FINAL_SELECTION",
]


def jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--queries", type=Path,
                        default=Path("benchmarks/quality/queries/fix480-validation.jsonl"))
    parser.add_argument("--candidate-limit", type=int, default=50)
    parser.add_argument("--graph-related-limit", type=int, default=5)
    args = parser.parse_args()
    queries = {item["id"]: item for item in jsonl(args.queries)}
    records = []
    failures = []

    for trace_path in sorted((args.evidence_dir / "ranking-traces").glob("*.json")):
        trace = json.loads(trace_path.read_text(encoding="utf-8"))
        query_id = trace["query_id"]
        query = queries[query_id]
        expected = query["expected"]
        targets = [("PRIMARY_EVIDENCE", block)
                   for block in expected.get("must_contain_block_ids", [])]
        targets += [("RELATED_GRAPH_EVIDENCE", block)
                    for block in expected.get("expected_related_block_ids", [])]
        for target_type, block_id in targets:
            matches = [candidate for candidate in trace["candidates"]
                       if candidate.get("identity", {}).get("source_block_id") == block_id]
            if not matches:
                failures.append({"code": "TARGET_NEVER_OBSERVED", "query_id": query_id,
                                 "source_block_id": block_id})
                records.append({"query_id": query_id, "target": {"type": target_type,
                                "source_block_id": block_id}, "trace_completeness": False,
                                "verdict": "FAIL", "failure_code": "TARGET_NEVER_OBSERVED"})
                continue
            identities = {json.dumps(item["identity"], sort_keys=True) for item in matches}
            if len(identities) > 1:
                failures.append({"code": "TARGET_IDENTITY_AMBIGUOUS", "query_id": query_id,
                                 "source_block_id": block_id})
                continue
            candidate = matches[0]
            by_stage = {stage["stage"]: stage for stage in candidate["stages"]}
            present = [stage for stage in STAGES if by_stage.get(stage, {}).get("present")]
            last = present[-1] if present else None
            first_loss = None
            if last:
                for stage in STAGES[STAGES.index(last) + 1:]:
                    value = by_stage.get(stage)
                    if value is not None and not value.get("present"):
                        first_loss = value
                        break
            final_present = by_stage.get("FINAL_SELECTION", {}).get("present", False)
            if final_present:
                verdict = "PASS"
                failure_code = None
            elif trace.get("truncated"):
                verdict = "FAIL"
                failure_code = "TARGET_TRACE_TRUNCATED"
            elif first_loss is None:
                verdict = "FAIL"
                failure_code = "TARGET_FIRST_LOSS_UNKNOWN"
            elif first_loss.get("drop_reason") in (None, "DROP_REASON_UNSPECIFIED"):
                verdict = "FAIL"
                failure_code = "TARGET_DROP_REASON_MISSING"
            else:
                verdict = "FAIL"
                failure_code = None
            if failure_code:
                failures.append({"code": failure_code, "query_id": query_id,
                                 "source_block_id": block_id})
            last_value = by_stage.get(last, {}) if last else {}
            loss_stage = first_loss.get("stage") if first_loss else None
            if loss_stage in {"DENSE_RETRIEVAL", "SPARSE_RETRIEVAL", "LEXICAL_RETRIEVAL",
                              "FUSION_ADMISSION", "POST_FUSION_DEDUP", "POSTGRES_HYDRATION",
                              "PRE_MMR_NO_ANSWER", "GRAPH_SEED", "MMR_INPUT"}:
                configured_limit = args.candidate_limit
            elif loss_stage in {"GRAPH_EXPANSION", "GRAPH_MERGE"}:
                configured_limit = args.graph_related_limit
            else:
                configured_limit = int(expected.get("max_contexts_count", 10))
            records.append({
                "query_id": query_id,
                "target": {"type": target_type, "source_block_id": block_id,
                           "identity": candidate["identity"]},
                "last_present_stage": last,
                "last_present_rank": last_value.get("rank"),
                "last_present_scores": {key: last_value.get(key) for key in (
                    "dense_score", "sparse_score", "lexical_score", "fusion_score",
                    "graph_score", "mmr_relevance", "final_score")},
                "first_loss_stage": loss_stage,
                "drop_reason": first_loss.get("drop_reason") if first_loss else None,
                "candidate_pool_size": trace.get("total_candidates_seen"),
                "configured_limit": configured_limit,
                "trace_completeness": not trace.get("truncated", False),
                "verdict": verdict,
                "failure_code": failure_code,
            })

    report = {"schema_version": 1, "records": records, "failures": failures,
              "ranking_trace_complete": not failures,
              "targets_traced": len(records),
              "targets_with_first_loss": sum(item.get("first_loss_stage") is not None for item in records),
              "targets_with_unknown_loss": sum(item.get("failure_code") == "TARGET_FIRST_LOSS_UNKNOWN" for item in records)}
    (args.evidence_dir / "ranking-first-loss-report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    lines = ["# Ranking First-Loss Report", "", f"- trace complete: `{report['ranking_trace_complete']}`",
             f"- targets traced: `{report['targets_traced']}`", ""]
    for item in records:
        lines.append(f"- `{item['query_id']}` / `{item['target']['source_block_id']}`: "
                     f"last `{item.get('last_present_stage')}`, loss `{item.get('first_loss_stage')}`, "
                     f"reason `{item.get('drop_reason')}`, verdict `{item['verdict']}`")
    (args.evidence_dir / "ranking-first-loss-report.md").write_text(
        "\n".join(lines) + "\n", encoding="utf-8")
    if failures:
        raise SystemExit("ranking first-loss report contains incomplete target traces")


if __name__ == "__main__":
    main()
