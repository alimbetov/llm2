import json
import os
import tempfile
import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487bc_capacity_campaign as campaign


def stable_metrics(level: int) -> dict:
    return {
        "completed_operations": campaign.MIN_COMPLETED[level],
        "minimum_completed_operations": campaign.MIN_COMPLETED[level],
        "success_rate": 0.999,
        "resource_exhausted_rate": 0.0,
        "deadline_exceeded_rate": 0.0,
        "cooldown_reached": True,
        "queues_bounded": True,
        "memory_behavior_stable": True,
    }


class Fix487BCCapacityCampaignTests(unittest.TestCase):
    def tearDown(self):
        os.environ.pop("FIX489_RUN_EXTREME_LEVELS", None)
        os.environ.pop("FIX489_CAPACITY_LEVELS", None)

    def test_campaign_plan_has_required_levels_durations_and_seeds(self):
        os.environ.pop("FIX489_CAPACITY_LEVELS", None)
        plan = campaign.campaign_plan()
        self.assertEqual([row["concurrency"] for row in plan], [5, 10, 15, 20, 25, 50])
        self.assertEqual([row["seed"] for row in plan], [489005, 489010, 489015, 489020, 489025, 489050])
        self.assertTrue(all(row["measurement_seconds"] == 600 for row in plan))
        self.assertTrue(all(row["load_warmup_seconds"] == 180 for row in plan))
        self.assertEqual(plan[0]["minimum_completed_operations"], 300)
        self.assertEqual(plan[-1]["minimum_completed_operations"], 1400)

    def test_capacity_levels_can_be_shortened_for_local_targeted_runs(self):
        os.environ["FIX489_CAPACITY_LEVELS"] = "5"
        plan = campaign.campaign_plan()
        self.assertEqual([row["concurrency"] for row in plan], [5])
        self.assertEqual(plan[0]["seed"], 489005)

    def test_extreme_levels_are_optional(self):
        os.environ["FIX489_RUN_EXTREME_LEVELS"] = "true"
        self.assertEqual([row["concurrency"] for row in campaign.campaign_plan()], [5, 10, 15, 20, 25, 50, 100, 200])

    def test_write_plan_preserves_concurrency_5_waiver_without_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = campaign.write_plan(Path(tmp))
            waiver = manifest["concurrency_5_pilot"]
            self.assertEqual(waiver["code"], "CONCURRENCY_5_PILOT_WAIVED_BY_PRODUCT_OWNER")
            self.assertEqual(waiver["historical_status_preserved"], "FIX487B_CONCURRENCY_5_PILOT_BLOCKED")
            self.assertNotIn("PASS", json.dumps(waiver))

    def test_stable_level_classification(self):
        verdict, reason = campaign.classify_level(stable_metrics(25))
        self.assertEqual(verdict, "STABLE")
        self.assertIsNone(reason)

    def test_controlled_saturation_classification(self):
        metrics = stable_metrics(100)
        metrics.update(
            {
                "completed_operations": 900,
                "success_rate": 0.97,
                "resource_exhausted_rate": 0.02,
                "controlled_saturation": True,
                "cooldown_reached": True,
                "queues_bounded": True,
                "memory_behavior_stable": True,
            }
        )
        verdict, reason = campaign.classify_level(metrics)
        self.assertEqual(verdict, "SATURATED_CONTROLLED")
        self.assertIsNone(reason)

    def test_hard_gate_failure_wins(self):
        metrics = stable_metrics(25)
        metrics["cross_zone_leakage_count"] = 1
        self.assertEqual(campaign.classify_level(metrics), ("FAILED", "cross_zone_leakage_count"))

    def test_capacity_curve_selects_recommendation(self):
        curve = campaign.capacity_curve(
            [
                {"concurrency": 5, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 10, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 15, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 20, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 25, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
                {"concurrency": 50, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
            ]
        )
        self.assertEqual(curve["capacity_scope"], "LOCAL_MAC_CPU")
        self.assertFalse(curve["production_capacity_claim"])
        self.assertEqual(curve["maximum_stable_concurrency"], 20)
        self.assertEqual(curve["first_controlled_saturation_concurrency"], 25)
        self.assertEqual(curve["recommended_operating_concurrency"], 15)
        self.assertTrue(curve["local_capacity_campaign_pass"])


if __name__ == "__main__":
    unittest.main()
