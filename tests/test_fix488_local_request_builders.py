import hashlib
import importlib.util
import json
import pathlib
import uuid
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "local-demo" / "local_demo.py"
spec = importlib.util.spec_from_file_location("local_demo", MODULE_PATH)
local_demo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(local_demo)


class Fix488LocalRequestBuilderTests(unittest.TestCase):
    def test_russian_text_json_roundtrip_is_preserved(self):
        text = "AstraVector хранит каноническое состояние документов в PostgreSQL."
        blocks = local_demo.make_blocks(text)
        encoded = json.dumps({"blocks": blocks}, ensure_ascii=False)
        decoded = json.loads(encoded)
        paragraphs = [b for b in decoded["blocks"] if b["blockType"] == "BLOCK_TYPE_PARAGRAPH"]
        self.assertEqual(paragraphs[0]["text"], text)

    def test_sha256_text_matches_standard_hashlib(self):
        text = "ASTRAVECTOR_LOCAL_DEMO_2026\nРусский текст"
        self.assertEqual(local_demo.sha256_text(text), hashlib.sha256(text.encode("utf-8")).hexdigest())

    def test_block_ids_are_valid_deterministic_uuids(self):
        text = "Первый абзац.\n\nВторой абзац."
        first = local_demo.make_blocks(text)
        second = local_demo.make_blocks(text)
        self.assertEqual(first, second)
        for block in first:
            uuid.UUID(block["blockId"])

    def test_logical_blocks_include_exactly_one_document_root(self):
        blocks = local_demo.make_blocks("Первый абзац.\n\nВторой абзац.")
        roots = [b for b in blocks if b["blockType"] == "BLOCK_TYPE_DOCUMENT"]
        self.assertEqual(len(roots), 1)
        root_id = roots[0]["blockId"]
        children = [b for b in blocks if b["blockType"] == "BLOCK_TYPE_PARAGRAPH"]
        self.assertGreater(len(children), 0)
        self.assertTrue(all(child["parentBlockId"] == root_id for child in children))

    def test_grpcurl_json_names_are_camel_case(self):
        text = "Текст для проверки JSON names."
        block = local_demo.make_blocks(text)[0]
        self.assertIn("blockId", block)
        self.assertIn("blockType", block)
        self.assertIn("orderIndex", block)
        self.assertIn("sourceLocation", block)
        self.assertNotIn("block_id", block)
        self.assertNotIn("order_index", block)

    def test_qdrant_114_collections_shape_is_ready(self):
        self.assertTrue(
            local_demo.qdrant_collections_response_is_ready(
                {"result": {"collections": [{"name": "astravector_v004"}]}, "status": "ok"}
            )
        )
        self.assertTrue(local_demo.qdrant_collections_response_is_ready({"collections": []}))
        self.assertFalse(local_demo.qdrant_collections_response_is_ready({"status": "ok"}))

    def test_vector_sync_complete_uses_canonical_counters_not_missing_wrapper_flag(self):
        self.assertTrue(
            local_demo.vector_sync_is_complete(
                {
                    "expectedBindings": 21,
                    "syncedBindings": 21,
                    "outboxCompleted": 21,
                    "qdrantPointsFound": 21,
                }
            )
        )
        self.assertFalse(
            local_demo.vector_sync_is_complete(
                {
                    "expectedBindings": 21,
                    "syncedBindings": 21,
                    "outboxCompleted": 20,
                    "qdrantPointsFound": 21,
                }
            )
        )

    def test_vector_sync_complete_defers_to_server_ready_to_activate(self):
        self.assertFalse(
            local_demo.vector_sync_is_complete(
                {
                    "readyToActivate": False,
                    "expectedBindings": 6,
                    "syncedBindings": 6,
                    "outboxCompleted": 6,
                    "qdrantPointsFound": 6,
                }
            )
        )
        self.assertTrue(local_demo.vector_sync_is_complete({"readyToActivate": True}))

    def test_document_vector_status_ready_uses_facade_ready_to_activate(self):
        self.assertFalse(
            local_demo.document_vector_status_ready(
                {
                    "status": {
                        "state": "OPERATION_STATE_SYNCING",
                        "readyToActivate": False,
                        "sync": {
                            "expectedBindings": 6,
                            "syncedBindings": 6,
                            "outboxCompleted": 6,
                            "qdrantPointsFound": 6,
                        },
                    }
                }
            )
        )
        self.assertFalse(
            local_demo.document_vector_status_ready(
                {
                    "status": {
                        "state": "OPERATION_STATE_SYNCING",
                        "sync": {
                            "expectedBindings": 6,
                            "syncedBindings": 6,
                            "outboxCompleted": 6,
                            "qdrantPointsFound": 6,
                        },
                    }
                }
            )
        )
        self.assertTrue(
            local_demo.document_vector_status_ready({"status": {"state": "OPERATION_STATE_READY_TO_ACTIVATE"}})
        )
        self.assertTrue(local_demo.document_vector_status_ready({"status": {"readyToActivate": True}}))


if __name__ == "__main__":
    unittest.main()
