#!/usr/bin/env python3
import hashlib
import json
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROFILE = ROOT / "benchmarks/quality/profiles/full-capability-quick.json"
QUERY_DIR = ROOT / "benchmarks/quality/queries"
QRELS = ROOT / "benchmarks/quality/qrels/qrels.jsonl"


def evidence_group(query):
    expected = query.get("expected", {})
    documents = sorted(expected.get("must_contain_document_ids", []) + expected.get("required_document_ids", []))
    blocks = sorted(expected.get("must_contain_block_ids", []) + expected.get("required_block_ids", []))
    if documents or blocks:
        return json.dumps([documents, blocks], separators=(",", ":"))
    normalized = " ".join("".join(ch.lower() if ch.isalnum() else " " for ch in query.get("question", "")).split())
    return normalized


def write_jsonl(path, values):
    path.parent.mkdir(parents=True, exist_ok=True)
    body = "".join(json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n" for value in values)
    path.write_text(body, encoding="utf-8")


def main():
    profile = json.loads(PROFILE.read_text(encoding="utf-8"))
    queries = []
    for name in profile["queries"]:
        for line in (QUERY_DIR / f"{name}.jsonl").read_text(encoding="utf-8").splitlines():
            if line.strip():
                queries.append(json.loads(line))
    groups = defaultdict(list)
    for query in queries:
        query["critical"] = query.get("critical", False) or query.get("category") in {
            "exact_lookup", "lexical_sparse", "access_isolation"
        }
        groups[evidence_group(query)].append(query)
    ordered = sorted(groups.values(), key=lambda group: hashlib.sha256(evidence_group(group[0]).encode()).hexdigest())
    targets = {"tuning": round(len(queries) * .60), "validation": round(len(queries) * .20)}
    splits = {"tuning": [], "validation": [], "holdout": []}
    for group in ordered:
        if len(splits["tuning"]) < targets["tuning"]:
            destination = "tuning"
        elif len(splits["validation"]) < targets["validation"]:
            destination = "validation"
        else:
            destination = "holdout"
        splits[destination].extend(group)
    for name, values in splits.items():
        write_jsonl(QUERY_DIR / f"fix480-{name}.jsonl", sorted(values, key=lambda value: value["id"]))
    qrels = []
    for query in sorted(queries, key=lambda value: value["id"]):
        expected = query.get("expected", {})
        documents = expected.get("must_contain_document_ids", []) + expected.get("required_document_ids", [])
        blocks = expected.get("must_contain_block_ids", []) + expected.get("required_block_ids", [])
        relevant = []
        for document in documents:
            if blocks:
                relevant.extend({"document_id": document, "source_block_id": block, "relevance": 3} for block in blocks)
            else:
                relevant.append({"document_id": document, "source_block_id": "", "relevance": 3})
        qrels.append({"query_id": query["id"], "relevant": relevant})
    write_jsonl(QRELS, qrels)
    print(json.dumps({name: len(values) for name, values in splits.items()}, sort_keys=True))


if __name__ == "__main__":
    main()
