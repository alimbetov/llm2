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


if __name__ == "__main__":
    unittest.main()
