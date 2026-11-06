"""Offline ordering, cleanup and sanitized-evidence tests; no sockets or DB."""
from contextlib import ExitStack, contextmanager
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from journey import new_result
from journey_process import JourneyError, installation_identity
from journey_upgrade import apply_with_interruptions, run_bound, run_upgrade_journey
from journey_upgrade_runtime import Runtime, incompatible_refusal
from test_journey_upgrade_state import manifest


@contextmanager
def supplied(value):
    yield value


class UpgradeEntryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="m-up-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.previous = Mock(binding={"manifest_sha256": "a" * 64})
        self.candidate = Mock(binding={"manifest_sha256": "b" * 64})
        self.cluster = Mock(created=[])
        self.old_manifest, self.new_manifest = manifest(), manifest("0.2.0", "b", 2)

    @contextmanager
    def mocked(self, failure=None):
        with ExitStack() as stack:
            stack.enter_context(patch("journey_upgrade.Archive", side_effect=[self.candidate, self.previous]))
            stack.enter_context(patch("journey_upgrade.verified_inputs", return_value=supplied((self.root, self.old_manifest))))
            stack.enter_context(patch("journey_upgrade.sha256", return_value="a" * 64))
            stack.enter_context(patch("journey_upgrade.InstalledKit"))
            connect = stack.enter_context(patch("journey_upgrade.owned_cluster", return_value=self.cluster))
            run = stack.enter_context(patch("journey_upgrade.run_bound", side_effect=failure, return_value=None))
            yield connect, run

    def run_journey(self):
        return run_upgrade_journey(self.root, self.new_manifest, self.root, "b" * 64, self.root / "predecessor")

    def test_success_keeps_all_cases_not_run_and_records_both_bindings(self):
        with self.mocked() as (_, run):
            report = self.run_journey()
        self.assertEqual(report["state"], "INCOMPLETE")
        self.assertFalse(report["qualified"])
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["full_cases"]))
        self.assertEqual(report["predecessor_binding"], self.previous.binding)
        self.assertEqual(report["binding"], self.candidate.binding)
        self.cluster.cleanup.assert_called_once()
        run.assert_called_once()

    def test_private_failure_is_sanitized_and_owned_database_always_cleaned(self):
        with self.mocked(RuntimeError("postgres://private-password/operator-payload")):
            report = self.run_journey()
        self.assertEqual(report["state"], "FAILED")
        self.assertNotIn("private-password", json.dumps(report))
        self.assertNotIn("operator-payload", json.dumps(report))
        self.cluster.cleanup.assert_called_once()
        self.assertEqual(report["checks"][-1]["state"], "PASSED")

    def test_missing_real_schema_extension_fails_before_install_or_database(self):
        self.new_manifest = manifest("0.2.0", "b", 1)
        with self.mocked() as (connect, run):
            report = self.run_journey()
        self.assertEqual(report["failure_code"], "UPGRADE_REQUIRES_STRICT_MIGRATION_EXTENSION")
        connect.assert_not_called()
        run.assert_not_called()
        self.previous.select.assert_not_called()
        self.candidate.select.assert_not_called()

    def test_overlong_control_path_fails_before_archive_execution_or_fixture(self):
        self.root /= "x" * 90
        self.root.mkdir(mode=0o700)
        with patch("journey_upgrade.Archive") as archive, patch("journey_upgrade.owned_cluster") as cluster:
            report = self.run_journey()
        self.assertEqual(report["failure_code"], "UPGRADE_CONTROL_PATH_TOO_LONG")
        archive.assert_not_called()
        cluster.assert_not_called()

    def test_actual_acceptance_path_budget_supports_direct_linux_and_macos_roots(self):
        for parent in ("/tmp", "/private/tmp"):
            sandbox = Path(parent) / "mike-acceptance-12345678"
            _, direct = installation_identity(sandbox / "u/r/config.toml")
            _, nested = installation_identity(sandbox / "upgrade-12345678/u/r/config.toml")
            self.assertLessEqual(len(str(direct.parent / "control.sock").encode()), 103)
            self.assertGreater(len(str(nested.parent / "control.sock").encode()), 103)
            self.root = sandbox / "upgrade-12345678"
            with patch("journey_upgrade.plain_path"), patch("journey_upgrade.Archive") as archive, \
                    patch("journey_upgrade.owned_cluster") as cluster, patch.object(Path, "mkdir") as mkdir:
                report = self.run_journey()
            self.assertEqual(report["failure_code"], "UPGRADE_CONTROL_PATH_TOO_LONG")
            archive.assert_not_called()
            cluster.assert_not_called()
            mkdir.assert_not_called()

    def test_cleanup_failure_cannot_be_reported_as_success(self):
        self.cluster.cleanup.side_effect = RuntimeError("private cleanup diagnostics")
        with self.mocked():
            report = self.run_journey()
        self.assertEqual(report["state"], "FAILED")
        self.assertEqual(report["checks"][-1]["state"], "FAILED")
        self.assertNotIn("private cleanup", json.dumps(report))

    def test_missing_installed_predecessor_kit_fails_before_database_connection(self):
        with self.mocked() as (connect, run), patch("journey_upgrade.InstalledKit") as kit:
            kit.return_value.recheck.side_effect = JourneyError("MAINTENANCE_KIT_NOT_IN_MANIFEST")
            report = self.run_journey()
        self.assertEqual(report["state"], "FAILED")
        self.assertEqual(report["failure_code"], "MAINTENANCE_KIT_NOT_IN_MANIFEST")
        connect.assert_not_called()
        run.assert_not_called()


class UpgradeOrderingTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="m-up-bound-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.result = new_result({"manifest_sha256": "b" * 64})
        self.cluster = Mock(created=["owned"])
        self.cluster.create.return_value = "owned"
        self.cluster.sql.return_value = "0"
        self.cluster.config_url.return_value = "postgres://postgres@127.0.0.1:6543/owned?sslmode=disable"
        self.cluster.service.side_effect = lambda path, _: path
        self.previous, self.candidate = Mock(), Mock()
        self.previous.prefix = self.candidate.prefix = self.root / "p"
        self.events = []

    @contextmanager
    def orchestration(self, *, old_changed=False):
        runtime = Mock()
        runtime.check.side_effect = lambda phase, suffix, fn: fn()
        def serve(archive, phase, **kwargs):
            self.events.append(phase)
            if kwargs.get("while_ready"):
                kwargs["while_ready"]()
        runtime.serve.side_effect = serve
        old = [{"version": 1, "all_fields": "retained"}]
        new = ([{"version": 1, "all_fields": "changed"}] if old_changed else old) + [{"version": 2}]
        def event(name, value=None):
            def action(*_args):
                self.events.append(name)
                return value
            return action
        with ExitStack() as stack:
            stack.enter_context(patch("journey_upgrade.reliability_fixture", return_value=supplied((Mock(), 12004))))
            stack.enter_context(patch("journey_upgrade.reserved_ports", return_value=supplied(([12001, 12002, 12003], [Mock()] * 3))))
            stack.enter_context(patch("journey_upgrade.Runtime", return_value=runtime))
            stack.enter_context(patch("journey_upgrade.read_ledger", side_effect=[old, new, new]))
            stack.enter_context(patch("journey_upgrade.migration_ledger"))
            stack.enter_context(patch("journey_upgrade.predecessor_backup", side_effect=event("backup", ({"owned": True}, {"verified": True}))))
            stack.enter_context(patch("journey_upgrade.tree_digest", return_value="backup-digest"))
            stack.enter_context(patch("journey_upgrade.apply_with_interruptions", side_effect=event("interruptions-and-retry")))
            stack.enter_context(patch("journey_upgrade.registration_roundtrip", side_effect=event("registration-removal")))
            stack.enter_context(patch("journey_upgrade.refuse_install", side_effect=event("installer-refusal")))
            stack.enter_context(patch("journey_upgrade.incompatible_refusal", side_effect=event("runtime-refusal")))
            stack.enter_context(patch("journey_upgrade.retained_reinstall", side_effect=event("retained-reinstall")))
            yield runtime

    def test_capture_backup_crash_retry_migrate_new_capture_refusal_reinstall_order(self):
        with self.orchestration():
            run_bound(self.previous, self.candidate, self.root, self.result, self.cluster)
        self.assertEqual(self.events, ["predecessor_isolation", "predecessor_capture", "backup", "interruptions-and-retry",
            "candidate_capture", "registration-removal", "installer-refusal", "runtime-refusal", "retained-reinstall", "reinstalled_continuity"])
        self.cluster.create.assert_called_once_with(seeded=False)
        self.assertFalse(self.result["qualified"])
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in self.result["full_cases"]))

    def test_rewritten_old_ledger_stops_before_downgrade_and_uninstall(self):
        with self.orchestration(old_changed=True), self.assertRaisesRegex(JourneyError, "REWROTE_PREDECESSOR"):
            run_bound(self.previous, self.candidate, self.root, self.result, self.cluster)
        self.assertNotIn("installer-refusal", self.events)
        self.assertNotIn("retained-reinstall", self.events)
        self.assertEqual(self.result["state"], "FAILED")

    def test_nonempty_fixture_stops_before_any_runtime_or_backup(self):
        self.cluster.sql.return_value = "1"
        with self.orchestration() as runtime, self.assertRaisesRegex(JourneyError, "NEW_EMPTY_OWNED"):
            run_bound(self.previous, self.candidate, self.root, self.result, self.cluster)
        runtime.serve.assert_not_called()
        self.assertEqual(self.events, [])

    def test_interruption_keeps_old_pair_before_swap_and_new_pair_after_swap(self):
        runtime = Mock()
        runtime.check.side_effect = lambda phase, suffix, fn: fn()
        self.candidate.select.return_value = {"status": "unchanged"}
        with patch("journey_upgrade.durable_snapshot", return_value={"all": "stable"}), \
                patch("journey_upgrade.interrupted_install") as interrupt:
            apply_with_interruptions(runtime, self.previous, self.candidate, {}, Path("/service"), self.root)
        self.assertEqual([call.args[2] for call in interrupt.call_args_list], ["before_swap", "after_swap"])
        self.previous.recheck.assert_called_once()
        self.candidate.recheck.assert_called_once()
        self.candidate.bind.assert_called_once_with(self.previous.prefix)
        self.candidate.select.assert_called_once_with(self.previous.prefix)

    def test_mutation_after_first_interruption_prevents_retry(self):
        runtime = Mock()
        runtime.check.side_effect = lambda phase, suffix, fn: fn()
        with patch("journey_upgrade.durable_snapshot", side_effect=[{"data": "old"}, {"data": "mutated"}]), \
                patch("journey_upgrade.interrupted_install") as interrupt, \
                self.assertRaisesRegex(JourneyError, "INTERRUPTION_CHANGED_DATA"):
            apply_with_interruptions(runtime, self.previous, self.candidate, {}, Path("/service"), self.root)
        self.assertEqual(interrupt.call_count, 1)
        self.candidate.select.assert_not_called()


class IncompatibleRefusalTests(unittest.TestCase):
    def setUp(self):
        self.runtime = Mock(config=Path("/owned/config"), home=Path("/owned/home"), cwd=Path("/owned/cwd"))
        self.previous = Mock(paths={"mike": Path("/owned/old/mike")})
        self.candidate = Mock()
        self.child = Mock(returncode=1)
        self.child.communicate.return_value = (b"", b"migration 2 was previously applied but is missing in the resolved migrations")
        self.process = self.enterContext(patch("journey_upgrade_runtime.OwnedProcess", return_value=self.child))
        self.snapshot = self.enterContext(patch("journey_upgrade_runtime.durable_snapshot", return_value={"data": "same"}))

    def run_refusal(self):
        return incompatible_refusal(self.runtime, self.previous, self.candidate, {}, Path("/owned/service"))

    def test_recognized_refusal_requires_unchanged_full_durable_snapshot_and_owned_cleanup(self):
        value = self.run_refusal()
        self.assertTrue(value["complete_logical_database_dump_unchanged"])
        self.assertTrue(value["ephemeral_lifecycle_and_logs_excluded"])
        self.previous.recheck.assert_called_once_with(active=False)
        self.child.close.assert_called_once()
        self.assertEqual(self.process.call_args.kwargs["env"]["PATH"], "")

    def test_environment_failure_never_counts_as_incompatible_schema_refusal(self):
        self.child.communicate.return_value = (b"", b"AF_UNIX operation not permitted")
        with self.assertRaisesRegex(JourneyError, "WRONG_REFUSAL_REASON"):
            self.run_refusal()
        self.child.close.assert_called_once()

    def test_success_signal_exit_timeout_and_any_data_mutation_fail(self):
        for code in (0, -9):
            self.child.returncode = code
            with self.subTest(code=code), self.assertRaisesRegex(JourneyError, "DID_NOT_REFUSE"):
                self.run_refusal()
        self.child.returncode = 1
        self.child.communicate.side_effect = subprocess.TimeoutExpired("private", 60)
        with self.assertRaisesRegex(JourneyError, "PREDECESSOR_TIMEOUT"):
            self.run_refusal()
        self.child.communicate.side_effect = None
        self.snapshot.side_effect = [{"data": "same"}, {"data": "changed"}]
        with self.assertRaisesRegex(JourneyError, "MUTATED_STATE"):
            self.run_refusal()


if __name__ == "__main__":
    unittest.main()
