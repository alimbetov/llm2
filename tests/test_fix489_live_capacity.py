import importlib.util
import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]


def load_module(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


live_client = load_module("astravector_live_client", ROOT / "scripts" / "astravector_live_client.py")
fix489 = load_module("fix489_live_capacity", ROOT / "scripts" / "fix489_live_capacity.py")


class Fix489LiveCapacityContracts(unittest.TestCase):
    def test_live_client_builds_single_document_root_for_multilingual_text(self):
        blocks = live_client.make_logical_blocks(
            "Русский абзац.\n\nKazakh мәтіні.\n\nEnglish paragraph.",
            namespace="fix489",
            section_path="fix489-test",
            heading="FIX489 test",
            root_text="FIX489 test document root.",
            metadata_prefix="fix489",
        )
        roots = [block for block in blocks if block["blockType"] == "BLOCK_TYPE_DOCUMENT"]
        self.assertEqual(len(roots), 1)
        self.assertEqual(len(blocks), 4)
        self.assertTrue(all(block.get("parentBlockId") == roots[0]["blockId"] for block in blocks[1:]))

    def test_live_capacity_supports_all_required_operation_types(self):
        self.assertEqual(
            set(fix489.QUERY_BY_TYPE),
            {"SEARCH", "RETRIEVE_CONTEXT", "GRAPH_RETRIEVE_CONTEXT", "SYNC_STATUS", "LIFECYCLE_STATUS"},
        )
        source = (ROOT / "scripts" / "fix489_live_capacity.py").read_text(encoding="utf-8")
        for operation_type in (
            "SEARCH",
            "RETRIEVE_CONTEXT",
            "GRAPH_RETRIEVE_CONTEXT",
            "INGEST_VERSION",
            "DELETE_OR_EXPIRE",
            "SYNC_STATUS",
            "LIFECYCLE_STATUS",
        ):
            self.assertIn(operation_type, source)
        self.assertIn("--operation-smoke-output", source)

    def test_capacity_and_soak_scripts_no_longer_end_as_not_implemented(self):
        for script_name in ("fix487bc-capacity-campaign.sh", "fix487c-soak-60m.sh"):
            text = (ROOT / "scripts" / script_name).read_text(encoding="utf-8")
            self.assertNotIn("LIVE_CAPACITY_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN", text)
            self.assertNotIn("LIVE_SOAK_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN", text)
            self.assertIn("scripts/fix489_live_capacity.py", text)

    def test_capacity_and_soak_use_fix489_operational_profile_by_default(self):
        profile = (ROOT / "config" / "application-fix489-capacity.yaml").read_text(encoding="utf-8")
        self.assertIn("FIX489_QUERY_DEADLINE_MS:-45000", profile)
        self.assertIn("FIX489_POSTGRES_STATEMENT_TIMEOUT_MS:-30000", profile)
        self.assertIn("FIX489_SPARSE_REQUIRED:-false", profile)
        for script_name in ("fix487bc-capacity-campaign.sh", "fix487c-soak-60m.sh"):
            text = (ROOT / "scripts" / script_name).read_text(encoding="utf-8")
            self.assertIn('ASTRAVECTOR_PROFILE="fix489-capacity"', text)
            self.assertIn('FIX489_CLIENT_DEADLINE_MS="${FIX489_CLIENT_DEADLINE_MS:-45000}"', text)

    def test_live_workload_uses_run_scoped_namespace(self):
        workload_a = fix489.LiveWorkload(client=object(), output=pathlib.Path("/tmp/fix489/run-a"))
        workload_b = fix489.LiveWorkload(client=object(), output=pathlib.Path("/tmp/fix489/run-b"))
        self.assertEqual(workload_a.run_namespace, "fix489-run-a")
        self.assertEqual(workload_b.run_namespace, "fix489-run-b")
        self.assertNotEqual(workload_a.run_namespace, workload_b.run_namespace)

    def test_capacity_level_artifacts_cover_monitoring_and_audits(self):
        expected = {
            "operations.jsonl",
            "resource-samples.jsonl",
            "postgres-after-cooldown.json",
            "qdrant-after-cooldown.json",
            "outbox-after-cooldown.json",
            "latency-summary.json",
            "grpc-status-summary.json",
            "integrity-summary.json",
            "level-result.json",
        }
        evidence = load_module("fix487bc_capacity_evidence", ROOT / "scripts" / "fix487bc_capacity_evidence.py")
        self.assertTrue(expected.issubset(set(evidence.LEVEL_ARTIFACTS)))

    def test_official_capacity_levels_remain_default(self):
        self.assertEqual(fix489.capacity_levels(), (25, 50, 100, 200))

    def test_grpcurl_camel_case_statuses_are_normalized(self):
        self.assertEqual(
            fix489.grpc_status_from_error("ERROR:\n  Code: DeadlineExceeded\n  Message: inference deadline"),
            "DEADLINE_EXCEEDED",
        )
        self.assertEqual(
            fix489.grpc_status_from_error("ERROR:\n  Code: ResourceExhausted\n  Message: admission queue full"),
            "RESOURCE_EXHAUSTED",
        )
        self.assertEqual(
            fix489.grpc_status_from_error("ERROR:\n  Code: FailedPrecondition\n  Message: document inactive"),
            "FAILED_PRECONDITION",
        )


if __name__ == "__main__":
    unittest.main()
