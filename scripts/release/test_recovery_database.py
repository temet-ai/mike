"""Database commands are replaced with bounded synthetic subprocess output."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from common import ReleaseError
from recovery_database import Database, database_connection, execute
from test_recovery import LEDGER

class DatabaseCommandTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mike-recovery-db-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.service = self.root / "pg_service.conf"
        self.service.write_text("[fixture]\nhost=localhost\nport=5432\ndbname=db\nuser=mike\npassword=FAKE_SECRET\n")
        self.service.chmod(0o600)
        self.config = self.root / "config.toml"
        self.config.write_text('database_url="postgres://mike:FAKE_SECRET@localhost:5432/db"\n')
        self.config.chmod(0o600)

    def test_child_diagnostics_and_timeouts_are_private(self):
        with self.assertRaises(ReleaseError) as raised:
            execute(["/bin/sh", "-c", "echo 'postgres://FAKE_SECRET' >&2; exit 1"], {"PATH":"/usr/bin:/bin"})
        self.assertNotIn("FAKE_SECRET", str(raised.exception))
        with self.assertRaisesRegex(ReleaseError, "could not finish"):
            execute(["/bin/sleep", "10"], {}, timeout=0.02)

    def test_explicit_service_isolated_from_inherited_credentials(self):
        with patch.dict(os.environ, {"DATABASE_URL":"unwanted", "PGPASSWORD":"unwanted", "PGHOST":"unwanted"}):
            database = Database(service_file=self.service, service="fixture")
        for name in ("PGPASSWORD", "DATABASE_URL", "PGHOST"):
            self.assertNotIn(name, database.env)
        self.assertEqual(database.env["PGSERVICEFILE"], str(self.service))
        dump = self.root / "dump"; dump.write_bytes(b"PGDMPfixture")
        with patch("recovery_database.execute", return_value=b"") as run, patch.object(database, "require_fresh"):
            database.restore(dump)
        command = run.call_args.args[0]
        self.assertIn("--dbname=service=fixture", command)
        self.assertIn("--single-transaction", command)
        self.assertIn("--exit-on-error", command)
        self.assertNotIn("--clean", command)
        self.assertNotIn("--create", command)
        self.assertNotIn("FAKE_SECRET", " ".join(command))

    def test_source_service_must_match_config_and_connected_database(self):
        database = Database(service_file=self.service, service="fixture")
        with patch.object(database, "query", return_value='["db", "mike"]'):
            database.assert_source(self.config)
        with patch.object(database, "query", return_value='["foreign", "mike"]'):
            with self.assertRaisesRegex(ReleaseError, "connected database"):
                database.assert_source(self.config)
        self.service.write_text("[fixture]\nhost=foreign\ndbname=db\nuser=mike\n")
        with self.assertRaisesRegex(ReleaseError, "target differs"):
            database.assert_source(self.config)

    def test_source_overriding_query_parameters_refused(self):
        database = Database(service_file=self.service, service="fixture")
        self.config.write_text('database_url="postgres://mike@localhost/db?host=foreign"\n')
        with self.assertRaisesRegex(ReleaseError, "unambiguous"):
            database.assert_source(self.config)

    def test_queries_are_explicitly_read_only_and_ignore_psqlrc(self):
        database = Database(service_file=self.service, service="fixture")
        with patch("recovery_database.execute", return_value=b"190000\n") as run:
            self.assertEqual(database.query("SHOW server_version_num"), "190000")
        command = run.call_args.args[0]
        self.assertIn("--no-psqlrc", command)
        self.assertEqual(command[-1], "BEGIN READ ONLY; SHOW server_version_num; COMMIT")

    def test_failed_ledger_and_nonempty_target_refused(self):
        database = object.__new__(Database)
        database.query = lambda sql: "190000" if sql.startswith("SHOW") else json.dumps([{**LEDGER[0], "success": False}])
        with self.assertRaisesRegex(ReleaseError, "failed migration ledger"):
            database.facts()
        database.query = lambda sql: "190000" if sql.startswith("SHOW") else "1"
        with self.assertRaisesRegex(ReleaseError, "empty PostgreSQL"):
            database.require_fresh()

    def test_change_token_ignores_background_wal(self):
        # Checkpoints and autovacuum advance the WAL position on an idle database.
        import recovery_database
        self.assertNotIn("wal_lsn", recovery_database.CHANGE_SQL)
        self.assertIn("pg_current_snapshot", recovery_database.CHANGE_SQL)

    def test_database_freshness_changes_without_migration_changes(self):
        database = object.__new__(Database)
        def response(token):
            return lambda sql: "190000" if sql.startswith("SHOW") else (json.dumps(LEDGER) if "_sqlx_migrations" in sql else token)
        database.query = response("db/mike/0:1/1:2:")
        before = database.facts()
        database.query = response("db/mike/0:2/1:3:")
        after = database.facts()
        self.assertNotEqual(before["database_change_token"], after["database_change_token"])
        self.assertEqual(before["migration_ledger"], after["migration_ledger"])

    def test_remote_or_unowned_docker_is_refused(self):
        with patch.dict(os.environ, {"DOCKER_HOST":"tcp://remote:2375", "DOCKER_CONTEXT":""}):
            with self.assertRaisesRegex(ReleaseError, "local Docker"):
                Database(docker_identity="a" * 32)
        with patch.dict(os.environ, {"DOCKER_HOST":"unix:///fixture.sock", "DOCKER_CONTEXT":""}):
            with patch("recovery_database.execute", side_effect=[b'[{"Config":{"Labels":{}}}]', b'[{"Labels":{}}]']):
                with self.assertRaisesRegex(ReleaseError, "foreign"):
                    Database(docker_identity="a" * 32)

    def docker_records(self):
        identity = "a" * 32
        labels = {"io.temet.mike.installation":identity, "io.temet.mike.managed-database":"1",
                  "com.docker.compose.project": "mike-" + identity}
        container = {"Id": "b" * 64, "Config":{"Labels":labels},
                     "Mounts":[{"Name":f"mike-{identity}-data", "Destination":"/var/lib/postgresql/data"}],
                     "HostConfig":{"PortBindings":{"5432/tcp":[{"HostIp":"127.0.0.1", "HostPort":"6666"}]}}}
        return container, {"Labels":labels}

    def test_managed_target_pins_container_id_and_validated_local_endpoint(self):
        container, volume = self.docker_records()
        with patch.dict(os.environ, {"DOCKER_HOST":"tcp://ignored:2375", "DOCKER_CONTEXT":"fixture"}):
            with patch("recovery_database.execute", side_effect=[b'"unix:///fixture.sock"', json.dumps([container]).encode(), json.dumps([volume]).encode()]):
                database = Database(docker_identity="a" * 32)
        self.assertEqual(database.prefix[-1], "b" * 64)
        self.assertEqual(database.env["DOCKER_HOST"], "unix:///fixture.sock")
        self.assertNotIn("DOCKER_CONTEXT", database.env)
        self.config.write_text('database_url="postgres://mike@localhost:6666/mike"\n')
        with patch.object(database, "query", return_value='["mike", "mike"]'):
            database.assert_source(self.config)
        self.config.write_text('database_url="postgres://mike@localhost:9999/mike"\n')
        with self.assertRaisesRegex(ReleaseError, "does not match"):
            database.assert_source(self.config)

    def test_managed_storage_or_public_binding_refused(self):
        for change in ("mount", "port"):
            container, volume = self.docker_records()
            if change == "mount": container["Mounts"][0]["Name"] = "foreign"
            else: container["HostConfig"]["PortBindings"]["5432/tcp"][0]["HostIp"] = "0.0.0.0"
            with self.subTest(change=change), patch.dict(os.environ, {"DOCKER_HOST":"unix:///fixture.sock", "DOCKER_CONTEXT":""}):
                with patch("recovery_database.execute", side_effect=[json.dumps([container]).encode(), json.dumps([volume]).encode()]):
                    with self.assertRaisesRegex(ReleaseError, "storage or loopback"):
                        Database(docker_identity="a" * 32)

    def test_service_snapshot_cannot_be_redirected_and_is_cleaned(self):
        with database_connection(service_file=self.service, service="fixture") as database:
            snapshot = Path(database.env["PGSERVICEFILE"])
            self.assertNotEqual(snapshot, self.service)
            self.assertEqual(snapshot.stat().st_mode & 0o777, 0o600)
            self.service.write_text("[fixture]\nhost=foreign\n")
            self.assertIn("host=localhost", snapshot.read_text())
            with patch.object(database, "query", return_value='["db", "mike"]'):
                database.assert_source(self.config)
        self.assertFalse(snapshot.exists())

if __name__ == "__main__": unittest.main()
