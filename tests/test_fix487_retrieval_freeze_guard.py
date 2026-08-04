import tempfile
import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487_retrieval_freeze_guard as guard


class RetrievalFreezeGuardTests(unittest.TestCase):
    def test_allowed_phase_paths_are_not_violations(self):
        for path in (
            "docs/fix487/phase-a-retrieval-freeze/RESULT.md",
            "scripts/fix487_retrieval_freeze_guard.py",
            "tests/test_fix487_retrieval_freeze_guard.py",
            "config/application-fix487b.yaml",
            "config/application-fix489-capacity.yaml",
            "docker-compose.fix487b.yml",
        ):
            self.assertTrue(guard.matches_any(path, guard.ALLOWED_PATH_PATTERNS))

    def test_config_change_is_protected(self):
        self.assertTrue(guard.matches_any("config/application.yaml", guard.PROTECTED_CONFIG_PATTERNS))
        self.assertTrue(guard.matches_any("src/config/mod.rs", guard.PROTECTED_CONFIG_PATTERNS))

    def test_frozen_bank_queries_qrels_and_fixtures_are_protected(self):
        self.assertTrue(
            guard.matches_any(
                "benchmarks/hierarchical/fix486/qrels/hierarchical-qrels-v1.jsonl",
                guard.PROTECTED_QREL_PATTERNS,
            )
        )
        self.assertTrue(
            guard.matches_any(
                "benchmarks/quality/queries/rag-quality-bank-v1-graph.jsonl",
                guard.PROTECTED_QREL_PATTERNS,
            )
        )
        self.assertTrue(
            guard.matches_any(
                "benchmarks/hierarchical/fix486/corpus/hierarchical-fixture-v1.json",
                guard.PROTECTED_FIXTURE_PATTERNS,
            )
        )

    def test_retrieval_semantic_hunk_is_rejected(self):
        diff = """@@
-fn final_no_answer_should_trigger(old: bool) -> bool {
+fn final_no_answer_should_trigger(old: bool) -> bool {
+    false
 }"""
        self.assertFalse(guard.retrieval_source_change_is_allowed(diff))

    def test_operational_hunk_is_allowed(self):
        diff = """@@
+tracing::debug!("graph expansion timeout observed");
+metrics::counter!("astravector_graph_timeout_total").increment(1);
"""
        self.assertTrue(guard.retrieval_source_change_is_allowed(diff))

    def test_makefile_target_hunk_is_allowed(self):
        diff = """@@
+.PHONY: verify-fix487a-retrieval-freeze
+verify-fix487b-contracts:
+\tpython3 -m py_compile scripts/fix487b_dataset.py
+\tpython3 -m unittest -v tests/test_fix487b_dataset.py
+verify-fix487bc-capacity-contracts:
+\tpython3 -m py_compile scripts/fix487bc_capacity_campaign.py scripts/fix487bc_capacity_evidence.py
+verify-fix487c-soak-contracts:
+\tpython3 -m py_compile scripts/fix487c_soak.py
+verify-fix487a-retrieval-freeze:
+\tpython3 scripts/fix487_retrieval_freeze_guard.py --repo .
+\tpython3 -m unittest -v tests/test_fix487_retrieval_freeze_guard.py
"""
        self.assertTrue(guard.makefile_change_is_allowed(diff))

    def test_makefile_unrelated_hunk_is_rejected(self):
        diff = """@@
+quality-runtime-confidence:
+\tASTRAVECTOR_GRAPH_MMR_LAMBDA=0.99 cargo test
"""
        self.assertFalse(guard.makefile_change_is_allowed(diff))

    def test_manifest_completeness(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            self.assertFalse(guard.manifest_complete(repo))
            for relative in guard.REQUIRED_MANIFEST_FILES:
                target = repo / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("ok\n", encoding="utf-8")
            self.assertTrue(guard.manifest_complete(repo))

    def test_untracked_file_command_is_part_of_change_scan(self):
        source = Path(guard.__file__).read_text(encoding="utf-8")
        self.assertIn('"ls-files", "--others", "--exclude-standard"', source)


if __name__ == "__main__":
    unittest.main()
