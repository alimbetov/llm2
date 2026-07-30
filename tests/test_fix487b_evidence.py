import json
import tempfile
import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487b_evidence as evidence


class Fix487BEvidenceTests(unittest.TestCase):
    def test_missing_file_fails_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = evidence.build_manifest(Path(tmp))
            self.assertEqual(manifest["status"], "FAIL")
            self.assertIn("bootstrap.json", manifest["missing"])

    def test_mandatory_artifacts_pass_and_hash_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in evidence.MANDATORY_ARTIFACTS:
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text("{}\n", encoding="utf-8")
            manifest = evidence.build_manifest(root)
            self.assertEqual(manifest["status"], "PASS")
            ok, errors = evidence.verify_manifest(root, manifest)
            self.assertTrue(ok)
            (root / evidence.MANDATORY_ARTIFACTS[0]).write_text("changed\n", encoding="utf-8")
            ok, errors = evidence.verify_manifest(root, manifest)
            self.assertFalse(ok)
            self.assertTrue(any(error.startswith("hash_mismatch:") for error in errors))

    def test_terminal_status_preserves_exit_code(self):
        with tempfile.TemporaryDirectory() as tmp:
            evidence.write_terminal_status(Path(tmp), "FAIL", 17, "TEST")
            payload = json.loads((Path(tmp) / "terminal-status.json").read_text(encoding="utf-8"))
            self.assertEqual(payload["exit_code"], 17)
            self.assertEqual(payload["reason"], "TEST")


if __name__ == "__main__":
    unittest.main()
