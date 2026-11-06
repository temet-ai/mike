"""Offline CLI/report wiring contracts; never execute candidate, DB or network."""
from contextlib import contextmanager
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import acceptance
from common import ReleaseError
from journey import new_result
from journey_reliability import new_reliability_report, reliability_policy
from test_journey_reliability import OfflineTests


class ReliabilityCliTests(OfflineTests):
    base = ["release-acceptance", "--candidate", "/tmp/candidate", "--output", "/tmp/evidence"]

    def test_default_does_not_opt_into_reliability(self):
        with patch.object(sys, "argv", self.base), patch.object(acceptance, "run_acceptance", return_value=2) as run:
            self.assertEqual(acceptance.main(), 2)
        self.assertIs(run.call_args.kwargs["reliability"], False)
        self.assertIsNone(run.call_args.kwargs["reliability_duration_seconds"])
        self.assertIsNone(run.call_args.kwargs["reliability_concurrent_sessions"])

    def test_orphan_invalid_and_under_minimum_cli_options_fail_before_output_or_candidate(self):
        choices = [
            ["--reliability-duration-seconds", "900"],
            ["--reliability-concurrent-sessions", "8"],
            ["--reliability-stage", "M1"],
            ["--reliability-stage", "M2", "--reliability-duration-seconds", "3600"],
            ["--reliability-journey", "--reliability-duration-seconds", "0"],
            ["--reliability-journey", "--reliability-duration-seconds", "899.9"],
            ["--reliability-journey", "--reliability-duration-seconds", "nan"],
            ["--reliability-journey", "--reliability-duration-seconds", "inf"],
            ["--reliability-journey", "--reliability-duration-seconds", "7201"],
            ["--reliability-journey", "--reliability-concurrent-sessions", "0"],
            ["--reliability-journey", "--reliability-concurrent-sessions", "33"],
            ["--reliability-journey", "--reliability-concurrent-sessions", "8.0"],
        ]
        for flags in choices:
            with self.subTest(flags=flags), patch.object(sys, "argv", self.base + flags), patch("sys.stderr"), \
                 patch.object(acceptance, "run_acceptance") as run, patch.object(acceptance, "output_directory") as output:
                with self.assertRaises(SystemExit) as error:
                    acceptance.main()
                self.assertEqual(error.exception.code, 2)
            run.assert_not_called()
            output.assert_not_called()

    def test_valid_explicit_values_are_preserved_without_requiring_other_journeys(self):
        flags = ["--reliability-journey", "--reliability-duration-seconds", "3601.5",
                 "--reliability-concurrent-sessions", "9"]
        with patch.object(sys, "argv", self.base + flags), patch.object(acceptance, "run_acceptance", return_value=2) as run:
            self.assertEqual(acceptance.main(), 2)
        self.assertEqual(run.call_args.args[2:4], (False, False))
        self.assertIs(run.call_args.kwargs["reliability"], True)
        self.assertEqual(run.call_args.kwargs["reliability_duration_seconds"], 3601.5)
        self.assertEqual(run.call_args.kwargs["reliability_concurrent_sessions"], 9)

    def test_programmatic_invalid_inputs_fail_before_output_or_candidate_reads(self):
        for options in ({"reliability_duration_seconds": 0}, {"reliability_concurrent_sessions": 0},
                        {"reliability": True, "reliability_duration_seconds": 0},
                        {"reliability": True, "reliability_duration_seconds": 899.9},
                        {"reliability": True, "reliability_concurrent_sessions": 0},
                        {"reliability": True, "reliability_concurrent_sessions": 33}):
            with self.subTest(options=options), patch.object(acceptance, "output_directory") as output, \
                 patch.object(acceptance, "verified_inputs") as inputs:
                with self.assertRaises(ReleaseError):
                    acceptance.run_acceptance(Path("/offline/candidate"), Path("/offline/output"), **options)
                with self.assertRaises(ReleaseError):
                    acceptance.run_helpers(Path("/offline/candidate"), acceptance.new_report(), **options)
            output.assert_not_called()
            inputs.assert_not_called()


class ReliabilityAcceptanceTests(OfflineTests):
    @contextmanager
    def helpers(self, *, fail=False):
        manifest = {"version": "0.0.1", "source_commit": "a" * 40, "target": "offline",
                    "artifacts": [{"binary": name, "archive": name + ".tar.gz", "sha256": "b" * 64, "smoke": {}}
                                  for name in ("mike", "mike-mcp")]}
        order, selected_sandboxes = [], []
        @contextmanager
        def verified(_candidate):
            yield Path("/offline/snapshot"), manifest
        def persistent(*_args, **_options):
            order.append("baseline-cleaned")
            return new_result({})
        def browser(*_args):
            order.append("browser-cleaned")
            return new_result({})
        def reliability(binary_dir, actual_manifest, sandbox, digest, **options):
            self.assertEqual(actual_manifest, manifest)
            self.assertEqual(digest, "c" * 64)
            self.assertEqual(binary_dir.name, "bin")
            self.assertTrue(sandbox.name.startswith("reliability-"))
            self.assertTrue(sandbox.is_dir())
            self.assertEqual(sandbox.stat().st_mode & 0o777, 0o700)
            self.assertNotEqual(sandbox, binary_dir.parent.parent)
            selected_sandboxes.append(sandbox)
            order.append("reliability")
            result = new_result({"manifest_sha256": digest})
            result["reliability"] = new_reliability_report(reliability_policy(
                options["duration_seconds"], options["concurrent_sessions"]))
            result["checks"] = [{"id": "offline_reliability_contract", "state": "FAILED" if fail else "PASSED"}]
            result["limits"] = ["synthetic isolation only; full AC20 NOT_RUN"]
            if fail:
                result.update(state="FAILED", failure_code="RELIABILITY_CAPTURE_COUNTS")
            return result
        with patch.object(acceptance, "verified_inputs", side_effect=verified), \
             patch.object(acceptance, "sha256", return_value="c" * 64), \
             patch.object(acceptance, "install_verified", return_value={"status": "unchanged"}), \
             patch.object(acceptance, "smoke", return_value={}), \
             patch.object(acceptance, "run_persistent_journey", side_effect=persistent), \
             patch.object(acceptance, "run_installed_browser_journey", side_effect=browser), \
             patch.object(acceptance, "run_reliability_journey", side_effect=reliability) as run:
            yield run, order, selected_sandboxes

    def test_default_never_calls_optional_reliability(self):
        with self.helpers() as (run, order, _):
            report = acceptance.new_report()
            acceptance.run_helpers(Path("/offline/candidate"), report)
        run.assert_not_called()
        self.assertNotIn("archive_reliability_journey", report)
        self.assertEqual(order, [])

    def test_opt_in_alone_runs_in_fresh_sandbox_and_extends_report(self):
        with self.helpers() as (run, order, sandboxes):
            report = acceptance.new_report()
            acceptance.run_helpers(Path("/offline/candidate"), report, reliability=True)
        self.assertEqual(order, ["reliability"])
        self.assertEqual(run.call_args.kwargs, {"duration_seconds": None, "concurrent_sessions": None})
        self.assertTrue(all(not path.exists() for path in sandboxes))
        self.assertEqual(report["checks"][-1]["id"], "offline_reliability_contract")
        self.assertIn("synthetic isolation only; full AC20 NOT_RUN", report["limits"])
        self.assertEqual(report["archive_reliability_journey"]["reliability"]["profile"], "synthetic_isolation_overrides")
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["cases"]))
        self.assertFalse(report["qualified"])

    def test_reliability_follows_cleaned_baseline_and_separate_browser(self):
        with self.helpers() as (run, order, _):
            report = acceptance.new_report()
            acceptance.run_helpers(Path("/offline/candidate"), report, persistent_journey=True,
                                   browser=object(), maintenance=True, reliability=True,
                                   reliability_duration_seconds=3601.5, reliability_concurrent_sessions=9)
        self.assertEqual(order, ["baseline-cleaned", "browser-cleaned", "reliability"])
        self.assertEqual(run.call_args.kwargs, {"duration_seconds": 3601.5, "concurrent_sessions": 9})

    def test_failed_helper_keeps_sanitized_nested_failure_and_cleans_sandbox(self):
        with self.helpers(fail=True) as (_, _, sandboxes):
            report = acceptance.new_report()
            with self.assertRaisesRegex(ReleaseError, "reliability journey failed"):
                acceptance.run_helpers(Path("/offline/candidate"), report, reliability=True)
        self.assertTrue(all(not path.exists() for path in sandboxes))
        self.assertEqual(report["archive_reliability_journey"]["failure_code"], "RELIABILITY_CAPTURE_COUNTS")
        self.assertEqual(report["checks"][-1]["state"], "FAILED")

    def test_acceptance_retains_not_run_and_failure_exit_semantics(self):
        for fail, expected_exit in ((False, 2), (True, 1)):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as temporary, self.helpers(fail=fail), patch("builtins.print"):
                output = Path(temporary) / "evidence"
                exit_status = acceptance.run_acceptance(Path("/offline/candidate"), output, reliability=True)
                report = json.loads((output / "result.json").read_bytes())
                self.assertEqual(exit_status, expected_exit)
                self.assertEqual(report["required_not_run"], 33)
                self.assertEqual(next(item for item in report["cases"] if item["id"] == "AC20")["state"], "NOT_RUN")
                self.assertFalse(report["qualified"])
                self.assertEqual(report["archive_reliability_journey"]["reliability"]["duration_seconds"], 0)
                self.assertEqual((output / "result.json").stat().st_mode & 0o777, 0o600)


if __name__ == "__main__":
    unittest.main()
