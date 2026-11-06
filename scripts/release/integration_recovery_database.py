"""Explicit PG19 backend recovery integration, never installed AC21–AC24 qualification.

Uses real psql/pg_dump/pg_restore with synthetic rows and private files. Installed
archive ownership is real but binaries, authoritative paths and database-check
responses are synthetic. No Mike daemon, production migrations, Docker, UI/MCP,
Cargo, network service outside the owned loopback fixture, or live data is used.
"""

import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

# The isolated entrypoint imports only its explicit adjacent release modules.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from common import ReleaseError
from install import install
from integration_recovery_fixture import Cluster, LEDGER
from recovery import (backup, installation_id, restore, selected_roots, upgrade_check, verify)
from recovery_database import EMPTY_SQL
from recovery_files import file_digest, inventory
from test_install import InstallTests

CLUSTER = None


class RecoveryDatabaseBootstrapTests(unittest.TestCase):
    def assert_rejected_before_commands(self, values, message):
        with patch.dict(os.environ, values, clear=True), patch("subprocess.run") as command:
            with self.assertRaisesRegex(RuntimeError, message):
                Cluster()
            command.assert_not_called()

    def test_missing_or_blank_disposable_configuration_fails_closed(self):
        for values in ({}, {"MIKE_TEST_ADMIN_URL": " "},
                       {"MIKE_TEST_ADMIN_URL": "postgres://postgres@127.0.0.1:56667/postgres?sslmode=disable"}):
            with self.subTest(values_present=sorted(values)):
                self.assert_rejected_before_commands(values, "required; no tests skipped")

    def test_live_defaults_remote_hosts_credentials_and_database_redirects_are_refused(self):
        for address in ("127.0.0.1:5432/postgres", "127.0.0.1:5440/postgres",
                        "127.0.0.1:56667/claude_proxy", "remote.invalid:56667/postgres",
                        "localhost:56667/postgres", "synthetic:secret@127.0.0.1:56667/postgres"):
            with self.subTest(target_kind=address.split("/")[-1]):
                self.assert_rejected_before_commands({
                    "MIKE_TEST_ADMIN_URL": "postgres://postgres@" + address + "?sslmode=disable",
                    "MIKE_RECOVERY_TEST_PGDATA": "/unused-owned-fixture"}, "loopback disposable admin URL")

    def test_unowned_layout_and_wrong_pgdata_major_are_refused_before_connection(self):
        with tempfile.TemporaryDirectory(prefix="recovery-guard-") as temporary:
            data = Path(temporary)
            values = {"MIKE_TEST_ADMIN_URL": "postgres://postgres@127.0.0.1:56667/postgres?sslmode=disable",
                      "MIKE_RECOVERY_TEST_PGDATA": str(data)}
            data.chmod(0o755)
            self.assert_rejected_before_commands(values, "owned private")
            data.chmod(0o700)
            (data / "PG_VERSION").write_text("16\n")
            self.assert_rejected_before_commands(values, "must be PostgreSQL 19")


class RecoveryDatabaseIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.cluster = CLUSTER
        self.addCleanup(self.cluster.cleanup)
        self.source = self.cluster.create(seeded=True)
        self.target = self.cluster.create()
        self.installation = InstallTests()
        self.installation.setUp()
        self.addCleanup(self.installation.doCleanups)
        self.prefix = self.installation.prefix
        install(self.installation.candidate, self.prefix)
        self.root = self.installation.base / "private"
        self.root.mkdir(mode=0o700)
        self.config = self.root / "config.toml"
        self.config.write_text('persist_enabled=true\ndatabase_url=' +
                               json.dumps(self.cluster.config_url(self.source)) + '\n')
        self.config.chmod(0o600)
        self.data, self.state = self.root / "data", self.root / "state"
        self.data.mkdir(mode=0o700)
        self.state.mkdir(mode=0o700)
        (self.data / "bodies").mkdir(mode=0o700)
        (self.data / "bodies" / "synthetic-unicode-café").write_bytes(b"\x00\xff\x00synthetic raw body\n")
        (self.data / "empty").mkdir(mode=0o700)
        (self.state / "database").mkdir(mode=0o700)
        (self.state / "database" / "fixture-identity").write_text("synthetic-state-identity\n")
        (self.state / "durable-queue").write_text('{"id":"synthetic-queued-item"}\n')
        for path in self.root.rglob("*"):
            if path.is_file():
                path.chmod(0o600)
        self.roots = selected_roots(self.config, self.data, self.state)
        self.source_service = self.cluster.service(self.root / "source.service", self.source)
        self.target_service = self.cluster.service(self.root / "target.service", self.target)
        self.bundle, self.output = self.root / "backup", self.root / "restored"
        paths = {"config_file": str(self.config), "installation_id": installation_id(self.config),
                 "data_dir": str(self.data), "state_dir": str(self.state),
                 "log_dir": str(self.state / "logs")}
        self.paths = self.enterContext(patch("recovery.runtime_paths", return_value=paths))
        self.runtime = self.enterContext(patch("recovery.runtime_command", return_value=b"synthetic check only"))

    def backup(self, *, destination=None):
        with self.cluster.connection(self.source_service) as database:
            return backup(self.prefix, self.roots, database, destination or self.bundle, exclusive=True)

    def restore(self, *, service=None, output=None):
        with self.cluster.connection(service or self.target_service) as database:
            return restore(self.bundle, self.prefix, database, output or self.output, exclusive=True)

    def assert_fresh(self, database=None):
        name = database or self.target
        self.assertEqual(self.cluster.sql(name, EMPTY_SQL), "0")

    def assert_no_incomplete_output(self):
        self.assertFalse(self.output.exists())
        self.assertEqual(list(self.root.glob(".mike-recovery-incomplete-*")), [])

    def test_real_backup_and_restore_preserve_exact_rows_files_and_selected_target(self):
        decoy = self.cluster.create(seeded=True)
        self.cluster.sql(decoy, "UPDATE capture.requests SET status=503;")
        source_before, decoy_before = self.cluster.snapshot(self.source), self.cluster.snapshot(decoy)
        files_before = inventory(self.roots)
        with patch.dict(os.environ, {"PGHOST": "unwanted.invalid", "PGPORT": "1",
                "PGDATABASE": decoy, "PGUSER": "unwanted", "PGPASSWORD": "synthetic-unwanted",
                "PGSERVICE": "unwanted", "DATABASE_URL": self.cluster.config_url(decoy)}):
            receipt = self.backup()
            manifest, index = verify(self.bundle)
            self.assertEqual(index["migration_ledger"], LEDGER)
            self.assertEqual(index["files"], files_before)
            self.assertEqual(manifest["database_dump"], file_digest(self.bundle / "database.dump"))
            self.assertEqual(manifest["private_index"], file_digest(self.bundle / "restore-index.json"))
            self.assertEqual(receipt["status"], "backup-complete")
            self.assertFalse(receipt["qualified"])
            restored = self.restore()
        self.assertEqual(restored["status"], "restored-offline-review-required")
        self.assertFalse(restored["qualified"])
        self.runtime.assert_called_once_with(self.prefix, self.config, ["database", "check"])
        self.assertEqual(self.cluster.snapshot(self.target), source_before)
        self.assertEqual(self.cluster.snapshot(self.source), source_before)
        self.assertEqual(self.cluster.snapshot(decoy), decoy_before)
        for entry in index["files"]:
            target = self.output / ("config.toml" if entry["scope"] == "config"
                                   else entry["scope"] + "/" + entry["path"])
            if entry["kind"] == "file":
                self.assertEqual(file_digest(target), {key: entry[key] for key in ("size", "sha256")})
            else:
                self.assertTrue(target.is_dir())
        self.assertEqual((self.output / "config.toml").read_bytes(), self.config.read_bytes())
        self.assertFalse((self.output / "database.dump").exists())
        self.assertFalse((self.output / "state" / "daemon.lock").exists())
        for root in (self.bundle, self.output):
            for path in [root, *root.rglob("*")]:
                self.assertEqual(path.stat().st_mode & 0o077, 0)
        public = (self.bundle / "manifest.json").read_text() + json.dumps(restored)
        for private in (self.source, str(self.root), "synthetic-state", "postgres://", "durable-queue"):
            self.assertNotIn(private, public)

    def test_nonempty_user_relations_are_refused_without_mutation(self):
        self.backup()
        definitions = {
            "table": "CREATE TABLE retained(id integer); INSERT INTO retained VALUES (17);",
            "partitioned": "CREATE TABLE retained(id integer) PARTITION BY RANGE(id);",
            "view": "CREATE VIEW retained AS SELECT 17 AS id;",
            "materialized": "CREATE MATERIALIZED VIEW retained AS SELECT 17 AS id;",
            "sequence": "CREATE SEQUENCE retained START 17;",
            "foreign": "CREATE FOREIGN DATA WRAPPER retained; CREATE SERVER retained FOREIGN DATA WRAPPER retained; "
                       "CREATE FOREIGN TABLE retained(id integer) SERVER retained;",
        }
        for kind, sql in definitions.items():
            with self.subTest(kind=kind):
                target = self.cluster.create()
                self.cluster.sql(target, sql)
                service = self.cluster.service(self.root / (kind + ".service"), target)
                before = self.cluster.sql(target, "SELECT oid,relname,relkind FROM pg_class "
                                          "WHERE relname='retained' ORDER BY oid;")
                with self.assertRaisesRegex(ReleaseError, "empty PostgreSQL 19"):
                    self.restore(service=service)
                self.assertEqual(before, self.cluster.sql(target, "SELECT oid,relname,relkind FROM pg_class "
                                                         "WHERE relname='retained' ORDER BY oid;"))
                self.assert_no_incomplete_output()
        self.assert_fresh()
        verify(self.bundle)

    def test_existing_file_destination_is_refused_before_database_mutation(self):
        self.backup()
        self.output.mkdir(mode=0o700)
        marker = self.output / "retained"
        marker.write_bytes(b"untouched")
        with self.assertRaisesRegex(ReleaseError, "already exists"):
            self.restore()
        self.assertEqual(marker.read_bytes(), b"untouched")
        self.assert_fresh()

    def test_source_database_and_role_must_match_selected_config(self):
        before = self.cluster.snapshot(self.source)
        original = self.config.read_text()
        for url in (self.cluster.config_url(self.target),
                    self.cluster.config_url(self.source).replace("postgres@", "wrong_role@")):
            with self.subTest(url_kind="target" if self.target in url else "role"):
                self.config.write_text('persist_enabled=true\ndatabase_url=' + json.dumps(url) + '\n')
                with self.assertRaisesRegex(ReleaseError, "target differs"):
                    self.backup()
                self.assertFalse(self.bundle.exists())
        self.config.write_text(original)
        self.assertEqual(self.cluster.snapshot(self.source), before)
        self.assert_fresh()

    def test_frozen_service_file_cannot_redirect_real_dump_or_restore(self):
        original = self.source_service.read_text()
        with self.cluster.connection(self.source_service) as database:
            frozen = Path(database.env["PGSERVICEFILE"])
            self.cluster.service(self.source_service, self.target)
            database.assert_source(self.config)
            backup(self.prefix, self.roots, database, self.bundle, exclusive=True)
            self.assertIn(self.source, frozen.read_text())
        self.assertFalse(frozen.exists())
        self.source_service.write_text(original)
        with self.cluster.connection(self.target_service) as database:
            frozen = Path(database.env["PGSERVICEFILE"])
            self.cluster.service(self.target_service, self.source)
            restore(self.bundle, self.prefix, database, self.output, exclusive=True)
            self.assertIn(self.target, frozen.read_text())
        self.assertFalse(frozen.exists())
        self.assertEqual(self.cluster.snapshot(self.target), self.cluster.snapshot(self.source))

    def test_failed_missing_empty_and_invalid_ledgers_never_publish(self):
        changes = {
            "failed": "UPDATE _sqlx_migrations SET success=false WHERE version=20260604000002;",
            "empty": "DELETE FROM _sqlx_migrations;",
            "bad-checksum": "UPDATE _sqlx_migrations SET checksum=decode('00','hex');",
            "nonpositive": "UPDATE _sqlx_migrations SET version=0 WHERE version=20260604000001;",
            "missing": "DROP TABLE _sqlx_migrations;",
        }
        for name, sql in changes.items():
            with self.subTest(ledger=name):
                source = self.cluster.create(seeded=True)
                service = self.cluster.service(self.root / (name + ".service"), source)
                self.config.write_text('persist_enabled=true\ndatabase_url=' +
                                       json.dumps(self.cluster.config_url(source)) + '\n')
                self.cluster.sql(source, sql)
                with self.cluster.connection(service) as database:
                    with self.assertRaises(ReleaseError):
                        backup(self.prefix, self.roots, database, self.bundle, exclusive=True)
                self.assertFalse(self.bundle.exists())
                self.assertEqual(list(self.root.glob(".mike-recovery-incomplete-*")), [])
        self.assert_fresh()

    def test_byte_tampering_fails_verification_before_real_restore(self):
        self.backup()
        for path in (self.bundle / "database.dump", self.bundle / "files/00000000"):
            with self.subTest(component=path.name):
                original = path.read_bytes()
                path.write_bytes(original + b"synthetic corruption")
                with self.assertRaisesRegex(ReleaseError, "checksum"):
                    self.restore()
                self.assert_fresh()
                self.assert_no_incomplete_output()
                path.write_bytes(original)
        verify(self.bundle)

    def test_real_sql_restore_error_rolls_back_all_imported_objects(self):
        self.cluster.sql(self.source, "CREATE FUNCTION public.fixture_conflict() RETURNS integer "
                         "LANGUAGE sql AS 'SELECT 1';")
        self.backup()
        self.cluster.sql(self.target, "CREATE FUNCTION public.fixture_conflict() RETURNS integer "
                         "LANGUAGE sql AS 'SELECT 99';")
        with self.assertRaisesRegex(ReleaseError, "database maintenance command failed"):
            self.restore()
        self.assert_fresh()
        self.assertEqual(self.cluster.sql(self.target, "SELECT fixture_conflict();"), "99")
        self.assertEqual(self.cluster.sql(self.target,
                         "SELECT count(*) FROM pg_namespace WHERE nspname='capture';"), "0")
        self.assert_no_incomplete_output()
        verify(self.bundle)

    def test_real_dump_failure_discards_private_stage_and_retains_previous_backup(self):
        self.backup()
        prior = (self.bundle / "manifest.json").read_bytes()
        with self.cluster.connection(self.source_service) as database:
            actual_dump = database.dump
            def invalid_tool_option(destination):
                # Only the real pg_dump invocation receives this failing option;
                # source identity/facts still exercise the real database first.
                database.connection.append("--fixture-invalid-pg-dump-option")
                try:
                    actual_dump(destination)
                finally:
                    database.connection.pop()
            database.dump = invalid_tool_option
            with self.assertRaisesRegex(ReleaseError, "database maintenance command failed"):
                backup(self.prefix, self.roots, database, self.root / "failed-backup", exclusive=True)
        self.assertFalse((self.root / "failed-backup").exists())
        self.assertEqual((self.bundle / "manifest.json").read_bytes(), prior)
        self.assert_no_incomplete_output()
        verify(self.bundle)

    def test_writes_during_backup_are_detected_without_publishing(self):
        with self.cluster.connection(self.source_service) as database:
            actual_dump = database.dump
            def dump_then_write(destination):
                actual_dump(destination)
                self.cluster.sql(self.source, "UPDATE capture.requests SET status=201;")
            database.dump = dump_then_write
            with self.assertRaisesRegex(ReleaseError, "state changed"):
                backup(self.prefix, self.roots, database, self.bundle, exclusive=True)
        self.assertFalse(self.bundle.exists())
        self.assert_no_incomplete_output()
        self.assert_fresh()

    def test_later_data_write_invalidates_upgrade_check_without_ledger_change(self):
        self.backup()
        candidate = self.installation.make_candidate("0.2.0")
        with self.cluster.connection(self.source_service) as database:
            before = database.facts()
            self.cluster.sql(self.source, "UPDATE capture.requests SET status=202;")
            after = database.facts()
            self.assertEqual(before["migration_ledger"], after["migration_ledger"])
            self.assertNotEqual(before["database_change_token"], after["database_change_token"])
            with self.assertRaisesRegex(ReleaseError, "backup no longer matches"):
                upgrade_check(self.bundle, self.prefix, self.roots, database, candidate, exclusive=True)
        self.assert_fresh()
        verify(self.bundle)

    def test_post_import_ledger_mismatch_leaves_target_offline_and_no_file_output(self):
        self.backup()
        index_path, manifest_path = self.bundle / "restore-index.json", self.bundle / "manifest.json"
        index, manifest = json.loads(index_path.read_text()), json.loads(manifest_path.read_text())
        index["migration_ledger"][0]["checksum"] = "c" * 96
        index_path.write_text(json.dumps(index))
        manifest["private_index"] = file_digest(index_path)
        manifest["migration_ledger"] = hashlib.sha256(
            json.dumps(index["migration_ledger"], sort_keys=True).encode()).hexdigest()
        manifest_path.write_text(json.dumps(manifest))
        verify(self.bundle)
        with self.assertRaisesRegex(ReleaseError, "restored ledger differs"):
            self.restore()
        self.assert_no_incomplete_output()
        self.assertEqual(self.cluster.snapshot(self.target), self.cluster.snapshot(self.source))
        with self.assertRaisesRegex(ReleaseError, "empty PostgreSQL 19"):
            self.restore()
        self.assert_no_incomplete_output()

    def test_wrong_installed_binary_generation_refused_before_database_mutation(self):
        self.backup()
        install(self.installation.make_candidate("0.2.0"), self.prefix)
        with self.assertRaisesRegex(ReleaseError, "exact backed-up binary"):
            self.restore()
        self.assert_fresh()
        self.assert_no_incomplete_output()


def main():
    global CLUSTER
    try:
        CLUSTER = Cluster()
    except (OSError, ValueError, RuntimeError) as error:
        print("recovery DB integration refused: " + str(error), file=sys.stderr)
        return 2
    print("Backend integration only: real PostgreSQL 19 dump/restore; synthetic installed paths/checks. "
          "Installed AC21–AC24, production migrations, UI/MCP, and native macOS remain NOT RUN.", flush=True)
    print("Client tool selection: " + ("explicit test-only relocated tools" if
          os.environ.get("MIKE_RECOVERY_TEST_TOOL_DIR") else "runtime fixed tool path"), flush=True)
    print("Server version_num: " + CLUSTER.server_version, flush=True)
    for version in CLUSTER.client_versions.values():
        print(version, flush=True)
    suite = unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromTestCase(case)
                               for case in (RecoveryDatabaseBootstrapTests, RecoveryDatabaseIntegrationTests))
    try:
        result = unittest.TextTestRunner(verbosity=2).run(suite)
    finally:
        CLUSTER.cleanup()
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
