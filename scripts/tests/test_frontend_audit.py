"""Offline audit evidence and wrapper regressions; never contact a registry."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("frontend_audit", ROOT / "scripts/lib/frontend_audit.py")
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def report(**counts):
    return {"advisories": {}, "metadata": {"vulnerabilities": {
        level: counts.get(level, 0) for level in AUDIT.SEVERITIES}}}


class FrontendAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "frontend").mkdir()
        (self.root / "frontend/pnpm-lock.yaml").write_text("lockfileVersion: '9.0'\n")

    def run_audit(self, value=None, exit_code=0, side_effect=None):
        result = subprocess.CompletedProcess([], exit_code, json.dumps(value), "")
        output = io.StringIO()
        with patch.object(AUDIT.shutil, "which", return_value="/offline/pnpm"), \
                patch.object(AUDIT.subprocess, "run", return_value=result, side_effect=side_effect) as run, \
                contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            code = AUDIT.run(self.root)
        return code, output.getvalue(), run

    def test_complete_clean_response_is_required_for_pass(self):
        code, output, run = self.run_audit(report())
        self.assertEqual(code, 0, output)
        self.assertIn("FRONTEND-AUDIT: PASS", output)
        self.assertIn("Frontend lockfile sha256:", output)
        command = run.call_args.args[0]
        self.assertEqual(command, ["/offline/pnpm", "audit", "--json", "--audit-level=high",
                                   "--dev", "--prod", "--optional",
                                   "--ignore-registry-errors=false", "--pm-on-fail=error"])
        self.assertEqual(run.call_args.kwargs["cwd"], self.root / "frontend")
        self.assertGreater(run.call_args.kwargs["timeout"], 0)

    def test_high_and_critical_fail_even_when_pnpm_exits_zero(self):
        for severity in AUDIT.BLOCKING:
            for exit_code in (0, 1):
                with self.subTest(severity=severity, exit_code=exit_code):
                    code, output, _ = self.run_audit(report(**{severity: 1}), exit_code)
                    self.assertEqual(code, 1, output)
                    self.assertIn("FRONTEND-AUDIT: FAIL", output)
                    self.assertNotIn("FRONTEND-AUDIT: PASS", output)

    def test_lower_severities_pass_and_are_still_reported(self):
        for severity in ("info", "low", "moderate"):
            with self.subTest(severity=severity):
                code, output, _ = self.run_audit(report(**{severity: 1}))
                self.assertEqual(code, 0, output)
                self.assertIn("FRONTEND-AUDIT: PASS", output)
                self.assertIn(f"{severity}=1", output)

    def test_inventory_advisory_cannot_be_hidden_by_zero_metadata(self):
        value = report()
        value["advisories"]["1"] = {"severity": "high", "patched_versions": None}
        code, output, _ = self.run_audit(value)
        self.assertEqual(code, 1, output)
        self.assertIn("high=1", output)

    def test_unavailable_or_invalid_report_is_not_run_even_on_zero_exit(self):
        values = [None, {}, [], {"error": "private registry token"}, report(low=-1),
                  report(high=True), report(critical="0"), {"metadata": report()["metadata"]}]
        value = report()
        del value["metadata"]["vulnerabilities"]["moderate"]
        values.append(value)
        value = report()
        value["metadata"]["vulnerabilities"]["unknown"] = 0
        values.append(value)
        value = report()
        value["advisories"]["1"] = {"severity": "unknown"}
        values.append(value)
        for value in values:
            with self.subTest(value=value):
                code, output, _ = self.run_audit(value)
                self.assertEqual(code, 2, output)
                self.assertIn("FRONTEND-AUDIT: NOT RUN", output)
                self.assertNotIn("private registry token", output)

    def test_nonzero_process_cannot_pass_with_zero_counts(self):
        code, output, _ = self.run_audit(report(), 17)
        self.assertEqual(code, 2, output)
        self.assertIn("pnpm exit 17", output)

    def test_timeout_and_execution_failure_are_not_run(self):
        for error in (OSError("not executable"), subprocess.TimeoutExpired("pnpm", 120)):
            code, output, _ = self.run_audit(side_effect=error)
            self.assertEqual(code, 2, output)
            self.assertIn("FRONTEND-AUDIT: NOT RUN", output)

    def test_lockfile_change_cannot_pass(self):
        def change_lock(*args, **kwargs):
            (self.root / "frontend/pnpm-lock.yaml").write_text("different lockfile")
            return subprocess.CompletedProcess([], 0, json.dumps(report()), "")
        code, output, _ = self.run_audit(side_effect=change_lock)
        self.assertEqual(code, 2, output)
        self.assertIn("lockfile changed", output)

    def test_missing_lockfile_or_tool_is_not_run(self):
        with patch.object(AUDIT.shutil, "which", return_value=None), \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(AUDIT.run(self.root), 2)
        (self.root / "frontend/pnpm-lock.yaml").unlink()
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(AUDIT.run(self.root), 2)

    def test_test_suite_propagates_audit_verdict_and_all_reaches_it(self):
        scripts = self.root / "scripts"
        (scripts / "lib").mkdir(parents=True)
        for name in ("test-suite", "frontend-audit", "lib/frontend_audit.py"):
            shutil.copy2(ROOT / "scripts" / name, scripts / name)
        for name in ("gate", "sb-test"):
            path = scripts / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o700)
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        os.symlink(sys.executable, bin_dir / "python3")
        pnpm = bin_dir / "pnpm"
        pnpm.write_text("#!/usr/bin/env python3\nimport os\nprint(os.environ['AUDIT_REPORT'])\n")
        pnpm.chmod(0o700)
        env = {"PATH": f"{bin_dir}:{os.defpath}", "HOME": str(self.root)}
        for mode, value, expected in (("frontend-audit", report(), 0),
                                      ("frontend-audit", report(high=1), 1),
                                      ("frontend-audit", {}, 2),
                                      ("all", report(high=1), 1), ("all", {}, 2)):
            with self.subTest(mode=mode, value=value):
                result = subprocess.run(["bash", str(scripts / "test-suite"), mode],
                                        env={**env, "AUDIT_REPORT": json.dumps(value)},
                                        capture_output=True, text=True, timeout=10)
                output = result.stdout + result.stderr
                self.assertEqual(result.returncode, expected, output)
                self.assertIn("Frontend dependency advisories", output)
                self.assertIn(f"TEST-SUITE: {'PASS' if expected == 0 else 'FAIL'}", output)

    def test_arguments_cannot_weaken_gate_policy(self):
        result = subprocess.run([sys.executable, str(ROOT / "scripts/lib/frontend_audit.py"),
                                 "--audit-level=critical"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("policy cannot be weakened", result.stderr)


if __name__ == "__main__":
    unittest.main()
