import importlib.util
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("test_tiers", ROOT / "scripts/lib/test_tiers.py")
TIERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TIERS)


class TestTierContracts(unittest.TestCase):
    def setUp(self):
        self.spec = json.loads((ROOT / ".config/test-tiers.json").read_text())

    def test_current_source_has_no_fixture_consumers_in_reviewed_fast_scopes(self):
        errors, records = TIERS.inspect(ROOT, self.spec)
        self.assertEqual(errors, [])
        self.assertGreater(sum(bool(row["fixture_markers"]) for row in records), 400)
        self.assertEqual(TIERS.validate_config(ROOT, self.spec), [])

    def test_direct_and_indirect_fixture_calls_fail_fast_scope_inventory(self):
        for source in ["TestDb::bootstrap().await?", "acquire_ephemeral_db().await?",
                       "use crate::db_helpers::start_proxy_with_db;",
                       "use crate::filter_proxy::start_proxy_with_rules;",
                       "use crate::store::test_support::*;",
                       "use crate::judge_worker::ownership_test_support::Fixture;"]:
            with self.subTest(source=source), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                path = root / "crates/mike/src/tui/new_test.rs"
                path.parent.mkdir(parents=True)
                path.write_text(source)
                spec = {**self.spec, "fixture_free_exceptions": {}}
                errors, _ = TIERS.inspect(root, spec)
                self.assertEqual(len(errors), 1, errors)
                self.assertIn("fast tier", errors[0])

    def test_unreviewed_mike_module_or_harness_defaults_to_required_db(self):
        for path in ["crates/mike/src/new_feature/tests.rs",
                     "crates/mike/tests/new_harness.rs",
                     "crates/mike/tests/cli/new_db_command.rs"]:
            self.assertFalse(TIERS.source_is_fast(path, self.spec), path)

    def test_known_db_consumers_and_colocated_pure_tests_are_db(self):
        for path in ["crates/mike/src/api/fleet/report_http_tests.rs",
                     "crates/mike/src/store/persistence/test_support/provisioning_tests.rs",
                     "crates/mike/src/steerer/pure_test.rs",
                     "crates/mike/tests/proxy/filter_rules_edge_cases.rs",
                     "crates/mike/tests/cli/eval_import_holdout.rs",
                     "crates/mike/tests/cli/eval_withdraw_batch.rs",
                     "crates/mike/tests/integration/migration_checksums.rs"]:
            self.assertFalse(TIERS.source_is_fast(path, self.spec), path)

    def test_database_free_checksum_and_metadata_helpers_remain_fast(self):
        for path in ["crates/mike/tests/schema/migration_checksums.rs",
                     "crates/mike/tests/release_contracts.rs",
                     "crates/mike/tests/cli/version.rs",
                     "crates/mike/src/tui/input.rs"]:
            self.assertTrue(TIERS.source_is_fast(path, self.spec), path)

    def test_fast_facade_cannot_path_include_a_database_fixture_module(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            target = root / "crates/mike/src/store/tests.rs"
            target.parent.mkdir(parents=True)
            target.write_text("TestDb::bootstrap().await?;")
            facade = root / "crates/mike/tests/release_contracts.rs"
            facade.parent.mkdir(parents=True)
            facade.write_text('#[path = "../src/store/tests.rs"]\nmod hidden_fixture;')
            spec = {**self.spec, "fixture_free_exceptions": {}}
            errors, _ = TIERS.inspect(root, spec)
            self.assertTrue(any("path-includes DB fixture" in error for error in errors), errors)

    def test_fixture_free_exception_is_invalidated_by_any_source_change(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            relative = next(iter(self.spec["fixture_free_exceptions"]))
            path = root / relative
            path.parent.mkdir(parents=True)
            path.write_text("TestDb::bootstrap().await?; // now actually uses DB")
            errors, _ = TIERS.inspect(root, self.spec)
            self.assertTrue(any("exception changed" in error for error in errors), errors)

    def test_selected_db_lane_fails_before_building_without_explicit_fixture(self):
        for value in [None, "", "  "]:
            env = {} if value is None else {"MIKE_TEST_ADMIN_URL": value}
            result = subprocess.run([sys.executable, str(ROOT / "scripts/lib/test_tiers.py"),
                                     "--require-db"], env=env, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("required DB tier NOT RUN", result.stdout)


if __name__ == "__main__":
    unittest.main()
