"""Exact-source preparation rejects Git index flags that hide changed inputs."""
import os
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

from common import ReleaseError, clean_source, source_git


class SourceIdentityTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-source-identity-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.environment = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
        self.environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                                GIT_TERMINAL_PROMPT="0", LC_ALL="C")
        self.git("init", "-q")
        self.git("config", "user.name", "Synthetic fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        (self.root / "runtime.png").write_bytes(b"synthetic-original")
        self.git("add", "runtime.png")
        self.git("commit", "-qm", "synthetic source")

    def git(self, *args):
        return subprocess.check_output(["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
                                        "-C", str(self.root), *args], env=self.environment,
                                       stderr=subprocess.DEVNULL).decode().strip()

    def test_complete_clean_source_retains_exact_commit(self):
        self.assertEqual(clean_source(self.root), self.git("rev-parse", "HEAD"))

    def test_skip_worktree_missing_runtime_input_is_refused(self):
        self.git("update-index", "--skip-worktree", "runtime.png")
        (self.root / "runtime.png").unlink()
        self.assertEqual(self.git("status", "--porcelain"), "")
        with self.assertRaisesRegex(ReleaseError, "hides tracked inputs"):
            clean_source(self.root)

    def test_assume_unchanged_mutation_is_refused(self):
        self.git("update-index", "--assume-unchanged", "runtime.png")
        (self.root / "runtime.png").write_bytes(b"different-runtime-input")
        self.assertEqual(self.git("status", "--porcelain"), "")
        with self.assertRaisesRegex(ReleaseError, "hides tracked inputs"):
            clean_source(self.root)

    def test_even_unchanged_hidden_entries_need_explicit_full_checkout(self):
        self.git("update-index", "--skip-worktree", "runtime.png")
        with self.assertRaisesRegex(ReleaseError, "complete checkout"):
            clean_source(self.root)

    def foreign_repository(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-foreign-source-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "foreign.txt").write_text("unrelated source")
        for arguments in (("init", "-q"), ("add", "foreign.txt"),
                          ("commit", "-qm", "foreign source")):
            subprocess.run(["git", "-c", "user.name=Synthetic fixture", "-c",
                            "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                            "-c", "core.hooksPath=/dev/null", *arguments], cwd=root,
                           env=self.environment, check=True, capture_output=True)
        return root

    def test_foreign_git_directory_and_worktree_cannot_supply_identity(self):
        foreign = self.foreign_repository()
        expected = self.git("rev-parse", "HEAD")
        with patch.dict(os.environ, {"GIT_DIR": str(foreign / ".git"),
                                     "GIT_WORK_TREE": str(foreign)}):
            self.assertEqual(clean_source(self.root), expected)
            (self.root / "runtime.png").write_bytes(b"hidden from foreign checkout")
            with self.assertRaisesRegex(ReleaseError, "source tree is not clean"):
                clean_source(self.root)

    def test_all_ambient_git_overrides_are_removed(self):
        overrides = {name: "untrusted" for name in (
            "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0",
            "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_SYSTEM", "GIT_REPLACE_REF_BASE")}
        with patch.dict(os.environ, overrides), patch("common.run", side_effect=[str(self.root), "result"]) as run:
            self.assertEqual(source_git(self.root, "rev-parse", "HEAD"), "result")
        for call in run.call_args_list:
            environment = call.kwargs["env"]
            self.assertFalse(set(overrides) & set(environment))
            self.assertEqual(environment["GIT_CONFIG_NOSYSTEM"], "1")
            self.assertEqual(environment["GIT_CONFIG_GLOBAL"], os.devnull)
            self.assertEqual(environment["GIT_NO_REPLACE_OBJECTS"], "1")
            self.assertEqual(environment["GIT_NO_LAZY_FETCH"], "1")
            self.assertIn("core.fsmonitor=false", call.args[0])
            self.assertIn("protocol.allow=never", call.args[0])

    def test_poisoned_global_and_system_configuration_is_not_read(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-git-config-")
        self.addCleanup(temporary.cleanup)
        config = Path(temporary.name) / "invalid-config"
        config.write_text("[invalid syntax\n")
        expected = self.git("rev-parse", "HEAD")
        with patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": str(config),
                                     "GIT_CONFIG_SYSTEM": str(config)}):
            self.assertEqual(clean_source(self.root), expected)

    def test_replacement_objects_do_not_change_source_metadata(self):
        original = self.git("rev-parse", "HEAD")
        tree = self.git("rev-parse", "HEAD^{tree}")
        replacement = self.git("commit-tree", tree, "-m", "replacement metadata")
        self.git("replace", original, replacement)
        self.assertEqual(self.git("show", "-s", "--format=%s", "HEAD"), "replacement metadata")
        self.assertEqual(source_git(self.root, "show", "-s", "--format=%s", "HEAD"), "synthetic source")
        self.assertEqual(clean_source(self.root), original)

    def test_selected_subdirectory_is_not_adopted_as_checkout_root(self):
        nested = self.root / "subdirectory"
        nested.mkdir()
        with self.assertRaisesRegex(ReleaseError, "requested checkout root"):
            clean_source(nested)

    def test_configured_fsmonitor_is_never_executed(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-fsmonitor-")
        self.addCleanup(temporary.cleanup)
        marker = Path(temporary.name) / "called"
        monitor = Path(temporary.name) / "monitor"
        monitor.write_text(f"#!/bin/sh\necho unexpected > '{marker}'\nexit 1\n")
        monitor.chmod(0o755)
        self.git("config", "core.fsmonitor", str(monitor))
        self.assertEqual(clean_source(self.root), source_git(self.root, "rev-parse", "HEAD"))
        self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
