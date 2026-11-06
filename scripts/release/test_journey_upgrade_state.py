"""Bounded offline contracts; fixture shells are not release evidence or daemons."""
from contextlib import contextmanager
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from common import sha256
from inputs import verified_inputs
from journey_process import JourneyError
from journey_upgrade_state import (Archive, compatible_pair, database_digest, durable_snapshot,
                                   interrupted_install, refuse_install, tree_digest)
from migration_inventory import SOURCE_PATH
import test_install


def manifest(version="0.1.0", source="a", count=1):
    return {"version": version, "target": "x86_64-unknown-linux-gnu", "source_commit": source * 40,
            "migration_inventory": {"schema_version": 1, "source_commit": source * 40,
                "source_path": SOURCE_PATH, "checksum_algorithm": "sha384",
                "migrations": [{"version": i, "filename": f"{i}_fixture.sql", "checksum": str(i) * 96}
                               for i in range(1, count + 1)]}}


class PairTests(unittest.TestCase):
    def setUp(self):
        self.previous = manifest()
        self.candidate = manifest("0.2.0", "b", 2)

    def test_requires_actual_newer_schema_and_distinct_source(self):
        value = compatible_pair(self.previous, self.candidate)
        self.assertTrue(value["incompatible_predecessor_schema_is_real"])
        self.assertEqual(value["candidate_migrations"], 2)

    def test_same_schema_target_version_commit_or_modified_prefix_cannot_qualify(self):
        variants = [manifest("0.2.0", "b", 1), manifest("0.1.0", "b", 2), manifest("0.0.9", "b", 2),
                    manifest("0.2.0", "a", 2), self.candidate | {"target": "aarch64-apple-darwin"}]
        modified = copy.deepcopy(self.candidate)
        modified["migration_inventory"]["migrations"][0]["checksum"] = "c" * 96
        variants.append(modified)
        for candidate in variants:
            with self.subTest(candidate=candidate), self.assertRaises(JourneyError):
                compatible_pair(self.previous, candidate)


class ExactInstallerTests(unittest.TestCase):
    def setUp(self):
        self.fixture = test_install.InstallTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.base
        self.home = self.root / "private-home"
        self.home.mkdir(mode=0o700)
        self.cwd = self.root / "unrelated-cwd"
        self.cwd.mkdir(mode=0o700)

    @contextmanager
    def pair(self):
        newer = self.fixture.make_candidate("0.2.0")
        with verified_inputs(self.fixture.candidate) as (old_snapshot, old_manifest), \
                verified_inputs(newer) as (new_snapshot, new_manifest):
            old = Archive(old_snapshot, old_manifest, sha256(old_snapshot / "manifest.json"))
            new = Archive(new_snapshot, new_manifest, sha256(new_snapshot / "manifest.json"))
            old.select(self.fixture.prefix)
            yield old, new

    def test_real_sigkill_before_after_paired_swap_then_idempotent_retry(self):
        with self.pair() as (old, new):
            original = old.paths.copy()
            first = interrupted_install(new, old.prefix, "before_swap", self.root, self.home, self.cwd)
            self.assertEqual(first["owned_installer_exit_status"], -9)
            old.recheck()
            self.assertEqual(old.paths, original)
            second = interrupted_install(new, old.prefix, "after_swap", self.root, self.home, self.cwd)
            self.assertEqual(second["migration_interruption"], "NOT_RUN")
            new.bind(old.prefix)
            old.recheck(active=False)
            before = tree_digest(new.prefix)
            self.assertEqual(new.select(old.prefix)["status"], "unchanged")
            self.assertEqual(tree_digest(new.prefix), before)
            self.assertTrue(refuse_install(old, new)["complete_install_tree_unchanged"])

    def test_marker_collision_prevents_any_process_execution(self):
        marker = self.root / "before_swap.marker"
        marker.write_bytes(b"unrelated")
        with patch("journey_upgrade_state.OwnedProcess") as process:
            with self.assertRaisesRegex(JourneyError, "MARKER_EXISTS"):
                interrupted_install(Mock(), self.fixture.prefix, "before_swap", self.root, self.home, self.cwd)
        process.assert_not_called()
        self.assertEqual(marker.read_bytes(), b"unrelated")

    def test_snapshot_mismatch_is_refused_without_install_or_execute(self):
        with patch("journey_upgrade_state.install_verified") as install:
            with self.assertRaisesRegex(JourneyError, "SNAPSHOT_CHANGED"):
                Archive(self.fixture.candidate, {}, "0" * 64)
        install.assert_not_called()

    def test_retained_bytes_and_link_changes_are_detected(self):
        with self.pair() as (old, _):
            original = tree_digest(old.prefix)
            link = old.prefix / "bin/mike"
            link.unlink()
            link.symlink_to("/outside/never-followed")
            self.assertNotEqual(tree_digest(old.prefix), original)
            with self.assertRaises(Exception):
                old.recheck()

    def test_changed_frozen_archive_fails_before_installer_process_or_prefix_mutation(self):
        with self.pair() as (old, new):
            artifact = new.snapshot / new.manifest["artifacts"][0]["archive"]
            artifact.write_bytes(artifact.read_bytes() + b"changed container bytes")
            original = tree_digest(old.prefix)
            with patch("journey_upgrade_state.OwnedProcess") as process, self.assertRaisesRegex(JourneyError, "SNAPSHOT_ARCHIVE_CHANGED"):
                interrupted_install(new, old.prefix, "before_swap", self.root, self.home, self.cwd)
            process.assert_not_called()
            self.assertEqual(tree_digest(old.prefix), original)


class DatabaseFingerprintTests(unittest.TestCase):
    def setUp(self):
        self.cluster = Mock(created=["owned"])
        connection = Mock(connection=["--dbname=service=fixture"], env={})
        connection.command.return_value = ["/explicit/pg_dump"]
        @contextmanager
        def connected(_):
            yield connection
        self.cluster.connection.side_effect = connected
        self.execute = self.enterContext(patch("journey_upgrade_state.execute"))

    def dump(self, token, body=b"INSERT INTO synthetic VALUES (1);\n"):
        return b"--\n-- PostgreSQL database dump\n\n\\restrict " + token + b"\n" + body + b"\\unrestrict " + token + b"\n"

    def test_only_random_restrict_framing_is_normalized(self):
        self.execute.side_effect = [self.dump(b"a1"), self.dump(b"b2"), self.dump(b"c3", b"INSERT INTO synthetic VALUES (2);\n")]
        before = database_digest(self.cluster, "owned", Path("/private/service"))
        self.assertEqual(database_digest(self.cluster, "owned", Path("/private/service")), before)
        self.assertNotEqual(database_digest(self.cluster, "owned", Path("/private/service")), before)
        self.assertEqual(self.execute.call_args.kwargs["timeout"], 60)
        self.assertIn("--format=plain", self.execute.call_args.args[0])
        self.assertIn("--create", self.execute.call_args.args[0])
        self.assertNotIn("--no-owner", self.execute.call_args.args[0])
        self.assertNotIn("--no-acl", self.execute.call_args.args[0])

    def test_unknown_database_is_refused_before_any_client(self):
        with self.assertRaisesRegex(JourneyError, "DATABASE_NOT_OWNED"):
            database_digest(self.cluster, "foreign", Path("/private/service"))
        self.cluster.connection.assert_not_called()
        self.execute.assert_not_called()

    def test_malformed_dump_or_restrict_pair_cannot_pass(self):
        for raw in (b"not a dump", self.dump(b"a1").replace(b"\\unrestrict a1", b"\\unrestrict b2")):
            self.execute.return_value = raw
            with self.subTest(raw=raw), self.assertRaises(JourneyError):
                database_digest(self.cluster, "owned", Path("/private/service"))

    def test_durable_snapshot_detects_content_and_mode_changes_but_labels_lifecycle_exclusion(self):
        with tempfile.TemporaryDirectory(prefix="m-up-state-") as temporary:
            root = Path(temporary)
            roots = {"config": root / "config.toml", "data": root / "data", "state": root / "state"}
            roots["config"].write_bytes(b"owned fixture config\n")
            roots["config"].chmod(0o600)
            for name in ("data", "state"):
                roots[name].mkdir(mode=0o700)
            value = roots["data"] / "sentinel"
            value.write_bytes(b"original")
            value.chmod(0o600)
            self.execute.return_value = self.dump(b"same")
            before = durable_snapshot(self.cluster, "owned", Path("/private/service"), roots)
            value.chmod(0o640)
            changed = durable_snapshot(self.cluster, "owned", Path("/private/service"), roots)
            self.assertEqual(changed["files_sha256"], before["files_sha256"])
            self.assertNotEqual(changed["file_modes_sha256"], before["file_modes_sha256"])
            value.chmod(0o600)
            (roots["state"] / "daemon.lock").write_bytes(b"ephemeral lifecycle")
            self.assertEqual(durable_snapshot(self.cluster, "owned", Path("/private/service"), roots), before)
            value.write_bytes(b"changed")
            self.assertNotEqual(durable_snapshot(self.cluster, "owned", Path("/private/service"), roots), before)


if __name__ == "__main__":
    unittest.main()
