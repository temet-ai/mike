"""Real sb-test supervision with fake package tools, no Node or browser needed."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/release"))
from owned_process import OwnedProcess

FAKE_PNPM = r'''#!/usr/bin/env python3
import os
from pathlib import Path
import subprocess
import signal
import sys
import time
if "vitest" not in sys.argv:
    sys.exit(0)
marker = Path(os.environ["TEST_MARKER"])
if os.environ.get("TEST_IGNORE_TERM") == "1":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)",
                          "chrome-headless-shell", "vitest"])
marker.write_text(str(child.pid))
if os.environ.get("TEST_WAIT") == "1":
    while not marker.with_suffix(".release").exists():
        time.sleep(0.02)
print(" Test Files  1 passed (1)")
print("      Tests  1 passed (1)")
sys.exit(int(os.environ.get("TEST_EXIT", "0")))
'''


def alive(pid):
    try:
        if sys.platform == "linux":
            status = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1]
            if status.strip().startswith("Z"):
                return False
        os.kill(pid, 0)
        return True
    except (ProcessLookupError, FileNotFoundError):
        return False


class OwnedTestRunnerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        scripts = self.root / "scripts"
        (scripts / "lib").mkdir(parents=True)
        (scripts / "release").mkdir()
        shutil.copy2(ROOT / "scripts/release/owned_process.py", scripts / "release")
        (self.root / "frontend" / "src").mkdir(parents=True)
        (self.root / "frontend" / "src" / "app.css").write_text("")
        shutil.copy2(ROOT / "scripts/sb-test", scripts / "sb-test")
        shutil.copy2(ROOT / "scripts/lib/owned-test-runner.py", scripts / "lib")
        (self.root / "bin").mkdir()
        pnpm = self.root / "bin/pnpm"
        pnpm.write_text(FAKE_PNPM)
        pnpm.chmod(0o700)
        self.children = []

    def tearDown(self):
        # Keep test-controller ownership too; marker PIDs are observations,
        # never authority to signal a possibly recycled process number.
        for child in self.children:
            if isinstance(child, OwnedProcess):
                child.close()
            else:
                if child.poll() is None:
                    child.terminate()
                child.communicate(timeout=6)
        self.tmp.cleanup()

    def start(self, name, wait=False, exit_code=0, options=(), ignore_term=False):
        marker = self.root / name
        child = OwnedProcess(
            ["bash", str(self.root / "scripts/sb-test"), "--unit",
             "--skip-typecheck", *options],
            env={"PATH": f"{self.root / 'bin'}:{os.defpath}",
                 "HOME": str(self.root), "TMPDIR": str(self.root),
                 "TEST_MARKER": str(marker), "TEST_WAIT": str(int(wait)),
                 "TEST_EXIT": str(exit_code), "TEST_IGNORE_TERM": str(int(ignore_term))},
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        )
        self.children.append(child)
        return child, marker

    def wait_started(self, marker):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if marker.exists() and marker.read_text():
                pid = int(marker.read_text())
                return pid
            time.sleep(0.02)
        self.fail(f"owned fake vitest did not start: {marker.name}")

    def foreign_process(self):
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)",
                                  "chrome-headless-shell", "vitest"])
        self.children.append(child)
        return child

    def assert_gone(self, pid):
        deadline = time.monotonic() + 3
        while alive(pid) and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertFalse(alive(pid), f"owned child {pid} was left running")

    def test_success_cleans_owned_descendants_and_preserves_unrelated_process(self):
        foreign = self.foreign_process()
        child, marker = self.start("normal")
        owned_pid = self.wait_started(marker)
        output, _ = child.communicate(timeout=8)
        self.assertEqual(child.returncode, 0, output)
        self.assertIn("sb-test: PASS", output)
        self.assert_gone(owned_pid)
        self.assertIsNone(foreign.poll(), "unrelated named browser/Vitest was killed")

    def test_failure_propagates_and_still_cleans_owned_descendants(self):
        child, marker = self.start("failure", exit_code=17)
        owned_pid = self.wait_started(marker)
        output, _ = child.communicate(timeout=8)
        self.assertEqual(child.returncode, 17, output)
        self.assertIn("sb-test: FAIL", output)
        self.assertNotIn("sb-test: PASS", output)
        self.assert_gone(owned_pid)

    def test_interruption_fails_and_only_terminates_owned_process_group(self):
        foreign = self.foreign_process()
        child, marker = self.start("interrupted", wait=True)
        owned_pid = self.wait_started(marker)
        child.signal_group(signal.SIGTERM)
        output, _ = child.communicate(timeout=8)
        self.assertEqual(child.returncode, 143, output)
        self.assertTrue(output.rstrip().endswith(
            "sb-test: FAIL (aborted by SIGTERM before it finished)"), output)
        self.assertNotIn("sb-test: PASS", output)
        self.assert_gone(owned_pid)
        self.assertIsNone(foreign.poll(), "unrelated named browser/Vitest was killed")

    def test_interruption_escalates_for_stubborn_owned_descendants(self):
        foreign = self.foreign_process()
        child, marker = self.start("stubborn", wait=True, ignore_term=True)
        owned_pid = self.wait_started(marker)
        child.signal_group(signal.SIGTERM)
        output, _ = child.communicate(timeout=8)
        self.assertEqual(child.returncode, 143, output)
        self.assertNotIn("sb-test: PASS", output)
        self.assert_gone(owned_pid)
        self.assertIsNone(foreign.poll())

    def test_concurrent_run_waits_for_atomic_lock_and_runs_after_release(self):
        first, first_marker = self.start("first", wait=True)
        first_pid = self.wait_started(first_marker)
        second, second_marker = self.start("second", wait=True)
        time.sleep(0.3)
        self.assertIsNone(second.poll())
        self.assertFalse(second_marker.exists(), "second test run bypassed the lock")
        first_marker.with_suffix(".release").touch()
        first_output, _ = first.communicate(timeout=8)
        self.assertEqual(first.returncode, 0, first_output)
        self.assert_gone(first_pid)
        second_pid = self.wait_started(second_marker)
        second_marker.with_suffix(".release").touch()
        second_output, _ = second.communicate(timeout=8)
        self.assertEqual(second.returncode, 0, second_output)
        self.assertIn("waiting for the active owned test run", second_output)
        self.assert_gone(second_pid)

    def test_waiting_run_can_abort_without_touching_lock_owner(self):
        first, first_marker = self.start("owner", wait=True)
        first_pid = self.wait_started(first_marker)
        second, second_marker = self.start("waiter", wait=True)
        time.sleep(0.3)
        second.signal_group(signal.SIGTERM)
        output, _ = second.communicate(timeout=8)
        self.assertEqual(second.returncode, 143, output)
        self.assertFalse(second_marker.exists())
        self.assertTrue(alive(first_pid))
        self.assertIsNone(first.poll())
        first_marker.with_suffix(".release").touch()
        output, _ = first.communicate(timeout=8)
        self.assertEqual(first.returncode, 0, output)
        self.assert_gone(first_pid)


if __name__ == "__main__":
    unittest.main()
