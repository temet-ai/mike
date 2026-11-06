#!/usr/bin/env python3
"""Evidence and resource ownership contracts for the archive acceptance foundation."""

import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

from common import ReleaseError
from inputs import clean_environment, owned_run
import test_install

ROOT = test_install.ROOT


class AcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.fixture = test_install.InstallTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.base = self.fixture.base
        self.candidate = self.fixture.candidate

    def run_acceptance(self, output, env=None):
        return subprocess.run([ROOT / "scripts/release-acceptance", "--candidate", self.candidate,
                               "--output", output], text=True, capture_output=True, env=env, timeout=30)

    def test_helpers_do_not_turn_full_cases_into_false_passes(self):
        output = self.base / "evidence"
        result = self.run_acceptance(output)
        self.assertEqual(result.returncode, 2, result.stderr)
        report = json.loads((output / "result.json").read_text())
        self.assertEqual(report["state"], "INCOMPLETE")
        self.assertEqual(report["qualified"], False)
        self.assertEqual(report["verdict"], "NOT RUN")
        self.assertEqual(report["required_not_run"], 5)  # AC08 passes from the install checks
        self.assertEqual([c["id"] for c in report["cases"]], [f"AC{i:02}" for i in range(1, 28)] +
                         ["SG01", "SG02", "SG03", "PUBLIC01", "PUBLIC02", "FEEDBACK01"])
        self.assertTrue(all(c["state"] == "NOT_RUN" for c in report["cases"] if c["id"] != "AC08"))
        self.assertEqual(sum(c["required"] for c in report["cases"]), 6)
        self.assertEqual(len(report["checks"]), 4)
        self.assertTrue(all(c["state"] == "PASSED" for c in report["checks"]))
        self.assertEqual(report["candidate"]["source_commit"], "a" * 40)
        self.assertEqual(len(report["candidate"]["archives"]), 2)
        self.assertIn("A gating NOT RUN case prevents the alpha gate", (output / "summary.txt").read_text())
        self.assertEqual(output.stat().st_mode & 0o777, 0o700)
        self.assertEqual((output / "result.json").stat().st_mode & 0o777, 0o600)

    def test_corrupt_archive_records_failure_and_not_run_journeys(self):
        archive = next(self.candidate.glob("*.tar.gz")); archive.write_bytes(b"bad archive")
        output = self.base / "evidence"
        result = self.run_acceptance(output)
        self.assertEqual(result.returncode, 1)
        report = json.loads((output / "result.json").read_text())
        self.assertEqual(report["state"], "FAILED")
        self.assertEqual(report["checks"][0]["state"], "FAILED")
        self.assertEqual(report["required_not_run"], 6)
        self.assertFalse(report["qualified"])

    def test_existing_evidence_never_overwritten(self):
        output = self.base / "evidence"; output.mkdir()
        (output / "keep").write_text("original")
        self.assertEqual(self.run_acceptance(output).returncode, 1)
        self.assertEqual(list(output.iterdir()), [output / "keep"])
        self.assertEqual((output / "keep").read_text(), "original")

    def test_evidence_inside_candidate_refused(self):
        output = self.candidate / "evidence"
        self.assertEqual(self.run_acceptance(output).returncode, 1)
        self.assertFalse(output.exists())

    def test_evidence_inside_source_refused(self):
        output = ROOT / "must-not-write-acceptance-test"
        self.assertEqual(self.run_acceptance(output).returncode, 1)
        self.assertFalse(output.exists())

    def test_sanitized_evidence_and_fresh_home_preserve_operator_files(self):
        output = self.base / "evidence"
        home = self.base / "operator"; home.mkdir()
        sentinel = "FAKE_PRIVATE_MARKER_MUST_NOT_APPEAR"
        for filename in (".env", ".profile", ".bashrc"):
            (home / filename).write_text(sentinel)
        env = dict(os.environ, HOME=str(home), MIKE_SMOKE_RUN="true", MIKE_SECRET=sentinel,
                   DATABASE_URL="postgres://" + sentinel, PYTHONPATH=sentinel, VITE_SECRET=sentinel)
        self.assertEqual(self.run_acceptance(output, env).returncode, 2)
        for path in output.iterdir():
            self.assertNotIn(sentinel, path.read_text())
            self.assertNotIn("postgres://", path.read_text())
        for path in home.iterdir():
            self.assertEqual(path.read_text(), sentinel)
        self.assertEqual(len(list(home.iterdir())), 3)

    def test_child_timeout_kills_only_owned_process_group(self):
        marker = self.base / "child-pid"
        # A child holding stdout open must not extend the deadline or survive it.
        code = "import os,time,pathlib; pid=os.fork(); pathlib.Path(%r).write_text(str(pid)) if pid else None; time.sleep(60)" % str(marker)
        foreign = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"],
                                   start_new_session=True)
        try:
            with self.assertRaisesRegex(ReleaseError, "timed out"):
                owned_run([sys.executable, "-c", code], self.base, clean_environment(self.base), timeout=0.2)
            self.assertIsNone(foreign.poll())
            pid = int(marker.read_text())
            proc = Path(f"/proc/{pid}/stat")
            self.assertIn(test_install.await_stopped_child(proc), (None, "Z"),
                          "owned descendant is still running after bounded cleanup wait")
            self.assertIsNone(foreign.poll())
        finally:
            foreign.terminate(); foreign.wait(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
