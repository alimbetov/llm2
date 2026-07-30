#!/usr/bin/env python3
"""Build FIX486G candidate lifecycle first-loss reports from focused observations."""
from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

STAGE_BUCKETS = {
    "DENSE_RETRIEVAL": "RETRIEVAL_OR_HYDRATION",
    "SPARSE_RETRIEVAL": "RETRIEVAL_OR_HYDRATION",
    "LEXICAL_RETRIEVAL": "RETRIEVAL_OR_HYDRATION",
    "FUSION_ADMISSION": "RETRIEVAL_OR_HYDRATION",
    "POST_FUSION_DEDUP": "POST_DEDUP",
    "POSTGRES_HYDRATION": "RETRIEVAL_OR_HYDRATION",
    "RETRIEVED": "RETRIEVAL_OR_HYDRATION",
    "PRE_MMR_NO_ANSWER": "PRE_NO_ANSWER",
    "PRE_NO_ANSWER": "PRE_NO_ANSWER",
    "POST_NO_ANSWER": "PRE_NO_ANSWER",
    "GRAPH_SEED": "GRAPH_SEED_ADMISSION",
    "GRAPH_SEED_ADMITTED": "GRAPH_SEED_ADMISSION",
    "GRAPH_EXPANSION": "GRAPH_EXPANSION",
    "GRAPH_EXPANDED": "GRAPH_EXPANSION",
    "GRAPH_MERGE": "POST_DEDUP",
    "POST_DEDUP": "POST_DEDUP",
    "MMR_INPUT": "PRE_MMR_BUDGET",
    "PRE_MMR": "PRE_MMR_BUDGET",
    "MMR_SELECTED": "MMR_SELECTION",
    "POST_MMR": "MMR_SELECTION",
    "POST_MMR_NO_ANSWER": "POST_MMR_FILTER",
    "TOKEN_BUDGET": "TOKEN_BUDGET",
    "VISIBILITY_RECHECK": "VISIBILITY",
    "FINAL_SELECTION": "FINAL_LIMIT",
    "FINAL": "FINAL_LIMIT",
}

NEXT_BUCKET_AFTER_LAST_PRESENT = {
    "RETRIEVAL_OR_HYDRATION": "PRE_NO_ANSWER",
    "PRE_NO_ANSWER": "GRAPH_SEED_ADMISSION",
    "GRAPH_SEED_ADMISSION": "GRAPH_EXPANSION",
    "GRAPH_EXPANSION": "POST_DEDUP",
    "POST_DEDUP": "PRE_MMR_BUDGET",
    "PRE_MMR_BUDGET": "MMR_SELECTION",
    "MMR_SELECTION": "POST_MMR_FILTER",
    "POST_MMR_FILTER": "TOKEN_BUDGET",
    "TOKEN_BUDGET": "VISIBILITY",
    "VISIBILITY": "FINAL_LIMIT",
}

FINAL_STAGES = {"FINAL", "FINAL_SELECTION"}


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(paths: list[Path]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for path in paths:
        with path.open("r", encoding="utf-8") as handle:
            for line_no, line in enumerate(handle, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError as exc:
                    raise SystemExit(f"{path}:{line_no}: invalid JSON: {exc}") from exc
                if not isinstance(row, dict):
                    raise SystemExit(f"{path}:{line_no}: expected JSON object")
                rows.append(row)
    return rows


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")


def camel_or_snake(obj: dict[str, Any], *names: str) -> Any:
    for name in names:
        if name in obj:
            return obj[name]
    return None


def load_bank(bank: Path) -> tuple[dict[str, Any], dict[str, str]]:
    profiles = read_json(bank / "qrels" / "qrel-profiles-v1.json")["profiles"]
    assignments: dict[str, str] = {}
    for row in read_jsonl([bank / "qrels" / "query-qrel-assignments-v1.jsonl"]):
        assignments[str(row["query_id"])] = str(row["qrel_profile"])
    return profiles, assignments


def resolved_profile(name: str, profiles: dict[str, Any]) -> dict[str, Any]:
    profile = dict(profiles[name])
    parent = profile.get("extends")
    if parent:
        base = resolved_profile(str(parent), profiles)
        merged = dict(base)
        merged.update(profile)
        profile = merged
    return profile


def response_contexts(row: dict[str, Any]) -> list[dict[str, Any]]:
    response = row.get("response")
    if not isinstance(response, dict):
        return []
    if row.get("entry_point") == "RetrieveContext":
        contexts = response.get("contexts") or response.get("results") or []
    else:
        contexts = response.get("results") or response.get("contexts") or []
    return contexts if isinstance(contexts, list) else []


def context_metadata(context: dict[str, Any]) -> dict[str, Any]:
    citation = context.get("citation")
    if isinstance(citation, dict) and isinstance(citation.get("metadata"), dict):
        return citation["metadata"]
    metadata = context.get("metadata")
    return metadata if isinstance(metadata, dict) else {}


def context_logical_parent(context: dict[str, Any]) -> str:
    metadata = context_metadata(context)
    return str(metadata.get("source_block_id") or metadata.get("fix486c_logical_block") or "")


def ranking_trace(row: dict[str, Any]) -> dict[str, Any]:
    diagnostics = row.get("response", {}).get("diagnostics", {})
    trace = camel_or_snake(diagnostics, "rankingTrace", "ranking_trace")
    return trace if isinstance(trace, dict) else {}


def trace_candidates(row: dict[str, Any]) -> list[dict[str, Any]]:
    trace = ranking_trace(row)
    candidates = trace.get("candidates")
    return candidates if isinstance(candidates, list) else []


def identity(candidate: dict[str, Any]) -> dict[str, Any]:
    value = candidate.get("identity")
    return value if isinstance(value, dict) else {}


def identity_value(identity_obj: dict[str, Any], camel: str, snake: str) -> str:
    value = camel_or_snake(identity_obj, camel, snake)
    return "" if value is None else str(value)


def candidate_logical_parent(candidate: dict[str, Any]) -> str:
    ident = identity(candidate)
    return identity_value(ident, "sourceBlockId", "source_block_id")


def is_graph_candidate(candidate: dict[str, Any]) -> bool:
    if candidate.get("graphExpanded") is True or candidate.get("graph_expanded") is True:
        return True
    for stage in candidate.get("stages", []):
        if not isinstance(stage, dict):
            continue
        sources = stage.get("retrievalSources") or stage.get("retrieval_sources") or []
        if "GRAPH_EXPANDED" in sources:
            return True
    return False


def is_direct_candidate(candidate: dict[str, Any]) -> bool:
    if candidate.get("primaryDirect") is True or candidate.get("primary_direct") is True:
        return True
    return not is_graph_candidate(candidate)


def trace_id(candidate: dict[str, Any]) -> str:
    ident = identity(candidate)
    parts = [
        identity_value(ident, "accessZoneId", "access_zone_id"),
        identity_value(ident, "retrievalSource", "retrieval_source"),
        identity_value(ident, "matchedChunkId", "matched_chunk_id"),
        identity_value(ident, "parentChunkId", "parent_chunk_id"),
        identity_value(ident, "graphSeedChunkId", "graph_seed_chunk_id"),
        identity_value(ident, "graphRelatedChunkId", "graph_related_chunk_id"),
        identity_value(ident, "graphRelatedParentChunkId", "graph_related_parent_chunk_id"),
        identity_value(ident, "graphEdgeId", "graph_edge_id"),
        identity_value(ident, "graphBindingId", "graph_binding_id"),
    ]
    return ":".join(parts)


def stage_name(stage: dict[str, Any]) -> str:
    value = stage.get("stage")
    return str(value) if value is not None else ""


def first_loss_for_candidate(candidate: dict[str, Any]) -> tuple[str | None, list[str]]:
    stages = [stage for stage in candidate.get("stages", []) if isinstance(stage, dict)]
    names = [stage_name(stage) for stage in stages]
    if any(name not in STAGE_BUCKETS for name in names):
        return "UNKNOWN", names
    for stage in stages:
        if stage.get("present") is False:
            return STAGE_BUCKETS.get(stage_name(stage), "UNKNOWN"), names
    if any(stage_name(stage) in FINAL_STAGES and stage.get("present") is True for stage in stages):
        return None, names
    present_buckets = [
        STAGE_BUCKETS[stage_name(stage)]
        for stage in stages
        if stage.get("present") is True and stage_name(stage) in STAGE_BUCKETS
    ]
    if not present_buckets:
        return "RETRIEVAL_OR_HYDRATION", names
    return NEXT_BUCKET_AFTER_LAST_PRESENT.get(present_buckets[-1], "UNKNOWN"), names


def select_expected_candidate(
    row: dict[str, Any], expected_parent: str, want_graph: bool
) -> tuple[str, list[str], list[dict[str, Any]]]:
    matches = [
        candidate
        for candidate in trace_candidates(row)
        if candidate_logical_parent(candidate) == expected_parent
        and (is_graph_candidate(candidate) if want_graph else is_direct_candidate(candidate))
    ]
    if not matches:
        return "RETRIEVAL_OR_HYDRATION", [], []
    losses = [first_loss_for_candidate(candidate) for candidate in matches]
    if any(loss is None for loss, _ in losses):
        return "PRESENT", sorted({trace_id(candidate) for candidate in matches}), matches
    known_losses = [loss for loss, _ in losses if loss]
    if not known_losses:
        return "UNKNOWN", sorted({trace_id(candidate) for candidate in matches}), matches
    if "UNKNOWN" in known_losses:
        return "UNKNOWN", sorted({trace_id(candidate) for candidate in matches}), matches
    # Pick the latest loss among equivalent candidates: the survivor got farthest.
    order = list(NEXT_BUCKET_AFTER_LAST_PRESENT)
    rank = {bucket: idx for idx, bucket in enumerate(order)}
    loss = max(known_losses, key=lambda item: rank.get(item, -1))
    return loss, sorted({trace_id(candidate) for candidate in matches}), matches


def analyze(bank: Path, raw_inputs: list[Path], output_dir: Path) -> dict[str, Any]:
    profiles, assignments = load_bank(bank)
    rows = read_jsonl(raw_inputs)
    trace_rows: list[dict[str, Any]] = []
    summary: dict[str, Counter[str]] = {
        "DIRECT_PARENT_MISSING": Counter(),
        "GRAPH_PARENT_MISSING": Counter(),
        "VALID_SURVIVOR_LOST": Counter(),
    }
    unknowns: list[dict[str, Any]] = []
    truncated_count = 0
    for row in rows:
        query_id = str(row.get("query_id", ""))
        profile_name = assignments.get(query_id, "")
        profile = resolved_profile(profile_name, profiles) if profile_name else {}
        trace = ranking_trace(row)
        if trace.get("truncated") is True:
            truncated_count += 1
        expected_direct = str(profile.get("expected_direct_parent") or profile.get("expected_parent") or "")
        expected_graph = str(profile.get("expected_graph_parent") or "")
        direct_loss, direct_ids, _ = (
            select_expected_candidate(row, expected_direct, False)
            if expected_direct
            else ("NOT_REQUIRED", [], [])
        )
        graph_loss, graph_ids, _ = (
            select_expected_candidate(row, expected_graph, True)
            if expected_graph and profile.get("required_graph_origin") is True
            else ("NOT_REQUIRED", [], [])
        )
        final_parents = [context_logical_parent(context) for context in response_contexts(row)]
        failure_codes: list[str] = []
        if expected_direct and expected_direct not in final_parents:
            failure_codes.append("DIRECT_PARENT_MISSING")
            summary["DIRECT_PARENT_MISSING"][direct_loss] += 1
        if expected_graph and profile.get("required_graph_origin") is True and expected_graph not in final_parents:
            failure_codes.append("GRAPH_PARENT_MISSING")
            summary["GRAPH_PARENT_MISSING"][graph_loss] += 1
        if "DIRECT_PARENT_MISSING" in failure_codes or "GRAPH_PARENT_MISSING" in failure_codes:
            survivor_loss = direct_loss if direct_loss != "PRESENT" else graph_loss
            summary["VALID_SURVIVOR_LOST"][survivor_loss] += 1
        if direct_loss == "UNKNOWN" or graph_loss == "UNKNOWN" or trace.get("truncated") is True:
            unknowns.append({"query_id": query_id, "entry_point": row.get("entry_point")})
        trace_rows.append(
            {
                "query_id": query_id,
                "entry_point": row.get("entry_point"),
                "run_kind": row.get("run_kind"),
                "run_index": row.get("run_index"),
                "qrel_profile": profile_name,
                "expected_direct_parent": expected_direct,
                "expected_graph_parent": expected_graph,
                "direct_first_loss_stage": direct_loss,
                "graph_first_loss_stage": graph_loss,
                "direct_trace_candidate_ids": direct_ids,
                "graph_trace_candidate_ids": graph_ids,
                "final_logical_parents": final_parents,
                "failure_codes": failure_codes,
            }
        )
    matrix = {
        "schema_version": 1,
        "raw_observation_count": len(rows),
        "trace_truncation_count": truncated_count,
        "unknown_count": len(unknowns),
        "unknown_observations": unknowns,
        "first_loss_summary": {
            key: dict(sorted(counter.items())) for key, counter in summary.items()
        },
    }
    write_jsonl(output_dir / "candidate-stage-trace.jsonl", trace_rows)
    write_json(output_dir / "first-loss-summary.json", matrix)
    write_json(output_dir / "first-loss-matrix.json", trace_rows)
    return matrix


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bank", type=Path, required=True)
    parser.add_argument("--raw-input", action="append", type=Path, required=True)
    parser.add_argument("--identity-map", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    matrix = analyze(args.bank, args.raw_input, args.output_dir)
    print(json.dumps(matrix, ensure_ascii=False, sort_keys=True))
    return 1 if matrix["trace_truncation_count"] or matrix["unknown_count"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
