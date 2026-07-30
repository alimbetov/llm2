import tempfile
import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487b_dataset as dataset


class Fix487BDatasetTests(unittest.TestCase):
    def test_same_seed_produces_same_manifest(self):
        docs_a = dataset.build_documents(487205)
        docs_b = dataset.build_documents(487205)
        self.assertEqual(dataset.build_manifest(docs_a), dataset.build_manifest(docs_b))

    def test_different_seed_changes_identities(self):
        docs_a = dataset.build_documents(487205)
        docs_b = dataset.build_documents(487206)
        self.assertNotEqual(
            dataset.build_manifest(docs_a)["document_sha256_aggregate"],
            dataset.build_manifest(docs_b, 487206)["document_sha256_aggregate"],
        )

    def test_distribution_and_lifecycle_controls_present(self):
        manifest = dataset.build_manifest(dataset.build_documents())
        self.assertEqual(manifest["document_count"], 60)
        self.assertEqual(set(manifest["zone_distribution"]), {"4871", "4872", "4873"})
        self.assertEqual(set(manifest["access_level_distribution"]), set(dataset.ACCESS_LEVELS))
        self.assertGreaterEqual(manifest["language_distribution"]["RU"], 1)
        self.assertGreaterEqual(manifest["language_distribution"]["KZ"], 1)
        self.assertGreaterEqual(manifest["lifecycle_distribution"]["LEGAL_HOLD_ACTIVE"], 3)
        for state in dataset.LIFECYCLE_STATES:
            self.assertIn(state, manifest["lifecycle_distribution"])

    def test_logical_identities_are_unique(self):
        docs = dataset.build_documents()
        ids = [block["logical_block_id"] for doc in docs for block in doc["logical_blocks"]]
        self.assertEqual(len(ids), len(set(ids)))

    def test_write_dataset_creates_required_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = dataset.write_dataset(Path(tmp))
            self.assertEqual(manifest["dataset_version"], dataset.DATASET_VERSION)
            for name in (
                "dataset-manifest.json",
                "documents.jsonl",
                "operations-input.jsonl",
                "logical-to-runtime.json",
            ):
                self.assertTrue((Path(tmp) / name).is_file())


if __name__ == "__main__":
    unittest.main()
