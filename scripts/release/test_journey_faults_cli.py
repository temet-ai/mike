"""Socket-free fault CLI/report integration; no candidate or DB execution."""
from contextlib import contextmanager
import inspect
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch
import unittest

import acceptance
from common import ReleaseError
from journey import new_result
from test_journey_faults import Offline


class FaultCliTests(Offline):
    base = ["release-acceptance", "--candidate", "/tmp/candidate", "--output", "/tmp/evidence"]

    def test_explicit_opt_in_is_independent_and_default_false(self):
        for flags, expected in (([], False), (["--fault-journey"], True)):
            with patch.object(sys, "argv", self.base + flags), patch.object(acceptance, "run_acceptance", return_value=2) as run:
                self.assertEqual(acceptance.main(), 2)
            self.assertEqual(run.call_args.kwargs["fault_journey"], expected)
            self.assertEqual(run.call_args.args[2:4], (False, False))
            self.assertIs(run.call_args.kwargs["reliability"], False)

    def test_keyword_only_option_preserves_positional_compatibility(self):
        for function in (acceptance.run_helpers, acceptance.run_acceptance):
            option = inspect.signature(function).parameters["fault_journey"]
            self.assertEqual(option.kind, inspect.Parameter.KEYWORD_ONLY)
            self.assertIs(option.default, False)

    @contextmanager
    def helpers(self, fail=False, raises=False):
        manifest = {"version": "0.0.1", "source_commit": "a" * 40, "target": "offline",
                    "artifacts": [{"binary": name, "archive": name + ".tar.gz", "sha256": "b" * 64, "smoke": {}}
                                  for name in ("mike", "mike-mcp")]}
        order, sandboxes = [], []
        @contextmanager
        def verified(_):
            yield Path("/offline/snapshot"), manifest
        def earlier(label):
            def run(*_args, **_options):
                order.append(label)
                return new_result({})
            return run
        def fault(binary_dir, actual_manifest, sandbox, digest):
            self.assertEqual(actual_manifest, manifest)
            self.assertEqual(digest, "c" * 64)
            self.assertEqual(binary_dir.name, "bin")
            self.assertTrue(sandbox.name.startswith("faults-"))
            self.assertTrue(sandbox.is_dir())
            self.assertEqual(sandbox.stat().st_mode & 0o777, 0o700)
            self.assertNotEqual(sandbox, binary_dir.parent.parent)
            sandboxes.append(sandbox)
            order.append("faults")
            if raises:
                raise RuntimeError("synthetic secret must not be retained")
            result = new_result({"manifest_sha256": digest})
            result["checks"] = [{"id": "offline_fault_contract", "state": "FAILED" if fail else "PASSED"}]
            result["limits"] = ["Partial synthetic errors/cancellation only; full AC21 NOT_RUN"]
            if fail:
                result.update(state="FAILED", failure_code="FAULT_CANCEL_NOT_PROPAGATED")
            return result
        with patch.object(acceptance, "verified_inputs", side_effect=verified), \
             patch.object(acceptance, "sha256", return_value="c" * 64), \
             patch.object(acceptance, "install_verified", return_value={"status": "unchanged"}), \
             patch.object(acceptance, "smoke", return_value={}), \
             patch.object(acceptance, "run_persistent_journey", side_effect=earlier("persistent-cleaned")), \
             patch.object(acceptance, "run_installed_browser_journey", side_effect=earlier("browser-cleaned")), \
             patch.object(acceptance, "run_reliability_journey", side_effect=earlier("reliability-cleaned")), \
             patch.object(acceptance, "run_fault_journey", side_effect=fault) as run:
            yield run, order, sandboxes

    def test_default_never_runs_faults(self):
        with self.helpers() as (run, order, _):
            report = acceptance.new_report()
            acceptance.run_helpers(Path("/offline/candidate"), report)
        run.assert_not_called()
        self.assertEqual(order, [])
        self.assertNotIn("archive_fault_journey", report)

    def test_fault_flag_alone_uses_fresh_private_sandbox_and_extends_report(self):
        with self.helpers() as (run, order, sandboxes):
            report = acceptance.new_report()
            acceptance.run_helpers(Path("/offline/candidate"), report, fault_journey=True)
        run.assert_called_once()
        self.assertEqual(order, ["faults"])
        self.assertTrue(all(not path.exists() for path in sandboxes))
        self.assertEqual(report["checks"][-1]["id"], "offline_fault_contract")
        self.assertIn("Partial synthetic errors/cancellation only; full AC21 NOT_RUN", report["limits"])
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["cases"] if item["id"] != "AC08"))
        self.assertFalse(report["qualified"])

    def test_faults_follow_cleaned_other_fixtures(self):
        with self.helpers() as (_, order, sandboxes):
            acceptance.run_helpers(Path("/offline/candidate"), acceptance.new_report(), persistent_journey=True,
                                   browser=object(), maintenance=True, reliability=True, fault_journey=True)
        self.assertEqual(order, ["persistent-cleaned", "browser-cleaned", "reliability-cleaned", "faults"])
        self.assertTrue(all(not path.exists() for path in sandboxes))

    def test_failure_preserves_sanitized_nested_result_and_cleans_sandbox(self):
        with self.helpers(fail=True) as (_, _, sandboxes):
            report = acceptance.new_report()
            with self.assertRaisesRegex(ReleaseError, "fault journey failed"):
                acceptance.run_helpers(Path("/offline/candidate"), report, fault_journey=True)
        self.assertTrue(all(not path.exists() for path in sandboxes))
        self.assertEqual(report["archive_fault_journey"]["failure_code"], "FAULT_CANCEL_NOT_PROPAGATED")
        self.assertEqual(report["checks"][-1]["state"], "FAILED")

    def test_runtime_exception_cleans_sandbox_and_does_not_retain_raw_message(self):
        with tempfile.TemporaryDirectory() as temporary, self.helpers(raises=True) as (_, _, sandboxes), patch("builtins.print"):
            output = Path(temporary) / "evidence"
            code = acceptance.run_acceptance(Path("/offline/candidate"), output, fault_journey=True)
            text = (output / "result.json").read_text()
        self.assertEqual(code, 1)
        self.assertNotIn("synthetic secret must not be retained", text)
        self.assertTrue(all(not path.exists() for path in sandboxes))

    def test_result_exit_and_private_evidence_semantics_remain_incomplete(self):
        for fail, code in ((False, 2), (True, 1)):
            with tempfile.TemporaryDirectory() as temporary, self.helpers(fail=fail), patch("builtins.print"):
                output = Path(temporary) / "evidence"
                actual = acceptance.run_acceptance(Path("/offline/candidate"), output, fault_journey=True)
                report = json.loads((output / "result.json").read_bytes())
                self.assertEqual(actual, code)
                self.assertEqual(report["required_not_run"], 6 if fail else 5)
                self.assertEqual(next(row for row in report["cases"] if row["id"] == "AC21")["state"], "NOT_RUN")
                self.assertFalse(report["qualified"])
                self.assertEqual((output / "result.json").stat().st_mode & 0o777, 0o600)
                self.assertIn("archive_fault_journey", report)


if __name__ == "__main__":
    unittest.main()
