import json
import tempfile
import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487bc_capacity_evidence as evidence


class Fix489CapacityEvidenceContracts(unittest.TestCase):
    def test_manifest_uses_campaign_levels_and_requires_real_proof_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "campaign-manifest.json").write_text(
                json.dumps({"levels": [{"concurrency": 5}, {"concurrency": 25}, {"concurrency": 50}]}) + "\n",
                encoding="utf-8",
            )
            paths = [str(path.relative_to(root)) for path in evidence.expected_paths(root)]
            self.assertIn("levels/concurrency-5/warmup-operations.jsonl", paths)
            self.assertIn("levels/concurrency-25/retrieval-controls.jsonl", paths)
            self.assertIn("levels/concurrency-50/qdrant-after-cooldown.json", paths)
            self.assertNotIn("levels/concurrency-100/level-result.json", paths)


if __name__ == "__main__":
    unittest.main()
