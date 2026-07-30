import unittest
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import fix487b_audit as audit


class Fix487BAuditTests(unittest.TestCase):
    def test_clean_synthetic_audit_passes(self):
        self.assertEqual(audit.classify_integrity({})["status"], "PASS")

    def test_orphan_binding_detected(self):
        self.assertEqual(audit.classify_integrity({"orphan_binding_count": 1})["status"], "FAIL")

    def test_duplicate_identity_detected(self):
        self.assertEqual(
            audit.classify_integrity({"duplicate_canonical_identity_count": 1})["status"],
            "FAIL",
        )

    def test_cross_zone_anomaly_detected(self):
        self.assertEqual(audit.classify_integrity({"cross_zone_binding_anomaly_count": 1})["status"], "FAIL")

    def test_failed_outbox_detected(self):
        self.assertEqual(audit.classify_integrity({"failed_outbox": 1})["status"], "FAIL")

    def test_missing_qdrant_point_detected(self):
        self.assertEqual(
            audit.classify_integrity({"missing_active_qdrant_points_after_cooldown": 1})["status"],
            "FAIL",
        )

    def test_sql_and_qdrant_payload_contract_exist(self):
        self.assertIn("orphan_binding_count", audit.postgres_audit_sql())
        self.assertIn("binding_id", audit.qdrant_payload_required_fields())
        self.assertIn("chunking_profile_version", audit.qdrant_payload_required_fields())


if __name__ == "__main__":
    unittest.main()
