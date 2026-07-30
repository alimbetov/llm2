import asyncio
import unittest
from collections import Counter
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487b_mixed_load as mixed


class Fix487BMixedLoadTests(unittest.TestCase):
    def test_100_operation_cycle_has_exact_distribution(self):
        schedule = mixed.deterministic_cycle()
        self.assertEqual(len(schedule), 100)
        self.assertEqual(Counter(op.operation_type for op in schedule), mixed.OPERATION_COUNTS)

    def test_operation_ids_are_deterministic(self):
        self.assertEqual(mixed.deterministic_cycle(), mixed.deterministic_cycle())

    def test_retry_classification(self):
        for status in mixed.RETRYABLE_STATUSES:
            self.assertTrue(mixed.should_retry(status))
        for status in mixed.NON_RETRYABLE_STATUSES:
            self.assertFalse(mixed.should_retry(status))

    def test_bounded_workers_are_never_exceeded(self):
        rows = asyncio.run(mixed.run_schedule(mixed.deterministic_cycle()[:25], workers=5, dry_run=True))
        self.assertLessEqual(max(row["max_observed_concurrency"] for row in rows), 5)

    def test_queue_is_bounded_in_source_contract(self):
        source = Path(mixed.__file__).read_text(encoding="utf-8")
        self.assertIn("asyncio.Queue(maxsize=workers * 2)", source)

    def test_manifest_declares_warmup_measurement_inputs(self):
        manifest = mixed.workload_manifest(seed=487205, workers=5, client_deadline_ms=30000)
        self.assertEqual(manifest["bounded_worker_count"], 5)
        self.assertFalse(manifest["unbounded_queue"])
        self.assertEqual(manifest["operation_total"], 100)


if __name__ == "__main__":
    unittest.main()
