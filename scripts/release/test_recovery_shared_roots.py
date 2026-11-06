"""Portable shared-root recovery fixtures; no native macOS or database claim."""
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from common import ReleaseError
from recovery import backup, restore, selected_roots, verify
from recovery_files import file_digest, inventory, validate_index
import test_recovery


class SharedRootRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.case = test_recovery.RecoveryTests()
        self.case.setUp()
        self.addCleanup(self.case.doCleanups)
        c = self.case
        (c.data / "private-person").rename(c.private / "private-person")
        c.data.rmdir()
        c.data = c.private
        c.roots = selected_roots(c.config, c.data, c.state)
        c.path_metadata["data_dir"] = str(c.data)
        c.bundle = c.base / "shared-backup"
        self.output = c.base / "shared-restored"

    def backup(self):
        return self.case.backup()

    def restore(self):
        c = self.case
        return restore(c.bundle, c.prefix, c.database, self.output, exclusive=True)

    def original_files(self):
        c = self.case
        return {p.relative_to(c.data).as_posix(): p.read_bytes()
                for p in c.data.rglob("*") if p.is_file()
                and p.name not in {"start.lock", "daemon.lock", "operation.lock"}}

    def test_shared_backup_verify_restore_is_byte_exact_with_no_duplicates(self):
        c = self.case
        before = self.original_files()
        expected = inventory(c.roots)
        self.backup()
        manifest, index = verify(c.bundle)
        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(index["files"], expected)
        self.assertEqual(manifest["files"], len(before))
        payloads = validate_index(index["files"])
        self.assertEqual(len(payloads), len(before))
        self.assertFalse(any(i["scope"] == "data" and
                             (i["path"] == "config.toml" or i["path"].split("/")[0] == "state")
                             for i in index["files"]))
        result = self.restore()
        self.assertEqual(result["status"], "restored-offline-review-required")
        restored = {"config": self.output / "config.toml", "data": self.output / "data",
                    "state": self.output / "state"}
        self.assertEqual(inventory(restored), expected)
        self.assertEqual(before, self.original_files())
        self.assertFalse((self.output / "data/config.toml").exists())
        self.assertFalse((self.output / "data/state").exists())
        for path in [self.output, *self.output.rglob("*"), c.bundle, *c.bundle.rglob("*")]:
            self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_nested_state_exclusions_are_never_reached_through_data_scope(self):
        c = self.case
        os.mkfifo(c.state / "control.sock", 0o600)
        (c.state / "boot-fixture.toml").write_text("ephemeral secret")
        (c.state / "instance.json").write_text("ephemeral identity")
        self.backup()
        _, index = verify(c.bundle)
        self.assertFalse(any(i["path"].startswith(("state/", "boot-", "control.sock", "instance.json"))
                             for i in index["files"]))
        self.restore()
        for name in ("control.sock", "boot-fixture.toml", "instance.json", "daemon.lock",
                     "start.lock", "database/operation.lock"):
            self.assertFalse((self.output / "state" / name).exists())
        self.assertTrue((self.output / "state/database/password").is_file())

    def test_shared_layout_still_requires_runtime_attestation(self):
        c = self.case
        c.path_metadata["data_dir"] = str(c.data / "different")
        with self.assertRaisesRegex(ReleaseError, "authoritative paths"):
            self.backup()
        self.assertFalse(c.bundle.exists())

    def test_matching_names_outside_scope_boundaries_are_retained(self):
        c = self.case
        nested = c.data / "nested"
        nested.mkdir(mode=0o700)
        (nested / "state").mkdir(mode=0o700)
        (nested / "config.toml").write_bytes(b"ordinary captured config")
        (nested / "state/control.sock").write_bytes(b"ordinary captured filename")
        (c.state / "archive").mkdir(mode=0o700)
        (c.state / "archive/instance.json").write_bytes(b"retained historical state")
        before = inventory(c.roots)
        self.backup()
        self.restore()
        self.assertEqual((self.output / "data/nested/config.toml").read_bytes(), b"ordinary captured config")
        self.assertEqual((self.output / "data/nested/state/control.sock").read_bytes(), b"ordinary captured filename")
        self.assertEqual((self.output / "state/archive/instance.json").read_bytes(), b"retained historical state")
        self.assertEqual(before, inventory({"config": self.output / "config.toml",
                                            "data": self.output / "data", "state": self.output / "state"}))

    def test_unsupported_partial_overlaps_and_aliases_are_refused(self):
        c = self.case
        separate = c.base / "separate-state"
        separate.mkdir(mode=0o700)
        other_config = c.data / "other.toml"
        other_config.write_bytes(c.config.read_bytes())
        other_config.chmod(0o600)
        for config, data, state in ((other_config, c.data, c.state),
                                    (c.config, c.data, separate),
                                    (c.config, c.state, c.data),
                                    (c.config, c.data, c.data)):
            with self.subTest(config=config.name, state=state.name):
                with self.assertRaisesRegex(ReleaseError, "overlapping"):
                    selected_roots(config, data, state)
        alias = c.base / "alias"
        alias.symlink_to(c.data, target_is_directory=True)
        with self.assertRaises(ReleaseError):
            selected_roots(alias / "config.toml", alias, alias / "state")

    def test_symlink_hardlink_and_special_files_remain_refused(self):
        c = self.case
        for directory in (c.data, c.state):
            for kind in ("symlink", "hardlink", "fifo"):
                path = directory / "unsafe"
                with self.subTest(scope=directory.name, kind=kind):
                    if kind == "symlink": path.symlink_to(c.config)
                    elif kind == "hardlink": os.link(c.config, path)
                    else: os.mkfifo(path, 0o600)
                    try:
                        with self.assertRaises(ReleaseError): self.backup()
                        self.assertFalse(c.bundle.exists())
                    finally: path.unlink()

    def test_changed_nested_state_during_dump_discards_snapshot(self):
        c = self.case
        original = c.database.dump
        def mutation(path):
            original(path)
            (c.state / "new-retained-state").write_bytes(b"changed")
        c.database.dump = mutation
        with self.assertRaisesRegex(ReleaseError, "state changed"):
            self.backup()
        self.assertFalse(c.bundle.exists())
        self.assertFalse(list(c.base.glob(".mike-recovery-incomplete-*")))

    def test_replaced_scope_boundary_fails_before_payload_copy(self):
        c = self.case
        saved = c.base / "saved-state"
        original = file_digest
        replaced = False
        def replace_boundary(path, destination=None):
            nonlocal replaced
            result = original(path, destination)
            if destination is None and Path(path) == c.config and not replaced:
                c.state.rename(saved)
                c.state.symlink_to(saved, target_is_directory=True)
                replaced = True
            return result
        try:
            with patch("recovery_files.file_digest", side_effect=replace_boundary):
                with self.assertRaises(ReleaseError): self.backup()
            self.assertTrue(replaced)
            self.assertFalse(c.bundle.exists())
            self.assertFalse(list(c.base.glob(".mike-recovery-incomplete-*")))
        finally:
            if replaced:
                c.state.unlink()
                saved.rename(c.state)

    def test_corrupt_shared_bundle_traversal_is_rejected_before_restore(self):
        c = self.case
        self.backup()
        c.rewrite(lambda index: next(i for i in index["files"] if i["scope"] == "state").update(path="../../escape"))
        with self.assertRaisesRegex(ReleaseError, "unsafe recovery path"):
            self.restore()
        self.assertFalse(c.database.restored)
        self.assertFalse(self.output.exists())

    def test_failed_shared_backup_and_restore_never_publish_complete_destination(self):
        c = self.case
        c.database.fail = True
        with self.assertRaises(ReleaseError): self.backup()
        self.assertFalse(c.bundle.exists())
        c.database.fail = False
        self.backup()
        c.database.fail = True
        with self.assertRaises(ReleaseError): self.restore()
        self.assertFalse(self.output.exists())
        self.assertFalse(list(c.base.glob(".mike-recovery-incomplete-*")))
        verify(c.bundle)


if __name__ == "__main__":
    unittest.main()
