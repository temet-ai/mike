"""Synthetic recovery fixtures; no server, real capture or source build."""
import ctypes
import errno
import fcntl
import json
import os
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch
from common import ReleaseError
from install import install
from recovery import backup, restore, runtime_paths as real_runtime_paths, runtime_command as real_runtime_command, selected_roots, uninstall, upgrade_check, verify
from recovery import publish_directory_noreplace, staging
from recovery_files import file_digest, inventory, validate_index
import test_install

LEDGER = [{"version": 20260604000001, "checksum": "a" * 96, "success": True}]

class FakeDatabase:
    kind = "postgres-service"
    def __init__(self):
        self.restored = False
        self.fresh = True
        self.fail = False
    def assert_source(self, config):
        pass
    def facts(self):
        return {"postgres_major": 19, "migration_ledger": LEDGER, "database_change_token": "d" * 64}
    def dump(self, path):
        path.write_bytes(b"PGDMPfixture-private-conversation-content")
        path.chmod(0o600)
        if self.fail:
            raise ReleaseError("synthetic dump failure")
    def restore(self, path):
        if not self.fresh:
            raise ReleaseError("database is not empty")
        if self.fail:
            raise ReleaseError("synthetic transactional restore failure")
        assert path.read_bytes().startswith(b"PGDMP")
        self.restored = True

class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.fixture = test_install.InstallTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.base, self.prefix = self.fixture.base, self.fixture.prefix
        install(self.fixture.candidate, self.prefix)
        self.private = self.base / "private"
        self.private.mkdir(mode=0o700)
        self.config = self.private / "config.toml"
        self.config.write_text('persist_enabled=true\ndatabase_url="postgres://user:FAKE_SECRET@localhost/db"\n')
        self.config.chmod(0o600)
        self.data, self.state = self.private / "data", self.private / "state"
        self.data.mkdir(mode=0o700); self.state.mkdir(mode=0o700)
        (self.data / "private-person").mkdir(mode=0o700)
        (self.data / "private-person" / "body").write_bytes(b"fake raw body\x00")
        (self.state / "database").mkdir(mode=0o700)
        (self.state / "database" / "password").write_text("c" * 64)
        (self.state / "database" / "password").chmod(0o600)
        self.roots = selected_roots(self.config, self.data, self.state)
        self.database = FakeDatabase()
        self.bundle = self.private / "backup"
        from recovery import installation_id
        self.path_metadata = {"config_file": str(self.config), "data_dir": str(self.data),
                              "state_dir": str(self.state), "log_dir": str(self.state / "logs"),
                              "installation_id": installation_id(self.config)}
        mocked_paths = patch("recovery.runtime_paths", return_value=self.path_metadata)
        mocked_paths.start(); self.addCleanup(mocked_paths.stop)
        commands = patch("recovery.runtime_command", return_value=b"prerequisites passed")
        commands.start(); self.addCleanup(commands.stop)
    def backup(self):
        return backup(self.prefix, self.roots, self.database, self.bundle, exclusive=True)
    def test_ambiguous_overlapping_roots_refuse_without_mutation(self):
        before = {path.relative_to(self.private): path.read_bytes()
                  for path in self.private.rglob("*") if path.is_file()}
        for data, state in ((self.data, self.data), (self.private, self.data),
                            (self.state, self.private)):
            with self.subTest(overlap_kind="legacy-or-nested"):
                with self.assertRaisesRegex(ReleaseError, "separate non-overlapping roots") as failure:
                    selected_roots(self.config, data, state)
                message = str(failure.exception)
                self.assertIn("exact config.toml and state/", message)
                self.assertIn("print-config --paths", message)
                self.assertIn("changing --config or data_dir alone is not migration", message)
                self.assertNotIn(str(self.private), message)
                self.assertNotIn("FAKE_SECRET", message)
        self.assertEqual(before, {path.relative_to(self.private): path.read_bytes()
                                 for path in self.private.rglob("*") if path.is_file()})
        self.assertFalse(self.database.restored)
        self.assertFalse(self.bundle.exists())
    def rewrite(self, change):
        path = self.bundle / "restore-index.json"
        index = json.loads(path.read_text()); change(index)
        path.write_text(json.dumps(index))
        manifest_path = self.bundle / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["private_index"] = file_digest(path)
        manifest_path.write_text(json.dumps(manifest))
    def test_backup_restores_byte_exact_private_data_without_starting(self):
        result = self.backup()
        self.assertFalse(result["qualified"])
        restored = self.private / "restored"
        output = restore(self.bundle, self.prefix, self.database, restored, exclusive=True)
        self.assertTrue(self.database.restored)
        self.assertEqual(output["status"], "restored-offline-review-required")
        self.assertEqual((restored / "config.toml").read_bytes(), self.config.read_bytes())
        self.assertEqual((restored / "data/private-person/body").read_bytes(), b"fake raw body\x00")
        self.assertEqual((restored / "state/database/password").read_text(), "c" * 64)
        self.assertFalse((restored / "state/daemon.lock").exists())
        self.assertFalse((restored / "state/database/operation.lock").exists())
        self.assertFalse((restored / "database.dump").exists())
        for root in (self.bundle, restored):
            for path in [root, *root.rglob("*")]:
                self.assertEqual(path.stat().st_mode & 0o077, 0, path)
    def test_manifest_and_receipt_are_allowlisted_not_secret_bearing(self):
        result = self.backup()
        public = (self.bundle / "manifest.json").read_text() + json.dumps(result)
        for private in ("FAKE_SECRET", "private-person", "fake raw body", str(self.private), "postgres://"):
            self.assertNotIn(private, public)
        self.assertIn("FAKE_SECRET", (self.bundle / "files/00000000").read_text())
    def test_failed_backup_never_publishes_complete_destination(self):
        self.database.fail = True
        with self.assertRaises(ReleaseError): self.backup()
        self.assertFalse(self.bundle.exists())
        self.assertEqual(list(self.private.glob(".mike-recovery-incomplete-*")), [])
    def test_running_daemon_or_database_operation_prevents_snapshot(self):
        for name in ("daemon.lock", "start.lock", "database/operation.lock"):
            with self.subTest(name=name):
                descriptor = os.open(self.state / name, os.O_CREAT | os.O_RDWR, 0o600)
                try:
                    fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    with self.assertRaisesRegex(ReleaseError, "running or starting"): self.backup()
                    self.assertFalse(self.bundle.exists())
                finally: os.close(descriptor)
    def test_lifecycle_socket_and_boot_snapshot_are_excluded(self):
        # Socket creation is denied in this executor. A FIFO exercises the same
        # named special-file exclusion without opening a network/socket resource.
        os.mkfifo(self.state / "control.sock", 0o600)
        (self.state / "boot-fixture.toml").write_text("private ephemeral override")
        (self.state / "instance.json").write_text('{"pid":1}')
        self.backup(); _, index = verify(self.bundle)
        self.assertFalse(any(i["path"].startswith(("boot-", "control.sock", "instance.json")) for i in index["files"]))
    def test_symlink_hardlink_and_special_input_are_refused(self):
        for kind in ("symlink", "hardlink", "fifo"):
            path = self.data / "unsafe"
            with self.subTest(kind=kind):
                if kind == "symlink": path.symlink_to(self.config)
                elif kind == "hardlink": os.link(self.config, path)
                else: os.mkfifo(path, 0o600)
                try:
                    with self.assertRaises(ReleaseError): self.backup()
                    self.assertFalse(self.bundle.exists())
                finally: path.unlink()
    def test_changed_state_during_dump_discards_snapshot(self):
        original = self.database.dump
        def mutation(path):
            original(path); (self.data / "changed").write_text("new capture")
        self.database.dump = mutation
        with self.assertRaisesRegex(ReleaseError, "changed"): self.backup()
        self.assertFalse(self.bundle.exists())
    def test_corruption_and_unexpected_bundle_entries_refused(self):
        self.backup(); payload = self.bundle / "files/00000000"
        original = payload.read_bytes(); payload.write_bytes(original + b"corruption")
        with self.assertRaisesRegex(ReleaseError, "checksum"): verify(self.bundle)
        payload.write_bytes(original); (self.bundle / "unexpected").write_text("not part of backup")
        with self.assertRaisesRegex(ReleaseError, "unexpected"): verify(self.bundle)
    def test_parent_traversal_refused_before_database(self):
        self.backup(); self.rewrite(lambda index: index["files"][1].update(path="../../outside"))
        with self.assertRaisesRegex(ReleaseError, "unsafe recovery path"):
            restore(self.bundle, self.prefix, self.database, self.private / "restore", exclusive=True)
        self.assertFalse(self.database.restored)
        for value in ("/outside", "a/../b", "a//b", "a\\b", ".", "a\x00b"):
            with self.subTest(value=value), self.assertRaises(ReleaseError):
                validate_index([{"scope":"config", "path":value, "kind":"file"}])
    def test_nonempty_restore_database_or_existing_destination_untouched(self):
        self.backup(); self.database.fresh = False; output = self.private / "restore"
        with self.assertRaisesRegex(ReleaseError, "not empty"):
            restore(self.bundle, self.prefix, self.database, output, exclusive=True)
        self.assertFalse(output.exists()); output.mkdir()
        with self.assertRaisesRegex(ReleaseError, "already exists"):
            restore(self.bundle, self.prefix, self.database, output, exclusive=True)
        self.assertFalse(self.database.restored)
    def test_restore_requires_exact_backed_up_binary_generation(self):
        self.backup(); install(self.fixture.make_candidate("0.2.0"), self.prefix)
        with self.assertRaisesRegex(ReleaseError, "exact backed-up binary"):
            restore(self.bundle, self.prefix, self.database, self.private / "restore", exclusive=True)
        self.assertFalse(self.database.restored)
    def test_existing_destination_or_nested_backup_refused(self):
        self.bundle.mkdir(mode=0o700)
        with self.assertRaisesRegex(ReleaseError, "already exists"): self.backup()
        self.bundle.rmdir()
        with self.assertRaisesRegex(ReleaseError, "outside every"):
            backup(self.prefix, self.roots, self.database, self.data / "backup", exclusive=True)
    def test_upgrade_checks_current_snapshot_and_refuses_downgrade(self):
        self.backup(); candidate = self.fixture.make_candidate("0.2.0")
        result = upgrade_check(self.bundle, self.prefix, self.roots, self.database, candidate, exclusive=True)
        self.assertEqual(result["schema_compatibility"], "NOT_RUN")
        self.assertEqual(result["candidate_version"], "0.2.0")
        self.assertIn("0.1.0", subprocess.check_output([self.prefix / "bin/mike", "--version"], text=True))
        with self.assertRaisesRegex(ReleaseError, "downgrade"):
            upgrade_check(self.bundle, self.prefix, self.roots, self.database, self.fixture.candidate, exclusive=True)
        (self.data / "new").write_text("new body")
        with self.assertRaisesRegex(ReleaseError, "no longer matches"):
            upgrade_check(self.bundle, self.prefix, self.roots, self.database, candidate, exclusive=True)
    def test_uninstall_only_unlinks_owned_commands_and_reinstall_reconnects(self):
        self.backup(); before = inventory(self.roots)
        unrelated = self.prefix / "bin/unrelated"; unrelated.write_text("unrelated")
        result = uninstall(self.prefix, self.state, self.config, exclusive=True)
        self.assertTrue(result["data_retained"])
        self.assertFalse((self.prefix / "bin/mike").exists())
        self.assertFalse((self.prefix / "bin/mike-mcp").exists())
        self.assertEqual(unrelated.read_text(), "unrelated")
        self.assertEqual(before, inventory(self.roots)); verify(self.bundle)
        uninstall(self.prefix, self.state, self.config, exclusive=True)
        install(self.fixture.candidate, self.prefix)
        self.assertTrue((self.prefix / "bin/mike").is_symlink())
        self.assertEqual(before, inventory(self.roots))
    def test_changed_command_is_never_removed(self):
        path = self.prefix / "bin/mike-mcp"; path.unlink(); path.write_text("user replacement")
        with self.assertRaisesRegex(ReleaseError, "unrelated"):
            uninstall(self.prefix, self.state, self.config, exclusive=True)
        self.assertTrue((self.prefix / "bin/mike").is_symlink())
        self.assertEqual(path.read_text(), "user replacement")
    def test_no_mutation_without_writer_confirmation(self):
        with self.assertRaisesRegex(ReleaseError, "confirm"):
            backup(self.prefix, self.roots, self.database, self.bundle)
        with self.assertRaisesRegex(ReleaseError, "confirm"): uninstall(self.prefix, self.state, self.config)
        self.assertTrue((self.prefix / "bin/mike").exists()); self.assertFalse(self.bundle.exists())
    def test_cli_never_echoes_bad_config_or_connection_values(self):
        self.config.write_text('database_url="postgres://FAKE_SECRET\n')
        result = subprocess.run([test_install.ROOT / "scripts/release-maintenance", "backup",
            "--prefix", self.prefix, "--config", self.config, "--data-root", self.data,
            "--state-root", self.state, "--destination", self.bundle,
            "--confirm-stopped-writers", "--pg-service", "recovery"], text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("FAKE_SECRET", result.stdout + result.stderr)
        self.assertNotIn(str(self.private), result.stdout + result.stderr)

    def test_runtime_path_probe_uses_verified_binary_and_clean_environment(self):
        with patch.dict(os.environ, {"MIKE_DATA_DIR":"foreign", "DATABASE_URL":"FAKE_SECRET"}), \
                patch("recovery.execute", return_value=json.dumps(self.path_metadata).encode()) as run:
            self.assertEqual(real_runtime_command(self.prefix, self.config, ["print-config", "--paths"]), json.dumps(self.path_metadata).encode())
        command, environment = run.call_args.args
        self.assertEqual(command[1:], ["--config", str(self.config), "print-config", "--paths"])
        self.assertEqual(command[0], str(self.prefix / "lib/mike-archive/current/mike"))
        self.assertNotIn("MIKE_DATA_DIR", environment)
        self.assertNotIn("DATABASE_URL", environment)

    def outside_snapshot(self, directory):
        result = {}
        for path in [directory, *sorted(directory.rglob("*"))]:
            relative = path.relative_to(directory).as_posix()
            if path.is_symlink():
                result[relative] = ("symlink", os.readlink(path))
            elif path.is_file():
                result[relative] = ("file", path.read_bytes(), path.stat().st_mode)
            else:
                result[relative] = ("directory", path.stat().st_mode)
        return result

    def assert_intermediate_symlink_refused(self, relative):
        self.backup()
        candidate = self.fixture.make_candidate("0.2.0")
        original = self.prefix / relative
        outside = self.base / "outside-installation"
        original.rename(outside)
        original.symlink_to(outside, target_is_directory=True)
        marker = outside / "unrelated-sentinel"
        marker.write_bytes(b"unrelated outside data")
        before = self.outside_snapshot(outside)
        untouched = inventory(self.roots)
        destination = self.private / "must-not-be-created"
        operations = {
            "uninstall": lambda: uninstall(self.prefix, self.state, self.config, exclusive=True),
            "backup": lambda: backup(self.prefix, self.roots, self.database, destination, exclusive=True),
            "restore": lambda: restore(self.bundle, self.prefix, self.database, destination, exclusive=True),
            "upgrade-check": lambda: upgrade_check(self.bundle, self.prefix, self.roots, self.database, candidate, exclusive=True),
        }
        with patch("recovery.runtime_paths", side_effect=AssertionError("runtime must not run")), \
                patch("recovery.runtime_command", side_effect=AssertionError("runtime must not run")):
            for name, operation in operations.items():
                with self.subTest(operation=name), self.assertRaisesRegex(ReleaseError, "not a plain directory"):
                    operation()
                self.assertEqual(self.outside_snapshot(outside), before)
                self.assertEqual(inventory(self.roots), untouched)
                self.assertFalse(destination.exists())
                self.assertFalse(self.database.restored)
                self.assertTrue(original.is_symlink())
        self.assertEqual(marker.read_bytes(), b"unrelated outside data")

    def test_symlinked_bin_never_unlinks_outside_matching_commands(self):
        self.assert_intermediate_symlink_refused("bin")

    def test_symlinked_lib_never_follows_outside_installation(self):
        self.assert_intermediate_symlink_refused("lib")

    def test_symlinked_generations_never_executes_outside_binaries(self):
        self.assert_intermediate_symlink_refused("lib/mike-archive/generations")

    def test_shared_writable_intermediate_directories_refuse_maintenance(self):
        for relative in (".", "bin", "lib", "lib/mike-archive/generations"):
            directory = self.prefix / relative
            mode = directory.stat().st_mode
            try:
                directory.chmod(mode | 0o020)
                with self.subTest(directory=relative), self.assertRaisesRegex(ReleaseError, "not shared-writable"):
                    uninstall(self.prefix, self.state, self.config, exclusive=True)
                self.assertTrue((self.prefix / "bin/mike").is_symlink())
                self.assertTrue((self.prefix / "bin/mike-mcp").is_symlink())
            finally:
                directory.chmod(mode)

    def test_wrong_authoritative_state_root_is_refused_before_locking(self):
        self.path_metadata["state_dir"] = str(self.private / "different")
        with self.assertRaisesRegex(ReleaseError, "authoritative paths"): self.backup()
        with self.assertRaisesRegex(ReleaseError, "authoritative paths"):
            uninstall(self.prefix, self.state, self.config, exclusive=True)
        self.assertFalse(self.bundle.exists())
        self.assertFalse((self.state / "daemon.lock").exists())
        self.assertTrue((self.prefix / "bin/mike").is_symlink())
    def test_incompatible_installed_migrations_block_backup_before_dump(self):
        with patch("recovery.runtime_command", side_effect=ReleaseError("incompatible migrations")):
            with self.assertRaisesRegex(ReleaseError, "incompatible migrations"): self.backup()
        self.assertFalse(self.bundle.exists())

    def test_wrong_database_source_aborts_before_snapshot(self):
        with patch.object(self.database, "assert_source", side_effect=ReleaseError("source mismatch")):
            with self.assertRaisesRegex(ReleaseError, "source mismatch"): self.backup()
        self.assertFalse(self.bundle.exists())
    def test_racing_empty_destination_is_never_replaced(self):
        original_inode = []
        def race(source, destination):
            destination.mkdir(mode=0o700)
            original_inode.append(destination.stat().st_ino)
            return publish_directory_noreplace(source, destination)
        with patch("recovery.publish_directory_noreplace", side_effect=race):
            with self.assertRaisesRegex(ReleaseError, "existing destination was preserved"):
                self.backup()
        self.assertEqual(self.bundle.stat().st_ino, original_inode[0])
        self.assertEqual(list(self.bundle.iterdir()), [])
        self.assertEqual(list(self.private.glob(".mike-recovery-incomplete-*")), [])

    def test_racing_restore_destination_keeps_foreign_inode_and_contents(self):
        self.backup()
        destination = self.private / "racing-restore"
        original_inode = []
        def race(source, target):
            target.mkdir(mode=0o700)
            (target / "operator-file").write_bytes(b"keep me")
            original_inode.append(target.stat().st_ino)
            return publish_directory_noreplace(source, target)
        with patch("recovery.publish_directory_noreplace", side_effect=race):
            with self.assertRaisesRegex(ReleaseError, "existing destination was preserved"):
                restore(self.bundle, self.prefix, self.database, destination, exclusive=True)
        self.assertEqual(destination.stat().st_ino, original_inode[0])
        self.assertEqual((destination / "operator-file").read_bytes(), b"keep me")
        self.assertEqual(list(destination.iterdir()), [destination / "operator-file"])
        self.assertEqual(list(self.private.glob(".mike-recovery-incomplete-*")), [])

    def test_atomic_publication_unsupported_platform_never_falls_back(self):
        with patch("recovery.sys.platform", "unsupported"), patch.object(Path, "rename", side_effect=AssertionError("unsafe fallback")):
            with self.assertRaisesRegex(ReleaseError, "unsupported on this platform"):
                self.backup()
        self.assertFalse(self.bundle.exists())

    def test_atomic_publication_unsupported_filesystem_never_falls_back(self):
        from unittest.mock import Mock
        library = Mock()
        library.renameat2.return_value = -1
        with patch("recovery.sys.platform", "linux"), patch("recovery.ctypes.CDLL", return_value=library), \
                patch("recovery.ctypes.get_errno", return_value=errno.ENOTSUP), \
                patch.object(Path, "rename", side_effect=AssertionError("unsafe fallback")):
            with self.assertRaisesRegex(ReleaseError, "filesystem lacks atomic"):
                self.backup()
        self.assertFalse(self.bundle.exists())
        self.assertEqual(library.renameat2.call_args.args[-1], 1)

    def test_atomic_publication_missing_symbol_never_falls_back(self):
        with patch("recovery.sys.platform", "linux"), patch("recovery.ctypes.CDLL", return_value=object()):
            with self.assertRaisesRegex(ReleaseError, "publication is unavailable"):
                self.backup()
        self.assertFalse(self.bundle.exists())

    def test_darwin_atomic_publication_uses_exclusive_flag(self):
        from unittest.mock import Mock
        library = Mock()
        library.renamex_np.return_value = 0
        source, destination = self.private / "source", self.private / "destination"
        with patch("recovery.sys.platform", "darwin"), patch("recovery.ctypes.CDLL", return_value=library):
            publish_directory_noreplace(source, destination)
        self.assertEqual(library.renamex_np.call_args.args,
                         (os.fsencode(source), os.fsencode(destination), 0x00000004))
        self.assertEqual(library.renamex_np.argtypes, [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint])

    def test_reused_temporary_name_is_not_deleted_after_success(self):
        replaced_temporary = []
        def publish_then_reuse(source, destination):
            publish_directory_noreplace(source, destination)
            source.mkdir(mode=0o700)
            (source / "foreign").write_bytes(b"operator-owned")
            replaced_temporary.append(source)
        with patch("recovery.publish_directory_noreplace", side_effect=publish_then_reuse):
            self.backup()
        self.assertEqual((replaced_temporary[0] / "foreign").read_bytes(), b"operator-owned")
        verify(self.bundle)

    def test_disk_full_after_dump_and_final_sync_do_not_publish_backup(self):
        from recovery import sync_directory as real_sync
        def fail_parent(path):
            if path == self.private: raise OSError("synthetic full volume")
            return real_sync(path)
        with patch("recovery.sync_directory", side_effect=fail_parent):
            with self.assertRaises(OSError): self.backup()
        self.assertFalse(self.bundle.exists())
        with patch("recovery.private_json", side_effect=OSError("synthetic full volume")):
            with self.assertRaises(OSError): self.backup()
        self.assertFalse(self.bundle.exists())
    def test_changed_database_token_rejects_upgrade_even_with_identical_ledger(self):
        self.backup(); candidate = self.fixture.make_candidate("0.2.0")
        stale = self.database.facts() | {"database_change_token": "e" * 64}
        with patch.object(self.database, "facts", return_value=stale):
            with self.assertRaisesRegex(ReleaseError, "no longer matches"):
                upgrade_check(self.bundle, self.prefix, self.roots, self.database, candidate, exclusive=True)
    def test_manifest_extra_fields_cannot_be_exported_as_support_metadata(self):
        self.backup(); path = self.bundle / "manifest.json"
        value = json.loads(path.read_text()); value["secret"] = "FAKE_SECRET"
        path.write_text(json.dumps(value))
        with self.assertRaisesRegex(ReleaseError, "unsupported or incomplete"): verify(self.bundle)
    def test_restore_transaction_failure_leaves_no_apparent_complete_files(self):
        self.backup(); self.database.fail = True
        output = self.private / "failed-restore"
        with self.assertRaisesRegex(ReleaseError, "transactional restore failure"):
            restore(self.bundle, self.prefix, self.database, output, exclusive=True)
        self.assertFalse(output.exists()); self.assertFalse(self.database.restored)
        verify(self.bundle)

if __name__ == "__main__": unittest.main()
