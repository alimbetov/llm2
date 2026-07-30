#!/usr/bin/env python3
"""Deterministic FIX487B synthetic dataset generator."""

from __future__ import annotations

import argparse
import hashlib
import json
import random
from collections import Counter
from pathlib import Path


DATASET_VERSION = "fix487b-dataset-v1"
DEFAULT_SEED = 487205
ZONES = ("4871", "4872", "4873")
ACCESS_LEVELS = ("PUBLIC", "INTERNAL", "CONFIDENTIAL", "RESTRICTED")
LANGUAGES = ("EN", "RU", "KZ")
LIFECYCLE_STATES = ("ACTIVE", "INDEXING", "DELETED", "EXPIRED", "LEGAL_HOLD_ACTIVE")


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canonical_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def document_text(index: int, language: str, exact: bool, graph: bool) -> str:
    exact_anchor = f"FIX487B_TECH_ID_{index:03d}" if exact else f"FIX487B_TOPIC_{index:03d}"
    graph_anchor = f"FIX487B_GRAPH_EDGE_{index:03d}" if graph else "FIX487B_DIRECT_ONLY"
    if language == "RU":
        return (
            f"Документ {index}. Контрольная фраза {exact_anchor}. "
            f"Связь графа {graph_anchor}. Текст описывает стабильную индексацию и поиск."
        )
    if language == "KZ":
        return (
            f"Құжат {index}. Бақылау белгісі {exact_anchor}. "
            f"Граф байланысы {graph_anchor}. Мәтін тұрақты іздеу дәлелін сипаттайды."
        )
    return (
        f"Document {index}. Control anchor {exact_anchor}. "
        f"Graph relation marker {graph_anchor}. The text describes stable indexing and retrieval evidence."
    )


def build_documents(seed: int = DEFAULT_SEED, count: int = 60) -> list[dict]:
    rng = random.Random(seed)
    documents: list[dict] = []
    for idx in range(count):
        zone = ZONES[idx % len(ZONES)]
        access_level = ACCESS_LEVELS[idx % len(ACCESS_LEVELS)]
        language = LANGUAGES[idx % len(LANGUAGES)] if idx < 9 else rng.choice(LANGUAGES)
        lifecycle = "ACTIVE"
        if idx in range(6):
            lifecycle = "INDEXING"
        elif idx in range(6, 12):
            lifecycle = "DELETED"
        elif idx in range(12, 18):
            lifecycle = "EXPIRED"
        elif idx in range(18, 21):
            lifecycle = "LEGAL_HOLD_ACTIVE"
        version = 2 if idx < 6 else 1
        exact = idx < 12
        graph = idx < 12
        blocks = [
            {
                "logical_block_id": f"fix487b-doc-{idx:03d}-parent-001",
                "kind": "PARENT",
                "ordinal": 1,
                "text": document_text(idx, language, exact, graph),
                "source_location": {"page": 1, "section": f"fix487b-{idx:03d}"},
            }
        ]
        documents.append(
            {
                "external_document_id": f"fix487b-doc-{idx:03d}",
                "document_version": version,
                "title": f"FIX487B synthetic document {idx:03d}",
                "source_uri": f"synthetic://fix487b/{idx:03d}",
                "source_type": "SYNTHETIC_TEXT",
                "mime_type": "text/plain; charset=utf-8",
                "language": language,
                "access_zone": zone,
                "access_level": access_level,
                "lifecycle": lifecycle,
                "metadata": {
                    "dataset_version": DATASET_VERSION,
                    "seed": seed,
                    "exact_identifier": exact,
                    "graph_relation_source": graph,
                    "legal_hold": lifecycle == "LEGAL_HOLD_ACTIVE",
                },
                "logical_blocks": blocks,
            }
        )
    return documents


def build_manifest(documents: list[dict], seed: int = DEFAULT_SEED) -> dict:
    serialized_docs = "\n".join(canonical_json(doc) for doc in documents) + "\n"
    block_count = sum(len(doc["logical_blocks"]) for doc in documents)
    active_like = sum(1 for doc in documents if doc["lifecycle"] in ("ACTIVE", "LEGAL_HOLD_ACTIVE"))
    return {
        "dataset_version": DATASET_VERSION,
        "seed": seed,
        "document_count": len(documents),
        "block_count": block_count,
        "expected_binding_count": active_like * 3,
        "zone_distribution": dict(Counter(doc["access_zone"] for doc in documents)),
        "access_level_distribution": dict(Counter(doc["access_level"] for doc in documents)),
        "language_distribution": dict(Counter(doc["language"] for doc in documents)),
        "lifecycle_distribution": dict(Counter(doc["lifecycle"] for doc in documents)),
        "document_sha256_aggregate": sha256_text(serialized_docs),
    }


def build_logical_to_runtime(documents: list[dict]) -> list[dict]:
    return [
        {
            "external_document_id": doc["external_document_id"],
            "document_version": doc["document_version"],
            "access_zone": doc["access_zone"],
            "runtime_document_id": None,
            "runtime_version_id": None,
            "logical_blocks": [block["logical_block_id"] for block in doc["logical_blocks"]],
        }
        for doc in documents
    ]


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.write_text("\n".join(canonical_json(row) for row in rows) + "\n", encoding="utf-8")


def write_dataset(output: Path, seed: int = DEFAULT_SEED) -> dict:
    output.mkdir(parents=True, exist_ok=True)
    documents = build_documents(seed)
    manifest = build_manifest(documents, seed)
    operations = [
        {
            "operation_id": f"fix487b-ingest-{idx:03d}",
            "operation_type": "INGEST_DOCUMENT",
            "external_document_id": doc["external_document_id"],
            "access_zone": doc["access_zone"],
            "access_level": doc["access_level"],
            "lifecycle": doc["lifecycle"],
        }
        for idx, doc in enumerate(documents)
    ]
    (output / "dataset-manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    write_jsonl(output / "documents.jsonl", documents)
    write_jsonl(output / "operations-input.jsonl", operations)
    write_jsonl(output / "logical-to-runtime.json", build_logical_to_runtime(documents))
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    args = parser.parse_args()
    manifest = write_dataset(Path(args.output), args.seed)
    print(json.dumps(manifest, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
