"""Synthetic POSIX group ownership contracts; no Mike services or credentials."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from owned_process import OwnedProcess


def alive(pid):
    try:
        if sys.platform == "linux":
            state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1]
            if state.strip().startswith("Z"):
                return False
        os.kill(pid, 0)
        return True
    except (ProcessLookupError, FileNotFoundError):
        return False


class OwnedProcessTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="mike-owned-helper-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def process(self, code, **options):
        child = OwnedProcess([sys.executable, "-c", code], **options)
        self.addCleanup(child.close)
        return child

    def assert_gone(self, pid):
        deadline = time.monotonic() + 3
        while alive(pid) and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertFalse(alive(pid), "owned synthetic descendant remained running")

    def descendant_code(self, suffix=""):
        marker = str(self.root / "descendant")
        return ("import subprocess,sys,pathlib,time\n"
                "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],"
                "stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)\n"
                f"pathlib.Path({marker!r}).write_text(str(child.pid))\n" + suffix)

    def test_target_reap_keeps_anchor_reserved_until_final_signal_then_one_reap(self):
        child = self.process("print('synthetic')", stdout=subprocess.PIPE)
        self.assertEqual(child.communicate(timeout=3)[0], b"synthetic\n")
        self.assertEqual(child.wait(timeout=1), 0)
        self.assertNotEqual(child.pid, child._anchor.pid)
        self.assertIsNone(child._anchor.returncode)
        self.assertEqual(os.getpgid(child._anchor.pid), child._anchor.pid)
        real_signal = os.killpg
        events = []

        def owned_signal(group, signum):
            self.assertIsNone(child._anchor.returncode, "group signalled after anchor reap")
            self.assertEqual(group, child._anchor.pid)
            events.append("signal")
            return real_signal(group, signum)

        real_wait = child._anchor.wait

        def reap(*args, **kwargs):
            self.assertFalse(child._can_signal)
            events.append("reap")
            child.signal_group(signal.SIGTERM)
            return real_wait(*args, **kwargs)

        with patch("owned_process.os.killpg", side_effect=owned_signal), patch.object(
                child._anchor, "wait", side_effect=reap):
            child.close()
            child.signal_group(signal.SIGTERM)
            child.close()
        self.assertEqual(events, ["signal", "reap"])

    def test_success_and_failure_still_kill_descendants_with_closed_stdio(self):
        for exit_code in (0, 17):
            with self.subTest(exit_code=exit_code):
                child = self.process(self.descendant_code(f"sys.exit({exit_code})\n"),
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                self.assertEqual(child.communicate(timeout=3), (b"", b""))
                self.assertEqual(child.returncode, exit_code)
                descendant = int((self.root / "descendant").read_text())
                self.assertTrue(alive(descendant))
                child.close()
                self.assert_gone(descendant)

    def test_timeout_kills_owned_tree_and_preserves_foreign_group(self):
        foreign = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(60)"],
                                   start_new_session=True)
        try:
            child = self.process(self.descendant_code("time.sleep(60)\n"),
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            with self.assertRaises(subprocess.TimeoutExpired):
                child.communicate(timeout=0.2)
            descendant = int((self.root / "descendant").read_text())
            child.close()
            self.assert_gone(descendant)
            self.assertIsNone(foreign.poll())
        finally:
            foreign.terminate()
            foreign.wait(timeout=3)

    def test_graceful_signal_reaches_target_but_anchor_remains_owned(self):
        marker = str(self.root / "ready")
        child = self.process("import signal,time,pathlib,sys\n"
                             "signal.signal(signal.SIGTERM,lambda *_: sys.exit(23))\n"
                             f"pathlib.Path({marker!r}).touch()\ntime.sleep(60)")
        deadline = time.monotonic() + 3
        while not Path(marker).exists() and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(Path(marker).exists())
        child.signal_group(signal.SIGTERM)
        self.assertEqual(child.wait(timeout=3), 23)
        self.assertIsNone(child._anchor.returncode)
        child.close()

    def test_unexpected_anchor_exit_is_not_reaped_before_cleanup_signal(self):
        child = self.process("import time;time.sleep(60)")
        os.kill(child._anchor.pid, signal.SIGKILL)
        with self.assertRaises(OSError):
            child.wait(timeout=3)
        self.assertIsNone(child._anchor.returncode)
        child.close()
        self.assertEqual(child._anchor.returncode, -signal.SIGKILL)

    def test_controller_death_closes_liveness_pipe_and_kills_owned_tree(self):
        helper_dir = str(Path(__file__).resolve().parent)
        marker = str(self.root / "controlled")
        code = (f"import sys,os,pathlib;sys.path.insert(0,{helper_dir!r})\n"
                "from owned_process import OwnedProcess\n"
                "child=OwnedProcess([sys.executable,'-c','import time;time.sleep(60)'])\n"
                f"pathlib.Path({marker!r}).write_text(str(child.pid))\nos._exit(0)\n")
        subprocess.run([sys.executable, "-c", code], check=True, timeout=5)
        self.assert_gone(int(Path(marker).read_text()))

    def test_stdio_protocol_and_file_redirection_remain_usable(self):
        child = self.process("import sys;print(sys.stdin.readline().strip())",
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        child.stdin.write(b"synthetic protocol\n")
        self.assertEqual(child.communicate(timeout=3)[0], b"synthetic protocol\n")
        input_path, output_path = self.root / "in", self.root / "out"
        input_path.write_bytes(b"private fixture")
        with input_path.open("rb") as source, output_path.open("wb") as destination:
            copied = self.process("import sys;sys.stdout.buffer.write(sys.stdin.buffer.read())",
                                  stdin=source, stdout=destination)
            self.assertEqual(copied.communicate(timeout=3), (None, None))
        self.assertEqual(output_path.read_bytes(), b"private fixture")

    def test_automatic_or_external_child_reaping_is_refused_before_spawn(self):
        for handler in (signal.SIG_IGN, lambda *_: None):
            with patch("owned_process.signal.getsignal", return_value=handler), patch(
                    "owned_process.subprocess.Popen") as spawn:
                with self.assertRaisesRegex(OSError, "default child reaping"):
                    OwnedProcess([sys.executable, "-c", "pass"])
                spawn.assert_not_called()

    def test_output_limit_and_missing_executable_fail_closed(self):
        child = self.process("import sys;sys.stdout.write('x'*10000)", stdout=subprocess.PIPE)
        with patch("owned_process.MAX_OUTPUT_BYTES", 1024), self.assertRaisesRegex(OSError, "limit"):
            child.communicate(timeout=3)
        child.close()
        with self.assertRaisesRegex(OSError, "could not start"):
            OwnedProcess([str(self.root / "missing")])


if __name__ == "__main__":
    unittest.main(verbosity=2)
