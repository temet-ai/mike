"""Synthetic local Git/SQL fixtures only; no Cargo, SQL execution or database."""

import copy
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from common import ReleaseError
from migration_inventory import (MAX_VERSION, SOURCE_PATH, migration_checksum,
                                 source_migration_inventory, validate_migration_inventory)


class MigrationInventoryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-migration-inventory-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.directory = self.root / SOURCE_PATH
        self.directory.mkdir(parents=True)
        self.environment = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
        self.environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, LC_ALL="C")
        self.git("init", "-q")
        self.git("config", "user.name", "Synthetic migration fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write("10_synthetic_ten.sql", b"-- Synthetic ten\r\nSELECT 10;\r\n")
        self.write("2_synthetic_two.sql", "-- Synthetic Unicode café\nSELECT 2;\n".encode())
        self.commit = self.save()

    def git(self, *arguments):
        return subprocess.check_output(["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
                                        "-C", str(self.root), *arguments], env=self.environment,
                                       stderr=subprocess.DEVNULL).decode().strip()

    def write(self, name, content=b"-- Synthetic fixture only\nSELECT 1;\n"):
        path = self.directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        return path

    def save(self):
        self.git("add", ".")
        self.git("commit", "-qm", "synthetic migration source")
        return self.git("rev-parse", "HEAD")

    def inventory(self, commit=None):
        return source_migration_inventory(self.root, commit or self.commit)

    def test_exact_unmodified_utf8_bytes_use_sqlx_sha384_and_numeric_order(self):
        observed = self.inventory()
        self.assertEqual(observed["source_commit"], self.commit)
        self.assertEqual(observed["source_path"], SOURCE_PATH)
        self.assertEqual(observed["checksum_algorithm"], "sha384")
        self.assertEqual([item["version"] for item in observed["migrations"]], [2, 10])
        for item in observed["migrations"]:
            raw = (self.directory / item["filename"]).read_bytes()
            self.assertEqual(item["checksum"], hashlib.sha384(raw).hexdigest())
        self.assertEqual(observed, self.inventory())
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_integer_versions_do_not_lose_precision_above_json_safe_integer(self):
        self.write(f"{MAX_VERSION}_synthetic_max.sql")
        observed = self.inventory(self.save())
        self.assertEqual(observed["migrations"][-1]["version"], MAX_VERSION)
        self.assertIs(type(observed["migrations"][-1]["version"]), int)

    def test_duplicate_numeric_versions_with_distinct_filenames_are_refused(self):
        self.write("02_synthetic_duplicate.sql")
        with self.assertRaisesRegex(ReleaseError, "unique"):
            self.inventory(self.save())

    def test_malformed_and_unsupported_tracked_inputs_are_not_silently_ignored(self):
        for name in ("README.md", "1.sql", "one_bad.sql", "1_.sql", "0_zero.sql", "-1_negative.sql",
                     "+1_signed.sql", f"{MAX_VERSION + 1}_overflow.sql", "1_reversible.up.sql",
                     "1_reversible.down.sql", "1_dotted.name.sql", "nested/1_hidden.sql"):
            with self.subTest(name=name):
                path = self.write(name)
                commit = self.save()
                with self.assertRaises(ReleaseError):
                    self.inventory(commit)
                path.unlink()
                self.save()

    def test_ignored_untracked_file_or_directory_is_refused(self):
        (self.root / ".gitignore").write_text(SOURCE_PATH + "/ignored*\n")
        commit = self.save()
        extra = self.write("ignored_migration.sql")
        for kind in ("file", "directory"):
            with self.subTest(kind=kind), self.assertRaisesRegex(ReleaseError, "untracked inputs"):
                self.inventory(commit)
            extra.rmdir() if extra.is_dir() else extra.unlink()
            if kind == "file":
                extra.mkdir()

    def test_untracked_or_missing_source_and_changed_commit_are_refused(self):
        target = self.directory / "2_synthetic_two.sql"
        original = target.read_bytes()
        target.unlink()
        with self.assertRaisesRegex(ReleaseError, "not clean"):
            self.inventory()
        target.write_bytes(original + b"SELECT 3;\n")
        with self.assertRaisesRegex(ReleaseError, "not clean"):
            self.inventory()
        self.save()
        with self.assertRaisesRegex(ReleaseError, "commit changed"):
            self.inventory()

    def test_empty_tracked_inventory_fails(self):
        for path in self.directory.iterdir():
            path.unlink()
        (self.root / "keep.txt").write_text("Synthetic empty migration tree")
        with self.assertRaises(ReleaseError):
            self.inventory(self.save())

    def test_symlink_tracked_migration_fails(self):
        path = self.directory / "2_synthetic_two.sql"
        path.unlink()
        path.symlink_to("10_synthetic_ten.sql")
        with self.assertRaisesRegex(ReleaseError, "tracked regular file"):
            self.inventory(self.save())

    def test_symlinked_parent_directory_fails_even_if_cleanliness_is_mocked(self):
        moved = self.root / "elsewhere"
        self.directory.rename(moved)
        self.directory.symlink_to(moved, target_is_directory=True)
        with patch("migration_inventory.clean_source"), self.assertRaisesRegex(ReleaseError, "nonregular"):
            self.inventory()

    def test_missing_symlink_fifo_and_directory_replacements_fail_without_reading_them(self):
        path = self.directory / "2_synthetic_two.sql"
        original = path.read_bytes()
        for replacement in ("missing", "symlink", "fifo", "directory"):
            with self.subTest(replacement=replacement):
                path.unlink()
                if replacement == "symlink":
                    path.symlink_to("10_synthetic_ten.sql")
                elif replacement == "fifo":
                    os.mkfifo(path)
                elif replacement == "directory":
                    path.mkdir()
                with patch("migration_inventory.clean_source"), self.assertRaises(ReleaseError):
                    self.inventory()
                if replacement == "directory":
                    path.rmdir()
                elif replacement != "missing":
                    path.unlink()
                path.write_bytes(original)

    def test_raw_blob_comparison_detects_changed_bytes_despite_clean_status(self):
        path = self.directory / "10_synthetic_ten.sql"
        path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n"))
        with patch("migration_inventory.clean_source"), self.assertRaisesRegex(ReleaseError, "bytes differ"):
            self.inventory()

    def test_non_utf8_sql_fails_even_when_tracked_and_clean(self):
        self.write("3_invalid_utf8.sql", b"SELECT '\xff';\n")
        with self.assertRaisesRegex(ReleaseError, "UTF-8"):
            self.inventory(self.save())

    def test_git_sha256_blob_format_is_supported_without_changing_sqlx_checksum(self):
        path = self.directory / "2_synthetic_two.sql"
        data = path.read_bytes()
        blob = hashlib.sha256(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        descriptor = os.open(self.directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            self.assertEqual(migration_checksum(descriptor, path.name, blob), hashlib.sha384(data).hexdigest())
        finally:
            os.close(descriptor)

    def test_inventory_schema_is_exact_and_must_bind_same_source_commit(self):
        valid = self.inventory()
        changes = [lambda value: value.update(schema_version=True), lambda value: value.update(extra=True),
                   lambda value: value.update(source_commit="a" * 40),
                   lambda value: value.update(source_path="other"),
                   lambda value: value.update(checksum_algorithm="sha256"),
                   lambda value: value.update(migrations=[]),
                   lambda value: value["migrations"].reverse(),
                   lambda value: value["migrations"].append(value["migrations"][0]),
                   lambda value: value["migrations"][0].update(version=True),
                   lambda value: value["migrations"][0].update(version=3),
                   lambda value: value["migrations"][0].update(checksum="A" * 96),
                   lambda value: value["migrations"][0].update(filename="../2_traversal.sql"),
                   lambda value: value["migrations"][0].update(extra=True)]
        for change in changes:
            value = copy.deepcopy(valid)
            change(value)
            with self.subTest(value=value), self.assertRaises(ReleaseError):
                validate_migration_inventory(value, self.commit)
        for value in (None, [], {}, {"schema_version": 1}):
            with self.assertRaises(ReleaseError):
                validate_migration_inventory(value, self.commit)


if __name__ == "__main__":
    unittest.main()
