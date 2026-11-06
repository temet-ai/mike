"""Bounded source-tree fixtures: local Git and tool stubs, never real builds."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from common import ReleaseError, clean_source
from source_inputs import SOURCE_ROOTS, verify_file, verify_source_inputs
import test_prepare


class SourceInputTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-source-inputs-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / "source"
        self.root.mkdir()
        self.environment = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
        self.environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                                GIT_TERMINAL_PROMPT="0", LC_ALL="C")
        self.git("init", "-q")
        self.git("config", "user.name", "Synthetic source fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write(".gitignore", "/target/\n/dist-out/\n/frontend/build/\n/frontend/node_modules/\n"
                                "/frontend/.svelte-kit/\nignored*\n")
        for scope in SOURCE_ROOTS:
            self.write(scope + "/fixture.txt", "unchanged source bytes\n")
        self.write("crates/mike/build.rs", "fn main() {}\n")
        self.write("crates/mike/Cargo.toml", "[package]\n")
        self.write("crates/mike/tests/example.rs", "#[test] fn example() {}\n")
        self.write("crates/mike/resources/fixture.bin", "synthetic resource\n")
        self.write("crates/mike/src/store/persistence/migrations/1_fixture.sql", "SELECT 1;\n")
        self.write("crates/mike/scripts/build.sh", "#!/bin/sh\nexit 0\n").chmod(0o755)
        self.commit = self.save()

    def git(self, *arguments):
        return subprocess.check_output(["git", "-c", "commit.gpgsign=false", "-c",
                                        "core.hooksPath=/dev/null", *arguments], cwd=self.root,
                                       env=self.environment, stderr=subprocess.DEVNULL).decode().strip()

    def write(self, path, data):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(data)
        return destination

    def save(self):
        self.git("add", ".")
        self.git("commit", "-qm", "synthetic source inputs")
        return self.git("rev-parse", "HEAD")

    def test_clean_source_accepts_regular_and_executable_tracked_modes(self):
        entries = self.git("ls-files", "--stage")
        self.assertIn("100644 ", entries)
        self.assertIn("100755 ", entries)
        self.assertEqual(clean_source(self.root), self.commit)
        self.assertEqual(clean_source(self.root, self.commit), self.commit)

    def test_ignored_inputs_in_every_source_root_fail_despite_clean_git_status(self):
        paths = [scope + "/ignored-input" for scope in SOURCE_ROOTS]
        paths += ["frontend/static/ignored-payload.js", "crates/mike/src/ignored-source.rs",
                  "crates/mike/src/store/persistence/migrations/ignored-migration.sql"]
        for name in paths:
            with self.subTest(path=name):
                extra = self.write(name, "unreviewed production input\n")
                self.assertEqual(self.git("status", "--porcelain"), "")
                for expected_sha in (None, self.commit):
                    with self.assertRaisesRegex(ReleaseError, "untracked inputs"):
                        clean_source(self.root, expected_sha)
                extra.unlink()

    def test_untracked_empty_and_generated_named_directories_are_not_exempted(self):
        for name in ("crates/ignored-empty", "frontend/src/node_modules", ".cargo/target"):
            with self.subTest(path=name):
                extra = self.root / name
                extra.mkdir()
                self.assertEqual(self.git("status", "--porcelain"), "")
                with self.assertRaisesRegex(ReleaseError, "untracked inputs"):
                    clean_source(self.root)
                extra.rmdir()

    def test_untracked_nonignored_input_is_refused(self):
        self.write("frontend/static/payload.js", "unreviewed\n")
        with self.assertRaisesRegex(ReleaseError, "not clean"):
            clean_source(self.root)
        with self.assertRaisesRegex(ReleaseError, "untracked inputs"):
            verify_source_inputs(self.root)

    def test_output_and_dependency_directories_outside_source_roots_remain_allowed(self):
        for path in ("frontend/build/asset.js", "frontend/node_modules/pkg/index.js",
                     "frontend/.svelte-kit/generated.js", "target/dist/mike", "dist-out/fixture"):
            self.write(path, "local output or dependency\n")
        self.assertEqual(clean_source(self.root), self.commit)

    def test_raw_blob_hash_detects_same_size_mtime_bytes_hidden_by_git_stat_cache(self):
        path = self.root / "frontend/static/fixture.txt"
        os.utime(path, (946684800, 946684800))
        self.git("update-index", "--refresh")
        self.git("config", "core.trustctime", "false")
        self.git("config", "core.checkStat", "minimal")
        before = path.stat()
        path.write_bytes(path.read_bytes().replace(b"unchanged", b"different"))
        os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
        self.assertEqual(self.git("status", "--porcelain"), "")
        with self.assertRaisesRegex(ReleaseError, "bytes differ"):
            clean_source(self.root)

    def test_missing_file_and_missing_root_fail_without_trusting_status(self):
        path = self.root / "frontend/static/fixture.txt"
        path.unlink()
        with self.assertRaisesRegex(ReleaseError, "missing tracked inputs"):
            verify_source_inputs(self.root)
        path.parent.rmdir()
        with self.assertRaisesRegex(ReleaseError, "missing tracked inputs"):
            verify_source_inputs(self.root)

    def test_permission_changes_are_detected_even_when_git_ignores_file_mode(self):
        self.git("config", "core.filemode", "false")
        for name, mode in (("frontend/src/fixture.txt", 0o755), ("crates/mike/scripts/build.sh", 0o644)):
            with self.subTest(path=name):
                path = self.root / name
                previous = path.stat().st_mode
                path.chmod(mode)
                self.assertEqual(self.git("status", "--porcelain"), "")
                with self.assertRaisesRegex(ReleaseError, "executable mode"):
                    clean_source(self.root)
                path.chmod(previous)

    def test_symlink_directory_escape_is_never_followed_including_scope_parents(self):
        for name in ("crates/mike/resources", "frontend/static", "frontend"):
            with self.subTest(path=name):
                directory = self.root / name
                outside = self.base / "outside"
                directory.rename(outside)
                directory.symlink_to(outside, target_is_directory=True)
                with self.assertRaisesRegex(ReleaseError, "symlink"):
                    verify_source_inputs(self.root)
                directory.unlink()
                outside.rename(directory)

    def test_symlink_file_fifo_and_directory_replacements_fail_without_reading(self):
        path = self.root / "frontend/static/fixture.txt"
        original = path.read_bytes()
        for kind in ("symlink", "fifo", "directory"):
            with self.subTest(kind=kind):
                path.unlink()
                if kind == "symlink":
                    path.symlink_to(self.root / "frontend/src/fixture.txt")
                elif kind == "fifo":
                    os.mkfifo(path)
                else:
                    path.mkdir()
                with self.assertRaises(ReleaseError):
                    verify_source_inputs(self.root)
                path.rmdir() if kind == "directory" else path.unlink()
                path.write_bytes(original)

    def test_committed_symlink_and_gitlink_modes_are_not_accepted_as_regular_inputs(self):
        path = self.root / "frontend/static/fixture.txt"
        path.unlink()
        path.symlink_to("../src/fixture.txt")
        self.save()
        with self.assertRaisesRegex(ReleaseError, "tracked regular file"):
            clean_source(self.root)
        self.git("update-index", "--cacheinfo", f"160000,{self.commit},frontend/static/fixture.txt")
        with self.assertRaisesRegex(ReleaseError, "tracked regular file"):
            verify_source_inputs(self.root)

    def test_git_sha256_blob_hash_and_filename_whitespace_are_supported(self):
        path = self.write("frontend/static/ whitespace\tand\nlines ", "synthetic bytes\n")
        self.save()
        self.assertEqual(clean_source(self.root), self.git("rev-parse", "HEAD"))
        data = path.read_bytes()
        digest = hashlib.sha256(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            verify_file(descriptor, path.name, path.as_posix(), ("100644", digest))
        finally:
            os.close(descriptor)

    def test_absent_untracked_roots_are_allowed_but_existing_untracked_roots_are_not(self):
        shutil.rmtree(self.root / ".sqlx")
        self.save()
        self.assertEqual(clean_source(self.root), self.git("rev-parse", "HEAD"))
        (self.root / ".sqlx").mkdir()
        with self.assertRaisesRegex(ReleaseError, "untracked inputs"):
            clean_source(self.root)


class SourceInputPreparationTests(unittest.TestCase):
    def setUp(self):
        self.fixture = test_prepare.PreparationTests("test_prepare_twice_preserves_source_and_exact_candidate")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.fixture.write("frontend/static/fixture.js", "synthetic static asset\n")
        self.fixture.write("frontend/src/fixture.ts", "synthetic source\n")
        with (self.fixture.repo / ".gitignore").open("a") as ignore:
            ignore.write("ignored*\n")
        self.fixture.git("add", ".")
        self.fixture.git("-c", "user.name=Synthetic fixture", "-c", "user.email=fixture@example.invalid",
                         "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
                         "commit", "-qm", "source guard fixture")

    def test_both_native_targets_reject_ignored_production_inputs_before_tools(self):
        for target in ("x86_64-unknown-linux-gnu", "aarch64-apple-darwin"):
            for name in ("frontend/static/ignored-payload.js", "frontend/src/ignored-source.ts",
                         "crates/mike/src/store/persistence/migrations/ignored-migration.sql"):
                with self.subTest(target=target, path=name):
                    self.fixture.configure(target=target)
                    self.fixture.write(name, "unreviewed input\n")
                    result = self.fixture.invoke("--target", target, success=False)
                    self.assertIn("untracked inputs", result.stderr)
                    self.assertEqual(self.fixture.calls(), [])
                    self.assertFalse((self.fixture.repo / "dist-out").exists())
                    (self.fixture.repo / name).unlink()

    def test_post_build_recheck_rejects_ignored_injection_without_finalizing_candidate(self):
        import prepare

        repo = self.fixture.repo
        commit = self.fixture.git("rev-parse", "HEAD")
        def inject(*_arguments):
            self.fixture.write("frontend/static/ignored-injected.js", "unreviewed post-build asset\n")
            return []
        with patch("prepare.preflight", return_value=("0.1.0", commit, "x86_64-unknown-linux-gnu",
                                                       {}, {}, 0)), patch("prepare.build", side_effect=inject), \
                patch("prepare.prepare_archives", return_value=[]), \
                self.assertRaisesRegex(ReleaseError, "untracked inputs"):
            prepare.prepare(repo, None, None)
        self.assertFalse((repo / "dist-out/.prepare.lock").exists())
        self.assertFalse(list((repo / "dist-out").rglob("manifest.json")))


if __name__ == "__main__":
    unittest.main(verbosity=2)
