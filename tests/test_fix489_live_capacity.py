import importlib.util
import os
import pathlib
import sys
import tempfile
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
    def tearDown(self):
        os.environ.pop("FIX489_CAPACITY_LEVELS", None)
        os.environ.pop("FIX489_RUN_EXTREME_LEVELS", None)

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
        self.assertIn("FIX489_QUERY_DEADLINE_MS:-67500", profile)
        self.assertIn("FIX489_POSTGRES_STATEMENT_TIMEOUT_MS:-45000", profile)
        self.assertIn("FIX489_SPARSE_REQUIRED:-false", profile)
        for script_name in ("fix487bc-capacity-campaign.sh", "fix487c-soak-60m.sh"):
            text = (ROOT / "scripts" / script_name).read_text(encoding="utf-8")
            self.assertIn('ASTRAVECTOR_PROFILE="fix489-capacity"', text)
            self.assertIn('FIX489_CLIENT_DEADLINE_MS="${FIX489_CLIENT_DEADLINE_MS:-67500}"', text)
        campaign_script = (ROOT / "scripts" / "fix487bc-capacity-campaign.sh").read_text(encoding="utf-8")
        self.assertIn('FIX489_CAPACITY_LEVELS="${FIX489_CAPACITY_LEVELS:-5,10,15,20,25,50}"', campaign_script)
        self.assertIn('FIX489_LOAD_MODE="${FIX489_LOAD_MODE:-CLOSED_LOOP}"', campaign_script)
        self.assertIn('fix489-capacity-$(date -u +%Y%m%dT%H%M%SZ)', campaign_script)
        self.assertIn('${EVIDENCE_ROOT}/fix489-capacity/${RUN_ID}', campaign_script)

    def test_live_workload_uses_run_scoped_namespace(self):
        workload_a = fix489.LiveWorkload(client=object(), output=pathlib.Path("/tmp/fix489/run-a"))
        workload_b = fix489.LiveWorkload(client=object(), output=pathlib.Path("/tmp/fix489/run-b"))
        self.assertEqual(workload_a.run_namespace, "fix489-run-a")
        self.assertEqual(workload_b.run_namespace, "fix489-run-b")
        self.assertNotEqual(workload_a.run_namespace, workload_b.run_namespace)

    def test_delete_or_expire_uses_prepared_pool_not_measured_ingest(self):
        class FakeClient:
            def __init__(self):
                self.index_calls = 0
                self.deleted: list[tuple[str, str, int]] = []

            def index_text(self, **kwargs):
                self.index_calls += 1
                document_id = f"doc-{self.index_calls}"
                return {
                    "document_id": document_id,
                    "response": {
                        "document": {
                            "accessZoneId": f"zone-{self.index_calls}",
                            "documentId": document_id,
                            "documentVersion": 1,
                        }
                    },
                }

            def wait_vector_sync(self, **kwargs):
                return {"status": {"state": "OPERATION_STATE_READY_TO_ACTIVATE"}}

            def activate_document(self, **kwargs):
                return {"activated": True}

            def delete_document_vectors(self, *, access_zone_id, document_id, document_version, reason):
                self.deleted.append((access_zone_id, document_id, document_version))
                return {"operation": {"state": "OPERATION_STATE_ACCEPTED"}, "reason": reason}

        client = FakeClient()
        workload = fix489.LiveWorkload(client=client, output=pathlib.Path("/tmp/fix489/delete-pool-contract"))
        workload.prepare_documents(count=1)
        workload.prepare_delete_documents(count=2)
        calls_after_setup = client.index_calls

        op = fix489.ScheduledOperation(
            operation_id="fix487b-op-001-delete_or_expire",
            cycle_index=1,
            operation_type="DELETE_OR_EXPIRE",
            access_zone="4871",
            access_level="PUBLIC",
            logical_identity="fix487b-doc-001",
            scheduled_at=1,
        )
        status, _response, classification = workload.execute_sync(op)

        self.assertEqual(status, "OK")
        self.assertEqual(classification, "DELETE_SCHEDULED")
        self.assertEqual(client.index_calls, calls_after_setup)
        self.assertEqual(len(client.deleted), 1)
        self.assertEqual(workload.delete_documents[0]["pool_state"], fix489.DELETE_SCHEDULED)

    def test_measured_ingest_records_accepted_document_without_sync_blocking(self):
        class FakeClient:
            def __init__(self):
                self.wait_calls = 0

            def index_text(self, **kwargs):
                return {
                    "document_id": "doc-accepted",
                    "response": {
                        "document": {
                            "accessZoneId": "zone-a",
                            "documentId": "doc-accepted",
                            "documentVersion": 1,
                        }
                    },
                }

            def wait_vector_sync(self, **kwargs):
                self.wait_calls += 1
                raise AssertionError("measured ingest must not wait for async vector sync")

        workload = fix489.LiveWorkload(client=FakeClient(), output=pathlib.Path("/tmp/fix489/async-ingest-contract"))
        workload.documents = [{"access_zone_code": "4871", "access_zone_id": "zone-a"}]
        op = fix489.ScheduledOperation(
            operation_id="fix487b-op-004-ingest_version",
            cycle_index=0,
            operation_type="INGEST_VERSION",
            access_zone="4871",
            access_level="PUBLIC",
            logical_identity="fix487b-doc-000",
            scheduled_at=4,
        )

        status, _response, classification = workload.execute_sync(op)

        self.assertEqual(status, "OK")
        self.assertEqual(classification, "INGEST_ACCEPTED")
        self.assertEqual(len(workload.pending_ingests), 1)
        self.assertEqual(workload.client.wait_calls, 0)

    def test_pending_ingest_finalization_waits_activates_and_extends_delete_pool(self):
        class FakeClient:
            def __init__(self):
                self.waited: list[dict] = []
                self.activated: list[dict] = []

            def wait_vector_sync(self, **kwargs):
                self.waited.append(kwargs)
                return {"status": {"state": "OPERATION_STATE_READY_TO_ACTIVATE"}}

            def activate_document(self, **kwargs):
                self.activated.append(kwargs)
                return {"status": "ACTIVE"}

        with tempfile.TemporaryDirectory() as tmp:
            workload = fix489.LiveWorkload(client=FakeClient(), output=pathlib.Path(tmp))
            workload.add_pending_ingest(
                {
                    "access_zone_code": "4871",
                    "access_zone_id": "zone-a",
                    "document_id": "doc-accepted",
                    "document_version": 1,
                    "operation_id": "fix487b-op-004-ingest_version",
                    "indexed_response": {},
                }
            )

            finalized = workload.finalize_pending_ingests(phase="unit")

            self.assertEqual(len(finalized), 1)
            self.assertEqual(len(workload.pending_ingests), 0)
            self.assertEqual(len(workload.delete_documents), 1)
            self.assertEqual(workload.delete_documents[0]["pool_state"], fix489.DELETE_READY)
            self.assertEqual(workload.client.waited[0]["evidence_path"], pathlib.Path(tmp) / "readiness" / "pending-unit-0000")
            self.assertEqual(workload.client.activated[0]["document_id"], "doc-accepted")

    def test_delete_or_expire_does_not_reuse_deleted_pool_documents(self):
        class FakeClient:
            def __init__(self):
                self.index_calls = 0
                self.deleted: list[str] = []

            def index_text(self, **kwargs):
                self.index_calls += 1
                document_id = f"doc-{self.index_calls}"
                return {"document_id": document_id, "response": {"document": {"accessZoneId": "zone", "documentId": document_id, "documentVersion": 1}}}

            def wait_vector_sync(self, **kwargs):
                return {"status": {"state": "OPERATION_STATE_READY_TO_ACTIVATE"}}

            def activate_document(self, **kwargs):
                return {"activated": True}

            def delete_document_vectors(self, *, access_zone_id, document_id, document_version, reason):
                self.deleted.append(document_id)
                return {"operation": {"state": "OPERATION_STATE_ACCEPTED"}}

        client = FakeClient()
        workload = fix489.LiveWorkload(client=client, output=pathlib.Path("/tmp/fix489/delete-no-reuse"))
        workload.prepare_documents(count=1)
        workload.prepare_delete_documents(count=2)
        ops = [
            fix489.ScheduledOperation(f"op-{idx}", idx, "DELETE_OR_EXPIRE", "4871", "PUBLIC", "doc", idx)
            for idx in range(2)
        ]
        for op in ops:
            workload.execute_sync(op)
        self.assertEqual(client.deleted, ["doc-2", "doc-3"])
        self.assertTrue(all(row["pool_state"] == fix489.DELETE_SCHEDULED for row in workload.delete_documents[:2]))

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

    def test_postgres_audit_uses_actual_binding_schema(self):
        source = (ROOT / "scripts" / "fix489_live_capacity.py").read_text(encoding="utf-8")
        audit_source = source[source.index("def postgres_audit") : source.index("def qdrant_audit")]
        self.assertIn("cache_entry_id", audit_source)
        self.assertIn("sequence_no", audit_source)
        self.assertNotIn("ordinal_in_parent", audit_source)
        self.assertNotIn("document_versions WHERE source_uri", audit_source)
        self.assertIn("c.metadata->>'fix489'='true'", audit_source)
        self.assertIn("c.metadata->>'fix487b'='true'", audit_source)
        self.assertNotIn("vector_bindings_v004\n  GROUP BY access_zone_id, chunk_id, representation_type, model_version", audit_source)

    def test_capacity_script_preserves_specific_terminal_failure_reasons(self):
        source = (ROOT / "scripts" / "fix487bc-capacity-campaign.sh").read_text(encoding="utf-8")
        self.assertIn('REASON="CAPACITY_PLAN_FAILED"', source)
        self.assertIn('REASON="LIVE_CAPACITY_RUN_FAILED"', source)
        self.assertIn('REASON="CAPACITY_EVIDENCE_VERIFICATION_FAILED"', source)

    def test_cleanup_target_removes_runtime_and_default_compose(self):
        makefile = (ROOT / "Makefile").read_text(encoding="utf-8")
        cleanup = makefile[makefile.index("fix487bc-cleanup:") : makefile.index("verify-fix489-live-capacity-contracts:")]
        self.assertIn("scripts/local-demo/stop-runtime.sh", cleanup)
        self.assertIn("docker compose -p astravector -f docker-compose.yml down", cleanup)
        self.assertIn("docker compose -p astravector_fix487b -f docker-compose.fix487b.yml down", cleanup)

    def test_debug_document_chunk_sql_groups_correlated_identity_columns(self):
        source = (ROOT / "src" / "grpc" / "mod.rs").read_text(encoding="utf-8")
        debug_source = source[source.index("async fn debug_document_state") : source.index("let binding_rows")]
        self.assertIn("GROUP BY c.id,c.access_zone_id,c.document_id,c.document_version", debug_source)
        self.assertIn("c.created_at ORDER BY c.created_at", debug_source)

    def test_prepare_paths_capture_vector_readiness_evidence(self):
        source = (ROOT / "scripts" / "fix489_live_capacity.py").read_text(encoding="utf-8")
        self.assertIn('"readiness" / f"prepared-{len(prepared):04d}"', source)
        self.assertIn('"readiness" / f"delete-pool-{index:04d}"', source)

    def test_readiness_diagnostics_mode_captures_runs_without_capacity_ladder(self):
        source = (ROOT / "scripts" / "fix489_live_capacity.py").read_text(encoding="utf-8")
        self.assertIn("--readiness-diagnostics-output", source)
        self.assertIn("run-a-one-document", source)
        self.assertIn("run-b-nine-documents", source)
        self.assertIn("run-c-repeat-status.jsonl", source)
        diagnostics_source = source[source.index("def run_readiness_diagnostics") : source.index("def run_operation_smoke")]
        self.assertNotIn("execute_level(", diagnostics_source)
        self.assertNotIn("capacity_levels()", diagnostics_source)

    def test_official_capacity_levels_remain_default(self):
        self.assertEqual(fix489.capacity_levels(), (5, 10, 15, 20, 25, 50))

    def test_vector_sync_timeout_covers_observed_cpu_qdrant_finalize_lag(self):
        self.assertEqual(fix489.DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS, 270)
        self.assertGreater(fix489.DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS, 224)
        self.assertLessEqual(fix489.DEFAULT_FIX489_VECTOR_SYNC_TIMEOUT_SECONDS, 300)

    def test_delete_pool_size_is_bounded_by_expected_delete_share(self):
        pool_size = fix489.expected_delete_pool_size((5, 10, 15, 20, 25, 50), 600, 180)
        self.assertGreaterEqual(pool_size, 100)
        self.assertLess(pool_size, 250)

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
