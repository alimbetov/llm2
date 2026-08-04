import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPTS = sorted((ROOT / "scripts" / "local-demo").glob("*.sh"))


class Fix488LocalScriptsTests(unittest.TestCase):
    def test_all_shell_scripts_use_strict_mode(self):
        self.assertGreater(len(SCRIPTS), 0)
        for script in SCRIPTS:
            text = script.read_text(encoding="utf-8")
            self.assertIn("set -Eeuo pipefail", text, script)

    def test_no_script_hardcodes_user_home(self):
        for script in SCRIPTS:
            text = script.read_text(encoding="utf-8")
            self.assertNotIn("/Users/ruslanalimbetov", text, script)

    def test_no_committed_secret_or_placeholder_success(self):
        checked = list(SCRIPTS) + [
            ROOT / ".env.local-demo.example",
            ROOT / "config" / "application-local-demo.yaml",
        ]
        for path in checked:
            text = path.read_text(encoding="utf-8")
            self.assertNotIn("SIMULATED_OK", text, path)
            self.assertNotIn("PLACEHOLDER_PASS", text, path)
            self.assertNotIn("password=", text.lower(), path)

    def test_postgres_audit_uses_current_chunk_schema(self):
        helper = (ROOT / "scripts" / "astravector_live_client.py").read_text(encoding="utf-8")
        demo = (ROOT / "scripts" / "local-demo" / "local_demo.py").read_text(encoding="utf-8")
        self.assertIn("AstraVectorLiveClient", demo)
        self.assertIn("SELECT granularity AS chunk_granularity", helper)
        self.assertNotIn("SELECT chunk_granularity, count(*) FROM astravector.content_chunks_v004", helper)

    def test_postgres_audit_scopes_outbox_through_bindings(self):
        helper = (ROOT / "scripts" / "astravector_live_client.py").read_text(encoding="utf-8")
        self.assertIn("JOIN astravector.vector_bindings_v004 b", helper)
        self.assertIn("b.id=o.binding_id", helper)
        self.assertIn("b.access_zone_id=o.binding_access_zone_id", helper)
        self.assertNotIn("FROM astravector.vector_outbox o, ids WHERE o.access_zone_id", helper)


if __name__ == "__main__":
    unittest.main()
