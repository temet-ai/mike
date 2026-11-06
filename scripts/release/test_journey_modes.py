"""Offline installed-mode orchestration and truthful report contracts.

Synthetic/mocked observations are helper tests only. No PostgreSQL, daemon or
AF_UNIX control executes here, and these tests cannot qualify any AC case.
"""
from contextlib import ExitStack, contextmanager
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import Mock, patch

from common import sha256
from journey import create_config, new_result
from journey_fixture import Fixture
from journey_modes import (LITERAL_INPUT, capture_continuity, cli_setup, literal_database_url, refused_setup, run_bound,
                           run_isolated, run_mode_journey, run_sequence, saved_mode, stopped_doctor, transition)
from journey_process import JourneyError, private_write
from test_journey_persistent import synthetic_inventory


class ModeConfigurationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mm-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.config, self.home, self.cwd = create_config(self.root / "m", [11001, 11002, 11003], 11004)

    def setup_report(self):
        backup = self.config.parent / (self.config.name + ".backup-offline")
        private_write(backup, self.config.read_bytes())
        return {"persistent": False, "config_file": str(self.config),
                "previous_config_backup": str(backup), "verified_running": True}

    def test_cli_setup_requires_exact_report_mode_selected_file_and_private_backup(self):
        value = self.setup_report()
        with patch("journey_modes.setup_command", return_value=(0, json.dumps(value).encode(), b"")) as command:
            observation = cli_setup(Path("/installed/mike"), self.config, self.home, self.cwd)
        self.assertEqual(command.call_args.args[-1], ["--mode", "trial", "--accept-trial-loss", "--json"])
        self.assertTrue(observation["previous_private_config_preserved"])
        self.assertNotIn(str(self.config), json.dumps(observation))
        for changed in ({"persistent": True}, {"verified_running": False}, {"config_file": "/foreign/config"},
                        {"previous_config_backup": "/foreign/backup"}):
            with self.subTest(changed=changed), patch("journey_modes.setup_command", return_value=(
                    0, json.dumps(dict(value, **changed)).encode(), b"")):
                with self.assertRaises(JourneyError):
                    cli_setup(Path("/installed/mike"), self.config, self.home, self.cwd)

    def test_database_url_stays_in_private_file_not_arguments(self):
        value = self.setup_report()
        value["persistent"] = True
        url_file = self.root / "database-url"
        private_write(url_file, b"postgres://PRIVATE_SENTINEL@127.0.0.1:6432/owned\n")
        with patch("journey_modes.setup_command", return_value=(0, json.dumps(value).encode(), b"")) as command:
            observation = cli_setup(Path("/installed/mike"), self.config, self.home, self.cwd, url_file)
        self.assertEqual(command.call_args.args[-1][-2:], ["--database-url-file", str(url_file)])
        self.assertNotIn("PRIVATE_SENTINEL", str(command.call_args) + json.dumps(observation))

    def test_refusals_require_expected_diagnostic_and_unchanged_files(self):
        outputs = [(1, b"", b"Repeat with --accept-trial-loss to proceed"),
                   (1, b"", b"flags override the requested storage mode or URL")]
        with patch("journey_modes.setup_command", side_effect=outputs):
            result = refused_setup(Path("/installed/mike"), self.config, self.home, self.cwd)
        self.assertEqual(len(result["refused_before_change"]), 2)
        for output in ((0, b"", outputs[0][2]), (-9, b"", outputs[0][2]), (1, b"", b"PRIVATE crash")):
            with patch("journey_modes.setup_command", return_value=output):
                with self.assertRaisesRegex(JourneyError, "EXPECTED_REFUSAL"):
                    refused_setup(Path("/installed/mike"), self.config, self.home, self.cwd)
        def mutation(*_args, **_kwargs):
            self.config.write_text("persist_enabled = true\n")
            return outputs[0]
        with patch("journey_modes.setup_command", side_effect=mutation):
            with self.assertRaisesRegex(JourneyError, "REFUSAL_CHANGED"):
                refused_setup(Path("/installed/mike"), self.config, self.home, self.cwd)

    def test_literal_url_and_explicit_overrides_preserve_exact_private_inputs(self):
        url = literal_database_url("postgres://postgres@127.0.0.1:6432/owned?sslmode=disable")
        for character in ("$", "'", '"', "\\", "`"):
            self.assertIn(character, url)
        url_file = self.root / "database-url"
        private_write(url_file, (url + "\n").encode())
        def refused(_binary, _config, _home, _cwd, args, **kwargs):
            if "--accept-trial-loss" not in args:
                diagnostic = b"Repeat with --accept-trial-loss"
            elif args[-1].endswith("malformed-literal-url"):
                diagnostic = b"provide a postgres:// or postgresql:// URL"
            else:
                diagnostic = b"override the requested storage mode or URL"
            return 1, b"", diagnostic
        with patch("journey_modes.setup_command", side_effect=refused) as command:
            result = refused_setup(Path("/installed/mike"), self.config, self.home, self.cwd, url_file)
        self.assertEqual(command.call_count, 6)
        self.assertEqual(len(result["refused_before_change"]), 6)
        self.assertNotIn(LITERAL_INPUT, json.dumps(result))
        self.assertEqual(url_file.read_bytes(), (url + "\n").encode())
        self.config.write_text("persist_enabled = true\ndb_fallback_enabled = false\ndatabase_url = "
                               + json.dumps(url) + "\n")
        saved = saved_mode(self.config, True, url)
        self.assertEqual(saved["database_url_sha256"], hashlib.sha256(url.encode()).hexdigest())
        self.assertNotIn(LITERAL_INPUT, json.dumps(saved))

    def test_literal_refusal_diagnostics_must_not_echo_planted_input(self):
        with patch("journey_modes.setup_command", return_value=(
                1, b"", b"Repeat with --accept-trial-loss " + LITERAL_INPUT.encode())):
            with self.assertRaisesRegex(JourneyError, "LITERAL_LEAK"):
                refused_setup(Path("/installed/mike"), self.config, self.home, self.cwd)

    def test_stopped_doctor_must_not_start_or_adopt_a_daemon(self):
        context = {"selected": Mock(), "paths": {"mike": Path("/installed/mike")},
                   "config": self.config, "home": self.home, "cwd": self.cwd, "manifest": {}}
        context["selected"].held.side_effect = [False, False]
        with patch("journey_modes.doctor", return_value={"daemon_state": "stopped"}) as doctor:
            self.assertEqual(stopped_doctor(context, True)["daemon_state"], "stopped")
            self.assertEqual(doctor.call_args.kwargs, {"persistent": True, "stopped": True})
        context["selected"].held.side_effect = [False, True]
        with patch("journey_modes.doctor", return_value={}):
            with self.assertRaisesRegex(JourneyError, "DOCTOR_STARTED"):
                stopped_doctor(context, False)

    def test_saved_mode_and_retained_database_url_are_checked_independently(self):
        self.assertFalse(saved_mode(self.config, False)["saved_persistent"])
        url = "postgres://postgres@127.0.0.1:6432/owned?sslmode=disable"
        self.config.write_text("persist_enabled = false\ndatabase_url = " + json.dumps(url) + "\n")
        self.assertTrue(saved_mode(self.config, False, url)["durable_database_url_retained"])
        with self.assertRaisesRegex(JourneyError, "SAVED_MODE"):
            saved_mode(self.config, True, url)
        self.config.write_text("persist_enabled = true\ndatabase_url = " + json.dumps(url) + "\n")
        with self.assertRaisesRegex(JourneyError, "FALLBACK"):
            saved_mode(self.config, True, url)
        with self.assertRaisesRegex(JourneyError, "DATABASE_CHANGED"):
            saved_mode(self.config, True, "foreign")

    def test_changed_capture_id_body_or_ledger_cannot_pass_continuity(self):
        capture = {"request_id": 7, "session_id": "fixture", "request_body_sha256": "a" * 64,
                   "response_body_sha256": "b" * 64}
        ledger = {"full_ledger_sha256": "c" * 64}
        self.assertTrue(capture_continuity(capture, capture, ledger, ledger)["no_duplicate_capture"])
        for key, value in (("request_id", 8), ("session_id", "different"), ("request_body_sha256", "d" * 64),
                           ("response_body_sha256", "d" * 64)):
            with self.subTest(key=key), self.assertRaisesRegex(JourneyError, "CAPTURE_CHANGED"):
                capture_continuity(dict(capture, **{key: value}), capture, ledger, ledger)
        with self.assertRaisesRegex(JourneyError, "LEDGER_CHANGED"):
            capture_continuity(capture, capture, {}, ledger)


class ModeOrchestrationTests(unittest.TestCase):
    def binding(self):
        return {"mike": Path("/offline/mike"), "mike-mcp": Path("/offline/mike-mcp")}, {"manifest_sha256": "a" * 64}

    @contextmanager
    def sequence(self, *, changed_capture=False, surface="cli"):
        paths, binding = self.binding()
        context = {"paths": paths, "manifest": {}, "result": new_result(binding), "ports": [1, 2, 3],
                   "surface": surface, "cwd": Path("/first"), "second_cwd": Path("/second"),
                   "config": Path("/private/config"), "home": Path("/private/home"), "upstream_port": 4,
                   "url": "postgres://owned", "selected": Mock(), "generations": []}
        ledger = {"full_ledger_sha256": "c" * 64}
        capture = {"request_id": 7, "session_id": "fixture", "request_body_sha256": "a" * 64,
                   "response_body_sha256": "b" * 64}
        observed = dict(capture, request_id=8) if changed_capture else dict(capture)
        transitions = []
        def mode_transition(context, persistent, phase, database_known):
            transitions.append((persistent, phase, database_known, context["cwd"]))
            context["generations"].append(phase)
        with ExitStack() as stack:
            stack.enter_context(patch("journey_modes.transition", side_effect=mode_transition))
            for name in ("disable_features", "verify_features", "saved_mode", "verify_service_prerequisites", "stopped_doctor"):
                stack.enter_context(patch("journey_modes." + name, return_value={"mocked": True}))
            stack.enter_context(patch("journey_modes.tui_restart", return_value=({"owned_pty": True},
                                {"instance": {"boot_generation": "tui_restart"}})))
            stack.enter_context(patch("journey_modes.migration_ledger", return_value=ledger))
            read = stack.enter_context(patch("journey_modes.stored_capture", side_effect=[capture, observed, observed]))
            empty = stack.enter_context(patch("journey_modes.http_requests", return_value=[]))
            stream = stack.enter_context(patch("journey_modes.stream_capture", return_value={"mocked": True}))
            stack.enter_context(patch("journey_modes.inspect_capture", return_value=(1, {"mocked": True})))
            yield context, transitions, read, empty, stream

    def test_sequence_drives_actual_surfaces_and_preserves_durable_identity(self):
        for surface in ("cli", "tui"):
            with self.subTest(surface=surface), self.sequence(surface=surface) as values:
                context, transitions, read, empty, stream = values
                observation = run_sequence(context, Mock(), "owned", synthetic_inventory(), Fixture())
            self.assertEqual([(mode, phase) for mode, phase, _, _ in transitions][:5], [
                (False, "initial_trial"), (True, "first_database"), (True, "database_isolation_restart"),
                (False, "return_trial"), (True, "return_database")])
            if surface == "cli":
                self.assertEqual(transitions[-1][1:], ("database_restart", True, Path("/second")))
            self.assertEqual(observation["distinct_boot_generations"], 6)
            self.assertEqual(read.call_count, 3)
            self.assertEqual(read.call_args.kwargs["expected_id"], 7)
            self.assertEqual(empty.call_count, 2)
            self.assertEqual(stream.call_count, 2)
            self.assertFalse(context["result"]["qualified"])
            self.assertTrue(all(case["state"] == "NOT_RUN" for case in context["result"]["full_cases"]))

    def test_changed_capture_after_return_fails_before_restart(self):
        with self.sequence(changed_capture=True) as values:
            context, transitions, *_ = values
            with self.assertRaisesRegex(JourneyError, "CAPTURE_CHANGED"):
                run_sequence(context, Mock(), "owned", synthetic_inventory(), Fixture())
            self.assertEqual(transitions[-1][1], "return_database")
            self.assertEqual(context["result"]["state"], "FAILED")

    def test_mode_transition_rechecks_hashes_and_rejects_reused_generation(self):
        paths, binding = self.binding()
        context = {"paths": paths, "manifest": {}, "result": new_result(binding), "surface": "cli",
                   "config": Path("/private/config"), "home": Path("/private/home"), "cwd": Path("/private/cwd"),
                   "url": "postgres://owned", "url_file": Path("/private/url"), "upstream_port": 4,
                   "selected": Mock(), "generations": ["existing"]}
        context["selected"].ready.return_value = {"instance": {"boot_generation": "existing"}}
        with patch("journey_modes.verify_binding", return_value=(paths, binding)), \
                patch("journey_modes.cli_setup", return_value={}) as setup, \
                patch("journey_modes.saved_mode", return_value={}), \
                patch("journey_modes.verify_service_prerequisites"), patch("journey_modes.doctor") as doctor:
            with self.assertRaisesRegex(JourneyError, "GENERATION_REUSED"):
                transition(context, True, "return_database", True)
        setup.assert_called_once()
        doctor.assert_not_called()
        with patch("journey_modes.verify_binding", side_effect=JourneyError("MODE_INSTALLED_BINDING_CHANGED")), \
                patch("journey_modes.cli_setup") as setup:
            with self.assertRaisesRegex(JourneyError, "BINDING_CHANGED"):
                transition(context, True, "return_database", True)
        setup.assert_not_called()

    def test_report_failure_is_sanitized_and_fixture_cleanup_runs(self):
        cluster = Mock()
        with patch("journey_modes.verify_binding", return_value=self.binding()), \
                patch("journey_modes.verified_migration_inventory", return_value=synthetic_inventory()), \
                patch("journey_modes.owned_cluster", return_value=cluster), \
                patch("journey_modes.run_isolated", side_effect=ValueError("PRIVATE_DATABASE_URL")):
            report = run_mode_journey("/offline", {}, "/offline", "a" * 64)
        cluster.cleanup.assert_called_once_with()
        self.assertEqual(report["state"], "FAILED")
        self.assertNotIn("PRIVATE_DATABASE_URL", json.dumps(report))
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in report["full_cases"]))

    def test_default_skips_tui_and_success_is_still_incomplete(self):
        for tui in (False, True):
            with patch("journey_modes.verify_binding", return_value=self.binding()), \
                    patch("journey_modes.verified_migration_inventory", return_value=synthetic_inventory()), \
                    patch("journey_modes.owned_cluster"), \
                    patch("journey_modes.run_isolated", return_value={"mocked": True}) as run:
                report = run_mode_journey("/offline", {}, "/offline", "a" * 64, tui=tui)
            self.assertEqual(run.call_count, 2 if tui else 1)
            self.assertEqual(report["state"], "INCOMPLETE")
            self.assertFalse(report["qualified"])
            if not tui:
                self.assertEqual(next(item["state"] for item in report["checks"]
                                     if item["id"] == "archive_mode_tui_sequence"), "NOT_RUN")

    def test_invalid_binding_never_allocates_database(self):
        with patch("journey_modes.verify_binding", side_effect=JourneyError("INSTALLED_BINARY_HASH_MISMATCH")), \
                patch("journey_modes.owned_cluster") as cluster:
            report = run_mode_journey("/offline", {}, "/offline", "a" * 64)
        cluster.assert_not_called()
        self.assertEqual(report["failure_code"], "INSTALLED_BINARY_HASH_MISMATCH")

    def test_runtime_failure_still_stops_and_stop_failure_marks_private_state_retained(self):
        @contextmanager
        def upstream():
            yield Fixture(), 11004
        @contextmanager
        def ports():
            yield [11001, 11002, 11003], [Mock(), Mock(), Mock()]
        for stop_error in (False, True):
            with self.subTest(stop_error=stop_error), tempfile.TemporaryDirectory(prefix="mc-") as temporary:
                root = Path(temporary) / "m"
                selected = Mock()
                selected.root = root / "not-created"
                if stop_error:
                    selected.stop.side_effect = JourneyError("SELECTED_COOPERATIVE_STOP_TIMEOUT")
                cluster = Mock()
                cluster.create.return_value = "owned"
                cluster.sql.return_value = "0"
                cluster.config_url.return_value = "postgres://postgres@127.0.0.1:6432/owned?sslmode=disable"
                paths, binding = self.binding()
                report = new_result(binding)
                with patch("journey_modes.upstream_fixture", upstream), patch("journey_modes.reserved_ports", ports), \
                        patch("journey_modes.SelectedInstallation", return_value=selected), \
                        patch("journey_modes.refused_setup", return_value={"mocked": True}), \
                        patch("journey_modes.run_sequence", side_effect=JourneyError("MODE_DURABLE_CAPTURE_CHANGED")):
                    with self.assertRaisesRegex(JourneyError, "STOP_TIMEOUT" if stop_error else "CAPTURE_CHANGED"):
                        run_bound(paths, {"source_commit": "b" * 40}, root, report, cluster,
                                  synthetic_inventory(), surface="cli")
                selected.stop.assert_called_once_with()
                if stop_error:
                    self.assertEqual(report["retained_private_fixtures"][0]["path"], str(root.parent))
                else:
                    self.assertNotIn("retained_private_fixtures", report)

    def test_unverified_daemon_cleanup_retains_exact_private_state_path(self):
        retained = []
        def failed_bound(paths, manifest, root, result, cluster, inventory, *, surface):
            retained.append(root.parent)
            result["retained_private_fixtures"] = [{"path": str(root.parent)}]
            raise JourneyError("SELECTED_COOPERATIVE_STOP_TIMEOUT")
        result = new_result({})
        try:
            with patch("journey_modes.run_bound", side_effect=failed_bound):
                with self.assertRaisesRegex(JourneyError, "STOP_TIMEOUT"):
                    run_isolated({}, {}, result, Mock(), {}, surface="cli")
            self.assertEqual(len(retained), 1)
            self.assertTrue(retained[0].is_dir())
        finally:
            for path in retained:
                shutil.rmtree(path)

    def test_normal_or_prelaunch_failure_removes_only_its_created_fixture(self):
        roots = []
        def failed_bound(paths, manifest, root, result, cluster, inventory, *, surface):
            roots.append(root.parent)
            raise ValueError("no runtime launched")
        with patch("journey_modes.run_bound", side_effect=failed_bound):
            with self.assertRaises(ValueError):
                run_isolated({}, {}, new_result({}), Mock(), {}, surface="cli")
        self.assertTrue(all(not path.exists() for path in roots))


if __name__ == "__main__":
    unittest.main()
