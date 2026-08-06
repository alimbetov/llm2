import importlib.util
import json
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


live_client = load_module("astravector_live_client_for_contracts", ROOT / "scripts" / "astravector_live_client.py")


class AstraVectorLiveClientContracts(unittest.TestCase):
    def test_grpc_status_ready_state_is_required(self):
        self.assertFalse(
            live_client.document_vector_status_ready(
                {
                    "status": {
                        "state": "OPERATION_STATE_SYNCING",
                        "sync": {
                            "expectedBindings": 3,
                            "syncedBindings": 3,
                            "outboxCompleted": 3,
                            "outboxFailed": 0,
                            "qdrantPointsFound": 3,
                        },
                    }
                }
            )
        )
        self.assertTrue(live_client.document_vector_status_ready({"status": {"state": "OPERATION_STATE_READY_TO_ACTIVATE"}}))

    def test_qdrant_collections_ready_accepts_modern_shape(self):
        self.assertTrue(live_client.qdrant_collections_response_is_ready({"result": {"collections": []}}))
        self.assertTrue(live_client.qdrant_collections_response_is_ready({"collections": []}))
        self.assertFalse(live_client.qdrant_collections_response_is_ready({}))

    def test_ready_status_has_no_blockers(self):
        snapshot = live_client.normalize_vector_status(
            {
                "status": {
                    "state": "OPERATION_STATE_READY_TO_ACTIVATE",
                    "readyToActivate": True,
                    "sync": {
                        "expectedBindings": 3,
                        "syncedBindings": 3,
                        "outboxCompleted": 3,
                        "qdrantPointsExpected": 3,
                        "qdrantPointsFound": 3,
                        "denseVectorsExpected": 3,
                        "denseVectorsFound": 3,
                    },
                }
            }
        )
        self.assertEqual(live_client.vector_readiness_blockers(snapshot), [])

    def test_binding_outbox_qdrant_and_dense_blockers_are_specific(self):
        base = {
            "state": "OPERATION_STATE_SYNCING",
            "ready_to_activate": False,
            "expected_bindings": 3,
            "synced_bindings": 2,
            "deleted_bindings": 1,
            "dense_vectors_expected": 3,
            "dense_vectors_found": 2,
            "outbox_pending": 1,
            "outbox_completed": 2,
            "qdrant_collection_exists": True,
            "qdrant_points_expected": 3,
            "qdrant_points_found": 2,
            "qdrant_points_missing": 1,
        }
        blockers = "\n".join(live_client.vector_readiness_blockers(base))
        self.assertIn("BINDINGS_NOT_SYNCED:expected_bindings=3,synced_bindings=2,deleted_bindings=1", blockers)
        self.assertIn("DELETED_BINDINGS_INCLUDED:deleted_bindings=1,expected_bindings=3", blockers)
        self.assertIn("DENSE_VECTOR_COUNT_MISMATCH:dense_vectors_expected=3,dense_vectors_found=2", blockers)
        self.assertIn("OUTBOX_PENDING:outbox_pending=1", blockers)
        self.assertIn("QDRANT_POINTS_MISSING:qdrant_points_missing=1", blockers)

    def test_sparse_mismatch_is_only_blocker_when_sparse_required(self):
        snapshot = {
            "state": "OPERATION_STATE_SYNCING",
            "ready_to_activate": False,
            "expected_bindings": 3,
            "synced_bindings": 3,
            "outbox_completed": 3,
            "qdrant_collection_exists": True,
            "qdrant_points_expected": 3,
            "qdrant_points_found": 3,
            "sparse_vectors_expected": 3,
            "sparse_vectors_found": 2,
        }
        self.assertNotIn("SPARSE_VECTOR_COUNT_MISMATCH", "\n".join(live_client.vector_readiness_blockers(snapshot)))
        self.assertIn("SPARSE_VECTOR_COUNT_MISMATCH", "\n".join(live_client.vector_readiness_blockers(snapshot, sparse_required=True)))

    def test_timeout_preserves_last_response_and_creates_diagnostics(self):
        class FakeClient(live_client.AstraVectorLiveClient):
            def __init__(self):
                super().__init__(grpc_addr="127.0.0.1:0", database_url="postgres://fake", qdrant_url="http://127.0.0.1:0")
                self.calls = 0

            def vector_status(self, **kwargs):
                self.calls += 1
                return {
                    "status": {
                        "state": "OPERATION_STATE_SYNCING",
                        "message": "waiting for qdrant",
                        "sync": {
                            "expectedBindings": 3,
                            "syncedBindings": 3,
                            "outboxCompleted": 3,
                            "qdrantPointsExpected": 3,
                            "qdrantPointsFound": 2,
                            "qdrantPointsMissing": 1,
                        },
                    }
                }

            def inspect_document_vector_state(self, **kwargs):
                return {"binding_rows": [{"qdrant_sync_status": "SYNCED", "binding_count": 3}]}

            def qdrant_document_diagnostic(self, **kwargs):
                return {"status": "MEASURED", "point_count": 2}

            def debug_document(self, **kwargs):
                return {"status": "UNAVAILABLE"}

        with tempfile.TemporaryDirectory() as tmp:
            evidence = pathlib.Path(tmp)
            with self.assertRaisesRegex(RuntimeError, "VECTOR_SYNC_QDRANT_MISMATCH"):
                FakeClient().wait_vector_sync(
                    access_zone_id="00000000-0000-0000-0000-000000000001",
                    document_id="00000000-0000-0000-0000-000000000002",
                    document_version=1,
                    timeout_seconds=0,
                    evidence_path=evidence,
                )
            polls = [json.loads(line) for line in (evidence / "vector-sync-polls.jsonl").read_text(encoding="utf-8").splitlines()]
            self.assertEqual(polls[-1]["event"], "timeout")
            self.assertIn("QDRANT_POINTS_MISSING", "\n".join(polls[-1]["blockers"]))
            self.assertTrue((evidence / "postgres-document-diagnostic.json").is_file())
            self.assertTrue((evidence / "qdrant-document-diagnostic.json").is_file())
            self.assertTrue((evidence / "debug-document-response.json").is_file())

    def test_final_non_extending_poll_can_return_ready_at_deadline_boundary(self):
        class FakeClient(live_client.AstraVectorLiveClient):
            def __init__(self):
                super().__init__(grpc_addr="127.0.0.1:0", database_url="postgres://fake", qdrant_url="http://127.0.0.1:0")

            def vector_status(self, **kwargs):
                return {"status": {"state": "OPERATION_STATE_READY_TO_ACTIVATE", "readyToActivate": True}}

        with tempfile.TemporaryDirectory() as tmp:
            result = FakeClient().wait_vector_sync(
                access_zone_id="00000000-0000-0000-0000-000000000001",
                document_id="00000000-0000-0000-0000-000000000002",
                document_version=1,
                timeout_seconds=0,
                evidence_path=pathlib.Path(tmp),
            )
            self.assertTrue(live_client.document_vector_status_ready(result))
            events = (pathlib.Path(tmp) / "vector-sync-polls.jsonl").read_text(encoding="utf-8")
            self.assertIn("READY_AT_DEADLINE_BOUNDARY", events)


if __name__ == "__main__":
    unittest.main()
