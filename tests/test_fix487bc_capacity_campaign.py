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
        os.environ.pop("FIX489_CAMPAIGN_MODE", None)

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

    def test_default_mode_remains_full_local_capacity(self):
        self.assertEqual(campaign.campaign_mode(), "FULL_LOCAL_CAPACITY")
        self.assertEqual(campaign.configured_capacity_levels(), (5, 10, 15, 20, 25, 50))

    def test_r3_mode_uses_local_stable_floor_levels_durations_and_seeds(self):
        os.environ["FIX489_CAMPAIGN_MODE"] = "LOCAL_STABLE_FLOOR_DISCOVERY"
        plan = campaign.campaign_plan()
        self.assertEqual([row["concurrency"] for row in plan], [1, 2, 3, 4])
        self.assertEqual([row["seed"] for row in plan], [489001, 489002, 489003, 489004])
        self.assertTrue(all(row["runtime_warmup_seconds"] == 30 for row in plan))
        self.assertTrue(all(row["load_warmup_seconds"] == 60 for row in plan))
        self.assertTrue(all(row["measurement_seconds"] == 300 for row in plan))
        self.assertTrue(all(row["cooldown_max_seconds"] == 300 for row in plan))
        self.assertEqual([row["minimum_completed_operations"] for row in plan], [100, 150, 200, 250])

    def test_full_mode_plan_remains_historical_durations(self):
        os.environ["FIX489_CAMPAIGN_MODE"] = "FULL_LOCAL_CAPACITY"
        plan = campaign.campaign_plan()
        self.assertEqual([row["concurrency"] for row in plan], [5, 10, 15, 20, 25, 50])
        self.assertTrue(all(row["runtime_warmup_seconds"] == 30 for row in plan))
        self.assertTrue(all(row["load_warmup_seconds"] == 180 for row in plan))
        self.assertTrue(all(row["measurement_seconds"] == 600 for row in plan))
        self.assertTrue(all(row["cooldown_max_seconds"] == 600 for row in plan))

    def test_full_campaign_pass_still_requires_25_and_50(self):
        rows = [
            {"concurrency": 5, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
            {"concurrency": 10, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
        ]
        self.assertFalse(campaign.local_capacity_campaign_pass(rows))

    def test_r3_pass_does_not_require_25_or_50(self):
        rows = [
            {"concurrency": 1, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
            {"concurrency": 2, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
            {"concurrency": 3, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
            {"concurrency": 4, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
        ]
        self.assertTrue(campaign.local_stable_floor_discovery_pass(rows))
        self.assertFalse(campaign.local_capacity_campaign_pass(rows))

    def test_r3_requires_at_least_one_stable_level(self):
        rows = [
            {"concurrency": 1, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
            {"concurrency": 2, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
        ]
        self.assertFalse(campaign.local_stable_floor_discovery_pass(rows))

    def test_r3_capacity_curve_selects_floor_and_recommendation(self):
        curve = campaign.capacity_curve(
            [
                {"concurrency": 1, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 2, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 3, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
                {"concurrency": 4, "verdict": "SATURATED_CONTROLLED", "hard_gate_not_measured_count": 0},
            ],
            mode="LOCAL_STABLE_FLOOR_DISCOVERY",
        )
        self.assertEqual(curve["campaign_mode"], "LOCAL_STABLE_FLOOR_DISCOVERY")
        self.assertEqual(curve["maximum_stable_concurrency"], 2)
        self.assertEqual(curve["first_controlled_saturation_concurrency"], 3)
        self.assertEqual(curve["recommended_operating_concurrency"], 1)
        self.assertTrue(curve["local_stable_floor_discovery_pass"])

    def test_r3_all_stable_curve_has_not_reached_saturation(self):
        curve = campaign.capacity_curve(
            [
                {"concurrency": 1, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 2, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 3, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
                {"concurrency": 4, "verdict": "STABLE", "hard_gate_not_measured_count": 0},
            ],
            mode="LOCAL_STABLE_FLOOR_DISCOVERY",
        )
        self.assertEqual(curve["maximum_stable_concurrency"], 4)
        self.assertEqual(curve["first_controlled_saturation_concurrency"], "NOT_REACHED")
        self.assertEqual(curve["recommended_operating_concurrency"], 3)


if __name__ == "__main__":
    unittest.main()
