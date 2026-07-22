import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "fix486g_proof", ROOT / "scripts/fix486g_proof.py"
)
PROOF = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(PROOF)


MANDATORY_PASS_EVIDENCE = {
    "aggregate.json",
    "stage-results.json",
    "query-results.jsonl",
    "identity-map/logical-to-runtime.json",
    "graph-disabled/results.jsonl",
    "graph-audit/graph-identity-chain.json",
    "graph-audit/graph-provenance-trace.json",
    "canonical-audit/integrity-summary.json",
    "qdrant-audit/payload-consistency.json",
    "comparisons/entry-point-parity.json",
    "comparisons/warm-repeat.json",
    "restart/pre-post-restart.json",
    "cleanup/summary.json",
    "statistical/statistical-report.json",
    "statistical/statistical-report.md",
    "statistical/per-query-results.jsonl",
    "statistical/per-slice-metrics.json",
    "statistical/latency-distribution.json",
    "statistical/safety-hard-gates.json",
    "statistical/confidence-intervals.json",
    "defect-register.json",
}


def identity(logical_chunk_id: str) -> dict:
    return {
        "logical_chunk_id": logical_chunk_id,
        "runtime_access_zone_id": "runtime-zone-a",
    }


def graph_metadata(matched: str, parent: str, edge_id: str) -> dict:
    return {
        "retrieval_source": "GRAPH_EXPANDED",
        "graph_seed_access_zone_id": "runtime-zone-a",
        "graph_seed_document_id": "runtime-document",
        "graph_seed_document_version": "1",
        "graph_seed_chunk_id": "runtime-direct-child",
        "graph_seed_parent_chunk_id": "runtime-direct-parent",
        "graph_relation_id": "relation-1",
        "graph_edge_id": edge_id,
        "graph_relation_type": "REPAIRED_BY",
        "graph_relation_score": "0.9",
        "graph_related_access_zone_id": "runtime-zone-a",
        "graph_related_document_id": "runtime-document",
        "graph_related_document_version": "1",
        "graph_related_chunk_id": matched,
        "graph_related_parent_chunk_id": parent,
        "graph_hop_distance": "1",
    }


def context(matched: str, parent: str, metadata: dict, parent_text: str = "") -> dict:
    return {
        "matchedChunkId": matched,
        "parentChunkId": parent,
        "accessZoneId": "runtime-zone-a",
        "documentVersion": "1",
        "matchedText": "matched",
        "parentText": parent_text,
        "metadata": metadata,
    }


class NormalizeContracts(unittest.TestCase):
    def test_direct_primary_preserves_secondary_graph_provenance(self):
        identities = {
            "runtime-direct-child": identity("child-a1-180"),
            "runtime-direct-parent": identity("parent-a1"),
            "runtime-graph-child": identity("child-a3-180"),
            "runtime-graph-parent": identity("parent-a3"),
        }
        secondary = graph_metadata(
            "runtime-graph-child", "runtime-graph-parent", "edge-secondary"
        )
        secondary.update(
            {
                "retrieval_source": "VECTOR_DIRECT",
                "retrieval_sources": '["VECTOR_DIRECT","GRAPH_EXPANDED"]',
                "graph_secondary_provenance": "true",
            }
        )
        response = {
            "results": [
                context(
                    "runtime-direct-child",
                    "runtime-direct-parent",
                    {"retrieval_source": "VECTOR_DIRECT"},
                ),
                context(
                    "runtime-graph-parent",
                    "runtime-graph-parent",
                    secondary,
                    "ASTRA_RECONCILIATION_A3",
                ),
            ]
        }

        result = PROOF.normalize(
            {"query_id": "q-graph-repair", "case_id": "FIX486-08"},
            {
                "expected_direct_parent": "parent-a1",
                "expected_graph_parent": "parent-a3",
                "expected_graph_child_any": ["child-a3-180"],
                "required_graph_relation_any": ["REPAIRED_BY"],
            },
            "Search",
            response,
            identities,
            True,
        )

        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["logical_identity"]["direct_parents"], ["parent-a1"])
        self.assertEqual(result["logical_identity"]["graph_children"], ["child-a3-180"])
        self.assertEqual(result["logical_identity"]["graph_parents"], ["parent-a3"])

    def test_any_wrong_graph_final_context_fails_the_result(self):
        identities = {
            "runtime-direct-child": identity("child-a1-180"),
            "runtime-direct-parent": identity("parent-a1"),
            "runtime-graph-child": identity("child-a3-180"),
            "runtime-graph-parent": identity("parent-a3"),
            "runtime-wrong-parent": identity("parent-a2"),
        }
        response = {
            "results": [
                context(
                    "runtime-direct-child",
                    "runtime-direct-parent",
                    {"retrieval_source": "VECTOR"},
                ),
                context(
                    "runtime-graph-child",
                    "runtime-graph-parent",
                    graph_metadata(
                        "runtime-graph-child", "runtime-graph-parent", "edge-good"
                    ),
                    "ASTRA_RECONCILIATION_A3",
                ),
                context(
                    "runtime-graph-child",
                    "runtime-wrong-parent",
                    graph_metadata(
                        "runtime-graph-child", "runtime-wrong-parent", "edge-wrong"
                    ),
                    "wrong graph parent",
                ),
            ]
        }

        result = PROOF.normalize(
            {"query_id": "q-graph-repair", "case_id": "FIX486-08"},
            {
                "expected_direct_parent": "parent-a1",
                "expected_graph_parent": "parent-a3",
                "expected_graph_child_any": ["child-a3-180"],
                "required_graph_relation_any": ["REPAIRED_BY"],
            },
            "Search",
            response,
            identities,
            True,
        )

        self.assertEqual(result["status"], "FAIL")
        self.assertIn("GRAPH_WRONG_PARENT", result["failure_codes"])


class ParityContracts(unittest.TestCase):
    def test_parity_compares_protected_graph_provenance(self):
        base = {
            "query_id": "q-graph-repair",
            "status": "PASS",
            "logical_identity": {},
            "runtime_identity": [],
            "assertions": {},
            "failure_codes": [],
            "protected_provenance": [
                {
                    "matched_chunk_id": "runtime-graph-child",
                    "parent_chunk_id": "runtime-graph-parent",
                    "graph_edge_id": "edge-search",
                }
            ],
        }
        search = dict(base, entry_point="Search")
        retrieve = copy.deepcopy(base)
        retrieve["entry_point"] = "RetrieveContext"
        retrieve["protected_provenance"][0]["graph_edge_id"] = "edge-retrieve"

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            left = root / "left.jsonl"
            right = root / "right.jsonl"
            left.write_text(json.dumps(search) + "\n", encoding="utf-8")
            right.write_text(json.dumps(retrieve) + "\n", encoding="utf-8")

            result = PROOF.compare_result_sets(left, right, parity=True)

        self.assertEqual(result["status"], "FAIL")
        self.assertEqual(result["differences"], ["q-graph-repair"])


class EvidenceManifestContracts(unittest.TestCase):
    def make_run(self, root: Path) -> None:
        for relative in MANDATORY_PASS_EVIDENCE:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            payload = (
                {"verdict": "FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS"}
                if relative == "aggregate.json"
                else {"status": "PASS"}
            )
            path.write_text(json.dumps(payload) + "\n", encoding="utf-8")

    def test_pass_requires_the_fixed_mandatory_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            self.make_run(run)
            omitted = "graph-audit/graph-provenance-trace.json"
            (run / omitted).unlink()
            manifest = PROOF.build_manifest(run)

            result = PROOF.verify_manifest(run, manifest)

        self.assertEqual(result["status"], "FAIL")
        self.assertIn("PASS_MANIFEST_MISSING_MANDATORY_ARTIFACT", result["failure_codes"])

    def test_pass_rejects_empty_mandatory_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            self.make_run(run)
            empty = run / "graph-audit/graph-provenance-trace.json"
            empty.write_bytes(b"")
            manifest = PROOF.build_manifest(run)

            result = PROOF.verify_manifest(run, manifest)

        self.assertEqual(result["status"], "FAIL")
        self.assertTrue(
            any(code.startswith("EMPTY_MANDATORY_ARTIFACT:") for code in result["failure_codes"])
        )

    def test_manifest_file_count_aggregate_and_hash_are_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            self.make_run(run)
            manifest = PROOF.build_manifest(run)

            bad_count = copy.deepcopy(manifest)
            bad_count["file_count"] += 1
            self.assertIn(
                "MANIFEST_FILE_COUNT_MISMATCH",
                PROOF.verify_manifest(run, bad_count)["failure_codes"],
            )

            bad_aggregate = copy.deepcopy(manifest)
            bad_aggregate["aggregate_sha256"] = "0" * 64
            self.assertIn(
                "MANIFEST_AGGREGATE_MISMATCH",
                PROOF.verify_manifest(run, bad_aggregate)["failure_codes"],
            )

            bad_hash = copy.deepcopy(manifest)
            bad_hash["records"][0]["sha256"] = "0" * 64
            self.assertTrue(
                any(
                    code.startswith("HASH_MISMATCH:")
                    for code in PROOF.verify_manifest(run, bad_hash)["failure_codes"]
                )
            )


if __name__ == "__main__":
    unittest.main()
