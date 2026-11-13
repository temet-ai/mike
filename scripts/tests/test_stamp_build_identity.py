"""The deploy's identity stamper, against fake binaries and a scratch git repository."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "stamp_build_identity", REPO / "scripts/lib/stamp_build_identity.py")
stamp = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stamp)

SHA = "0123456789abcdef0123456789abcdef01234567"


def fake_binary(slots: int = 1) -> bytes:
    slot = stamp.MARKER + b"\0" * (stamp.SLOT_LEN - len(stamp.MARKER))
    return b"\x7fELF-head" + slot * slots + b"tail-bytes"


class StampBuildIdentityTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.dir = Path(self.scratch.name)

    def write_binary(self, data: bytes) -> Path:
        path = self.dir / "mike"
        path.write_bytes(data)
        path.chmod(0o755)
        return path

    def test_stamp_writes_payload_after_marker_and_keeps_length(self):
        data = fake_binary()
        payload = stamp.payload_for(SHA, "0123456", False, "2026-11-13T15:00:00Z")
        stamped = stamp.stamp_bytes(data, payload)
        self.assertEqual(len(stamped), len(data))
        start = stamped.index(stamp.MARKER) + len(stamp.MARKER)
        self.assertTrue(stamped[start:].startswith(
            f"{SHA}\t0123456\tfalse\t2026-11-13T15:00:00Z\0".encode()))
        self.assertTrue(stamped.endswith(b"tail-bytes"))

    def test_restamp_replaces_previous_identity_whole(self):
        long = stamp.payload_for(SHA, "0123456", True, "2026-11-13T15:00:00Z")
        short = stamp.payload_for("ab", "a", False, "t")
        twice = stamp.stamp_bytes(stamp.stamp_bytes(fake_binary(), long), short)
        self.assertEqual(twice, stamp.stamp_bytes(fake_binary(), short))

    def test_binary_without_or_with_two_slots_is_refused(self):
        payload = stamp.payload_for(SHA, "0123456", False, "t")
        for slots in (0, 2):
            with self.assertRaises(stamp.StampError):
                stamp.stamp_bytes(fake_binary(slots), payload)

    def test_identity_that_does_not_fit_or_holds_a_tab_is_refused(self):
        with self.assertRaises(stamp.StampError):
            stamp.payload_for("a" * 300, "a", False, "t")
        with self.assertRaises(stamp.StampError):
            stamp.payload_for("a\tb", "a", False, "t")

    def test_stamp_file_breaks_hard_link_and_keeps_mode(self):
        path = self.write_binary(fake_binary())
        store = self.dir / "store-object"
        os.link(path, store)
        stamp.stamp_file(str(path), stamp.payload_for(SHA, "0123456", False, "t"))
        self.assertEqual(store.read_bytes(), fake_binary())
        self.assertNotEqual(path.read_bytes(), fake_binary())
        self.assertTrue(os.access(path, os.X_OK))

    def test_repo_identity_is_head_and_clean_tree(self):
        repo = self.dir / "repo"
        repo.mkdir()
        env = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.invalid",
               "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.invalid"}
        for args in (["init", "-q"], ["commit", "-q", "--allow-empty", "-m", "one"]):
            subprocess.run(["git", "-C", str(repo), *args], check=True, env=env)
        head = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], check=True,
                              capture_output=True, text=True).stdout.strip()
        path = self.write_binary(fake_binary())
        commit_time = subprocess.run(["git", "-C", str(repo), "log", "-1", "--format=%ct"],
                                     check=True, capture_output=True, text=True).stdout.strip()
        os.environ.pop("SOURCE_DATE_EPOCH", None)
        self.assertEqual(stamp.main([str(path), "--repo", str(repo)]), 0)
        first = path.read_bytes()
        self.assertIn(head.encode() + b"\t", first)
        self.assertIn(b"\tfalse\t" + stamp.utc_text(int(commit_time)).encode() + b"\0", first)
        self.assertEqual(stamp.main([str(path), "--repo", str(repo)]), 0)
        self.assertEqual(path.read_bytes(), first, "a clean tree stamps the same bytes twice")

    def test_main_reports_failure_for_a_binary_without_slot(self):
        path = self.write_binary(b"no slot here")
        self.assertEqual(stamp.main([str(path), "--sha", SHA, "--short", "0123456",
                                     "--dirty", "false", "--built-at", "t"]), 1)
        self.assertEqual(path.read_bytes(), b"no slot here")


if __name__ == "__main__":
    unittest.main()
