import unittest
import tempfile
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487c_soak as soak


class Fix487CSoakTests(unittest.TestCase):
    def test_soak_concurrency_is_75_percent_floor(self):
        self.assertEqual(soak.soak_concurrency(1), 1)
        self.assertEqual(soak.soak_concurrency(2), 1)
        self.assertEqual(soak.soak_concurrency(3), 2)
        self.assertEqual(soak.soak_concurrency(4), 3)
        self.assertEqual(soak.soak_concurrency(25), 18)
        self.assertEqual(soak.soak_concurrency(50), 37)
        self.assertEqual(soak.soak_concurrency(100), 75)
        self.assertEqual(soak.soak_concurrency(200), 150)

    def test_no_stable_capacity_blocks_soak(self):
        plan = soak.plan_from_capacity({"maximum_stable_concurrency": None})
        self.assertEqual(plan["status"], "BLOCKED")
        self.assertEqual(plan["reason"], "NO_STABLE_CAPACITY_LEVEL")

    def test_capacity_result_creates_sixty_minute_plan(self):
        plan = soak.plan_from_capacity({"maximum_stable_concurrency": 50})
        self.assertEqual(plan["status"], "READY")
        self.assertEqual(plan["soak_concurrency"], 37)
        self.assertEqual(plan["measurement_seconds"], 3600)
        self.assertEqual(plan["seed"], 487460)

    def test_r3_capacity_result_creates_local_stable_floor_soak_plan(self):
        plan = soak.plan_from_capacity(
            {"campaign_mode": "LOCAL_STABLE_FLOOR_DISCOVERY", "maximum_stable_concurrency": 3}
        )
        self.assertEqual(plan["status"], "READY")
        self.assertEqual(plan["soak_mode"], "LOCAL_STABLE_FLOOR")
        self.assertEqual(plan["soak_concurrency"], 2)
        self.assertEqual(plan["load_warmup_seconds"], 300)
        self.assertEqual(plan["measurement_seconds"], 3600)
        self.assertEqual(plan["cooldown_max_seconds"], 900)

    def test_soak_pass_classification(self):
        verdict, reason = soak.classify_soak(
            {
                "success_rate": 0.999,
                "sample_completeness_ratio": 0.99,
                "cooldown_reached": True,
            }
        )
        self.assertEqual(verdict, "PASS")
        self.assertIsNone(reason)

    def test_soak_hard_gate_failure_wins(self):
        verdict, reason = soak.classify_soak(
            {
                "success_rate": 0.999,
                "sample_completeness_ratio": 0.99,
                "cooldown_reached": True,
                "dead_letters": 1,
            }
        )
        self.assertEqual(verdict, "FAILED")
        self.assertEqual(reason, "dead_letters")

    def test_soak_evidence_manifest_fails_missing_and_passes_complete(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertEqual(soak.verify_soak_evidence(root)["status"], "FAIL")
            for name in soak.SOAK_ARTIFACTS:
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text("{}\n", encoding="utf-8")
            manifest = soak.verify_soak_evidence(root)
            self.assertEqual(manifest["status"], "PASS")
            self.assertTrue((root / "evidence-manifest.json").is_file())


if __name__ == "__main__":
    unittest.main()
