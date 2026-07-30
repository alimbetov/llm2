import tempfile
import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487bc_capacity_evidence as evidence


class Fix487BCCapacityEvidenceTests(unittest.TestCase):
    def test_missing_evidence_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = evidence.build_manifest(Path(tmp))
            self.assertEqual(manifest["status"], "FAIL")
            self.assertIn("campaign-manifest.json", manifest["missing"])

    def test_complete_capacity_evidence_passes_and_hashes_everything(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for path in evidence.expected_paths(root):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("{}\n", encoding="utf-8")
            manifest = evidence.build_manifest(root)
            self.assertEqual(manifest["status"], "PASS")
            self.assertEqual(len(manifest["artifacts"]), len(evidence.expected_paths(root)))
            self.assertTrue(all("sha256" in row for row in manifest["artifacts"]))

    def test_each_capacity_level_has_required_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths = [str(path.relative_to(root)) for path in evidence.expected_paths(root)]
            for level in evidence.LEVELS:
                self.assertIn(f"levels/concurrency-{level}/level-result.json", paths)
                self.assertIn(f"levels/concurrency-{level}/resource-samples.jsonl", paths)


if __name__ == "__main__":
    unittest.main()
