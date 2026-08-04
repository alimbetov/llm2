import importlib.util
import json
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "local-demo" / "local_demo.py"
spec = importlib.util.spec_from_file_location("local_demo", MODULE_PATH)
local_demo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(local_demo)


class Fix488LocalEvidenceTests(unittest.TestCase):
    def test_manifest_fails_when_required_artifact_is_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = local_demo.evidence_manifest(pathlib.Path(tmp))
        self.assertEqual(manifest["status"], "FAIL")
        self.assertIn("environment.json", manifest["missing"])

    def test_manifest_rejects_dry_run_marker(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in local_demo.REQUIRED_EVIDENCE:
                root.joinpath(name).write_text("{}", encoding="utf-8")
            root.joinpath("local-e2e-result.json").write_text(
                json.dumps({"status": "DRY_RUN_ONLY"}), encoding="utf-8"
            )
            manifest = local_demo.evidence_manifest(root)
        self.assertEqual(manifest["status"], "FAIL")
        self.assertIn("local-e2e-result.json", manifest["bad_markers"])

    def test_manifest_passes_when_all_required_artifacts_exist(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in local_demo.REQUIRED_EVIDENCE:
                root.joinpath(name).write_text("{}", encoding="utf-8")
            manifest = local_demo.evidence_manifest(root)
        self.assertEqual(manifest["status"], "PASS")
        self.assertEqual(manifest["missing"], [])
        self.assertEqual(manifest["bad_markers"], [])


if __name__ == "__main__":
    unittest.main()

