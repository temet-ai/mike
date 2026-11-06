"""Standalone build-script regression fixtures; never compile the Mike workspace."""

from datetime import datetime, timezone
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class BuildIdentityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.compilation = tempfile.TemporaryDirectory(prefix="mike-build-script-")
        cls.addClassCleanup(cls.compilation.cleanup)
        cls.scripts = {}
        rustc = os.environ.get("RUSTC") or shutil.which("rustc")
        if rustc is None:
            raise RuntimeError("preinstalled rustc required for standalone build identity regression")
        for name in ("mike", "mike-mcp"):
            output = Path(cls.compilation.name) / name
            subprocess.run([rustc, "--edition=2024", str(ROOT / "crates" / name / "build.rs"),
                            "-o", str(output)], env=dict(os.environ, RUSTUP_AUTO_INSTALL="0"),
                           check=True, capture_output=True, text=True, timeout=60)
            cls.scripts[name] = output

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-build-identity-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / "source"
        self.root.mkdir()
        self.environment = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
        self.environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                                GIT_TERMINAL_PROMPT="0", LC_ALL="C")
        self.git("init", "-q")
        self.git("config", "user.name", "Synthetic build fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        (self.root / "crates/mike").mkdir(parents=True)
        (self.root / "crates/mike-mcp").mkdir()
        (self.root / ".gitignore").write_text("/.claude/\n")
        (self.root / "prepare.py").write_text("first Python-only revision\n")
        self.commit()

    def git(self, *args, root=None):
        return subprocess.check_output(["git", "-c", "commit.gpgsign=false", "-c",
                                        "core.hooksPath=/dev/null", *args], cwd=root or self.root,
                                       env=self.environment, stderr=subprocess.DEVNULL).decode().strip()

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "synthetic build fixture")
        return self.git("rev-parse", "HEAD")

    def emit(self, *, root=None, name="mike", epoch="0", environment=None, success=True):
        root = root or self.root
        manifest = root / "crates" / name
        manifest.mkdir(parents=True, exist_ok=True)
        env = dict(self.environment, CARGO_MANIFEST_DIR=str(manifest), SOURCE_DATE_EPOCH=epoch)
        env.update(environment or {})
        result = subprocess.run([self.scripts[name]], cwd=self.base, env=env,
                                capture_output=True, text=True, timeout=15)
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        values = dict(line.removeprefix("cargo::rustc-env=").split("=", 1)
                      for line in result.stdout.splitlines() if line.startswith("cargo::rustc-env="))
        paths = {Path(line.removeprefix("cargo:rerun-if-changed=")).resolve()
                 for line in result.stdout.splitlines() if line.startswith("cargo:rerun-if-changed=")}
        return values, paths, result

    def test_both_build_scripts_stamp_the_same_exact_clean_source_and_epoch(self):
        expected = self.git("rev-parse", "HEAD")
        for name in self.scripts:
            with self.subTest(binary=name):
                values, _, result = self.emit(name=name, epoch="1704067200")
                self.assertEqual(values["MIKE_BUILD_SHA"], expected)
                self.assertEqual(values["MIKE_BUILD_DIRTY"], "false")
                self.assertEqual(values["MIKE_BUILD_TIME"], "2024-01-01T00:00:00Z")
                self.assertIn("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH\n", result.stdout)

    def test_detached_worktree_python_only_checkout_changes_watched_head(self):
        first = self.git("rev-parse", "HEAD")
        (self.root / "prepare.py").write_text("second Python-only revision\n")
        second = self.commit()
        worktree = self.root / ".claude/worktrees/identity"
        self.git("worktree", "add", "--quiet", "--detach", str(worktree), first)
        first_stamp, watched, _ = self.emit(root=worktree)
        head = Path(self.git("rev-parse", "--git-path", "HEAD", root=worktree)).resolve()
        index = Path(self.git("rev-parse", "--git-path", "index", root=worktree)).resolve()
        self.assertIn(head, watched)
        self.assertIn(index, watched)
        self.assertNotIn(self.root / ".git/HEAD", watched)
        before = head.read_bytes()
        self.git("checkout", "--quiet", "--detach", second, root=worktree)
        self.assertNotEqual(head.read_bytes(), before)
        second_stamp, _, _ = self.emit(root=worktree)
        self.assertEqual(first_stamp["MIKE_BUILD_SHA"], first)
        self.assertEqual(second_stamp["MIKE_BUILD_SHA"], second)

    def test_symbolic_and_packed_refs_remain_watched(self):
        reference = self.git("symbolic-ref", "HEAD")
        _, loose_paths, _ = self.emit()
        self.assertIn(self.root / ".git" / reference, loose_paths)
        self.git("pack-refs", "--all", "--prune")
        _, paths, _ = self.emit()
        self.assertIn(self.root / ".git/HEAD", paths)
        self.assertNotIn(self.root / ".git" / reference, paths)
        self.assertIn(self.root / ".git/packed-refs", paths)

    def test_missing_repository_cannot_claim_a_clean_build(self):
        empty = self.base / "no-repository"
        empty.mkdir()
        values, _, _ = self.emit(root=empty)
        self.assertEqual(values["MIKE_BUILD_SHA"], "unknown")
        self.assertEqual(values["MIKE_BUILD_DIRTY"], "true")

    def test_failed_status_cannot_claim_a_clean_build_with_known_sha(self):
        tools = self.base / "tools"
        tools.mkdir()
        real_git = shutil.which("git")
        fake = tools / "git"
        fake.write_text('#!/bin/sh\nfor arg do\n  [ "$arg" != status ] || exit 71\ndone\n'
                        f'exec "{real_git}" "$@"\n')
        fake.chmod(0o755)
        values, _, _ = self.emit(environment={"PATH": str(tools) + os.pathsep + self.environment["PATH"]})
        self.assertEqual(values["MIKE_BUILD_SHA"], self.git("rev-parse", "HEAD"))
        self.assertEqual(values["MIKE_BUILD_DIRTY"], "true")

    def test_ambient_git_overrides_cannot_change_the_embedded_source(self):
        values, _, _ = self.emit(environment={"GIT_DIR": str(self.base / "missing"),
                                              "GIT_WORK_TREE": str(self.base),
                                              "GIT_CONFIG_COUNT": "nonsense"})
        self.assertEqual(values["MIKE_BUILD_SHA"], self.git("rev-parse", "HEAD"))
        self.assertEqual(values["MIKE_BUILD_DIRTY"], "false")

    def test_source_epoch_utc_is_deterministic_across_timezone_and_calendar_edges(self):
        for epoch in (0, 951782400, 2147483647, 4294967295, 253402300799):
            with self.subTest(epoch=epoch):
                expected = datetime.fromtimestamp(epoch, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
                values, _, _ = self.emit(epoch=str(epoch), environment={"TZ": "Pacific/Honolulu"})
                self.assertEqual(values["MIKE_BUILD_TIME"], expected)

    def test_invalid_source_epoch_fails_instead_of_using_wall_clock(self):
        for epoch in ("", "-1", "+1", " 1", "1 ", "1.0", "abc", "253402300800", "9" * 60):
            with self.subTest(epoch=epoch):
                _, _, result = self.emit(epoch=epoch, success=False)
                self.assertIn("SOURCE_DATE_EPOCH", result.stderr)

    def test_tracked_dirty_source_is_reported(self):
        (self.root / "prepare.py").write_text("unstaged edit")
        values, _, _ = self.emit()
        self.assertEqual(values["MIKE_BUILD_DIRTY"], "true")

    def test_mbx_cache_identity_remains_explicitly_non_release(self):
        values, _, _ = self.emit(environment={"MIKE_BUILD_CACHE": "mbx"})
        self.assertEqual(values["MIKE_BUILD_SHA"], "mbx-build")
        self.assertEqual(values["MIKE_BUILD_TIME"], "mbx-build")


if __name__ == "__main__":
    unittest.main(verbosity=2)
