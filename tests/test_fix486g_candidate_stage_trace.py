import json
import tempfile
import unittest
from pathlib import Path

from scripts.fix486g_candidate_stage_trace import analyze


class Fix486GCandidateStageTraceTest(unittest.TestCase):
    def test_complete_trace_reports_mmr_first_loss_without_unknown(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bank = root / "bank"
            (bank / "qrels").mkdir(parents=True)
            (bank / "qrels" / "qrel-profiles-v1.json").write_text(
                json.dumps(
                    {
                        "profiles": {
                            "POSITIVE_GRAPH": {
                                "expected_direct_parent": "parent-direct",
                                "expected_graph_parent": "parent-graph",
                                "required_graph_origin": True,
                            }
                        }
                    }
                ),
                encoding="utf-8",
            )
            (bank / "qrels" / "query-qrel-assignments-v1.jsonl").write_text(
                json.dumps({"query_id": "q1", "qrel_profile": "POSITIVE_GRAPH"}) + "\n",
                encoding="utf-8",
            )
            raw = root / "raw.jsonl"
            row = {
                "query_id": "q1",
                "entry_point": "Search",
                "run_kind": "warm",
                "run_index": 1,
                "response": {
                    "results": [
                        {
                            "citation": {
                                "metadata": {"source_block_id": "parent-direct"}
                            }
                        }
                    ],
                    "diagnostics": {
                        "rankingTrace": {
                            "truncated": False,
                            "candidates": [
                                {
                                    "primaryDirect": True,
                                    "graphExpanded": False,
                                    "identity": {
                                        "sourceBlockId": "parent-direct",
                                        "matchedChunkId": "direct-child",
                                        "parentChunkId": "direct-parent",
                                    },
                                    "stages": [
                                        {"stage": "RETRIEVED", "present": True},
                                        {"stage": "PRE_NO_ANSWER", "present": True},
                                        {"stage": "POST_NO_ANSWER", "present": True},
                                        {"stage": "PRE_MMR", "present": True},
                                        {"stage": "POST_MMR", "present": True},
                                        {"stage": "FINAL", "present": True},
                                    ],
                                },
                                {
                                    "primaryDirect": False,
                                    "graphExpanded": True,
                                    "identity": {
                                        "sourceBlockId": "parent-graph",
                                        "retrievalSource": "GRAPH_EXPANDED",
                                        "matchedChunkId": "graph-child",
                                        "parentChunkId": "graph-parent",
                                        "graphSeedChunkId": "seed-child",
                                        "graphRelatedChunkId": "graph-child",
                                        "graphEdgeId": "edge-1",
                                        "graphBindingId": "binding-1",
                                    },
                                    "stages": [
                                        {"stage": "GRAPH_EXPANDED", "present": True},
                                        {"stage": "PRE_MMR", "present": True},
                                        {"stage": "POST_MMR", "present": False},
                                    ],
                                },
                            ],
                        }
                    },
                },
            }
            raw.write_text(json.dumps(row) + "\n", encoding="utf-8")

            matrix = analyze(bank, [raw], root / "out")

            self.assertEqual(matrix["trace_truncation_count"], 0)
            self.assertEqual(matrix["unknown_count"], 0)
            self.assertEqual(
                matrix["first_loss_summary"]["GRAPH_PARENT_MISSING"],
                {"MMR_SELECTION": 1},
            )


if __name__ == "__main__":
    unittest.main()
