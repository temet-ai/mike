"""Hermetic host-tool selection: local executable fixtures, never a DB or Docker."""

import json
import os
import stat
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from common import ReleaseError
from recovery_database import Database, database_connection
from recovery_tools import selected_tools
from test_recovery import LEDGER

TOOL_NAMES = ("psql", "pg_dump", "pg_restore", "docker")


def fixture_tools(directory, record):
    directory.mkdir(mode=0o700)
    script = '''import json, os, sys
from pathlib import Path
name, args = Path(sys.argv[0]).name, sys.argv[1:]
with Path(RECORD).open("a") as output:
    output.write(json.dumps({"name": name, "args": args, "env": dict(os.environ)}) + "\\n")
if name == "docker":
    identity = "a" * 32
    labels = {"io.temet.mike.installation": identity, "io.temet.mike.managed-database": "1",
              "com.docker.compose.project": "mike-" + identity}
    if args[:2] == ["context", "inspect"]:
        print(json.dumps("unix:///synthetic-fixture.sock")); sys.exit(0)
    if args[:2] == ["container", "inspect"]:
        print(json.dumps([{"Id": "b" * 64, "Config": {"Labels": labels},
            "Mounts": [{"Name": "mike-" + identity + "-data", "Destination": "/var/lib/postgresql/data"}],
            "HostConfig": {"PortBindings": {"5432/tcp": [{"HostIp": "127.0.0.1", "HostPort": "6666"}]}}}]))
        sys.exit(0)
    if args[:2] == ["volume", "inspect"]:
        print(json.dumps([{"Labels": labels}])); sys.exit(0)
    assert args[:3] == ["exec", "-i", "b" * 64]
    name, args = args[3], args[4:]
if name == "psql":
    sql = args[-1]
    if "SHOW server_version_num" in sql: print("190000")
    elif "SELECT count(*)" in sql: print("0")
    elif "_sqlx_migrations" in sql: print(json.dumps(LEDGER))
    elif "json_build_array" in sql: print('["db", "mike"]')
    else: print("db/mike/0:1/1:2:")
elif name == "pg_dump": sys.stdout.buffer.write(b"PGDMPfixture-private-data")
elif name == "pg_restore": assert sys.stdin.buffer.read().startswith(b"PGDMP")
else: raise AssertionError("unexpected fixture command")
'''
    content = (f"#!{sys.executable}\nRECORD = {str(record)!r}\nLEDGER = {LEDGER!r}\n" + script)
    for name in TOOL_NAMES:
        path = directory / name
        path.write_text(content)
        path.chmod(0o755)
    return directory


class ToolSelectionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mike-tool-selection-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.record = self.root / "commands.jsonl"
        self.tools = fixture_tools(self.root / "tools with spaces", self.record)
        self.service = self.root / "pg_service.conf"
        self.service.write_text("[fixture]\nhost=localhost\nport=5432\ndbname=db\nuser=mike\npassword=FAKE_SECRET\n")
        self.service.chmod(0o600)

    def records(self):
        return [json.loads(line) for line in self.record.read_text().splitlines()]

    def test_explicit_postgres_tools_run_without_ambient_path_or_credentials(self):
        poison = fixture_tools(self.root / "ambient-poison", self.root / "unexpected-execution")
        inherited = {"PATH": str(poison), "PGPASSWORD": "FAKE_SECRET", "PGHOST": "foreign",
                     "DATABASE_URL": "postgres://FAKE_SECRET", "PYTHONPATH": str(poison),
                     "LD_PRELOAD": "/not-a-library", "DYLD_LIBRARY_PATH": str(poison)}
        with patch.dict(os.environ, inherited):
            with database_connection(service_file=self.service, service="fixture", tool_directory=self.tools) as database:
                self.assertEqual(database.env["PATH"], "/usr/bin:/bin")
                snapshot = database.env["PGSERVICEFILE"]
                self.assertNotEqual(snapshot, str(self.service))
                dump = self.root / "dump"
                database.dump(dump)
                database.restore(dump)
                self.assertEqual(database.facts()["migration_ledger"], LEDGER)
        self.assertFalse((self.root / "unexpected-execution").exists())
        self.assertFalse(Path(snapshot).exists())
        records = self.records()
        self.assertEqual({r["name"] for r in records}, set(TOOL_NAMES[:3]))
        for record in records:
            self.assertEqual(record["env"]["PGSERVICEFILE"], snapshot)
            for name in inherited.keys() - {"PATH"}:
                self.assertNotIn(name, record["env"])
            self.assertNotIn("FAKE_SECRET", json.dumps(record))

    def test_explicit_docker_tool_runs_container_tools_without_host_pg_tools(self):
        for name in TOOL_NAMES[:3]:
            (self.tools / name).unlink()
        with patch.dict(os.environ, {"DOCKER_CONTEXT": "synthetic", "DOCKER_HOST": "tcp://ignored:2375",
                                     "PGPASSWORD": "FAKE_SECRET", "PATH": "/missing-tools"}):
            database = Database(docker_identity="a" * 32, tool_directory=self.tools)
            database.dump(self.root / "dump")
            database.restore(self.root / "dump")
        self.assertEqual(database.prefix[0], str(self.tools / "docker"))
        records = self.records()
        self.assertEqual({r["name"] for r in records}, {"docker"})
        inside = [r for r in records if r["args"][0] == "exec"]
        self.assertEqual({r["args"][3] for r in inside}, set(TOOL_NAMES[:3]))
        self.assertTrue(all(r["args"][2] == "b" * 64 for r in inside))
        for record in inside:
            self.assertNotIn("DOCKER_CONTEXT", record["env"])
            self.assertNotIn("PGPASSWORD", record["env"])
            self.assertEqual(record["env"]["DOCKER_HOST"], "unix:///synthetic-fixture.sock")
            self.assertEqual(record["env"]["PATH"], "/usr/bin:/bin")

    def test_default_keeps_fixed_path_and_explicit_selection_is_not_a_search_path(self):
        with patch.dict(os.environ, {"PATH": str(self.tools)}):
            database = Database(service_file=self.service, service="fixture")
        self.assertEqual(database.env["PATH"], "/usr/bin:/bin")
        self.assertEqual(database.command("psql"), ["psql"])
        with patch("recovery_database.selected_tools", wraps=selected_tools) as select:
            with database_connection(service_file=self.service, service="fixture", tool_directory=self.tools) as database:
                self.assertEqual(database.command("psql"), [str(self.tools / "psql")])
                self.assertEqual(database.command("pg_dump"), [str(self.tools / "pg_dump")])
            select.assert_called_once_with(self.tools, TOOL_NAMES[:3])
        (self.tools / "pg_dump").unlink()
        with self.assertRaisesRegex(ReleaseError, "missing required"):
            Database(service_file=self.service, service="fixture", tool_directory=self.tools)
        self.assertFalse(self.record.exists())

    def test_relative_traversing_symlink_and_shared_directories_are_refused(self):
        alias = self.root / "alias"
        alias.symlink_to(self.tools, target_is_directory=True)
        intermediate = self.root / "intermediate"
        intermediate.symlink_to(self.root, target_is_directory=True)
        for directory in (Path("."), self.tools / ".." / self.tools.name, alias,
                          intermediate / self.tools.name):
            with self.subTest(directory=directory), self.assertRaises(ReleaseError):
                selected_tools(directory, TOOL_NAMES[:3])
        for mode in (0o775, 0o777, 0o1777):
            self.tools.chmod(mode)
            with self.subTest(mode=mode), self.assertRaises(ReleaseError):
                selected_tools(self.tools, TOOL_NAMES[:3])
        self.tools.chmod(0o755)
        self.root.chmod(0o775)
        try:
            with self.assertRaises(ReleaseError):
                selected_tools(self.tools, TOOL_NAMES[:3])
        finally:
            self.root.chmod(0o700)

    def test_unsafe_executable_files_are_refused(self):
        executable = self.tools / "psql"
        for mode in (0o644, 0o777, 0o4755, 0o2755):
            executable.chmod(mode)
            with self.subTest(mode=mode):
                if stat.S_IMODE(executable.stat().st_mode) != mode:
                    self.skipTest(f"this system did not set mode {mode:o} (macOS drops setgid outside the file's group)")
                with self.assertRaises(ReleaseError):
                    selected_tools(self.tools, TOOL_NAMES[:3])
        executable.chmod(0o755)
        outside = self.root / "outside"
        executable.rename(outside)
        executable.symlink_to(outside)
        with self.assertRaises(ReleaseError): selected_tools(self.tools, TOOL_NAMES[:3])
        executable.unlink()
        os.link(outside, executable)
        with self.assertRaises(ReleaseError): selected_tools(self.tools, TOOL_NAMES[:3])
        executable.unlink()
        os.mkfifo(executable)
        with self.assertRaises(ReleaseError): selected_tools(self.tools, TOOL_NAMES[:3])

    def test_foreign_executable_or_ancestor_owner_is_refused(self):
        original = Path.lstat
        for target in (self.tools / "psql", self.tools, self.root):
            def foreign(path, *args, **kwargs):
                info = original(path, *args, **kwargs)
                if path == target:
                    fields = list(info); fields[4] = os.getuid() + 1
                    return os.stat_result(fields)
                return info
            with self.subTest(target=target), patch.object(Path, "lstat", foreign), self.assertRaises(ReleaseError):
                selected_tools(self.tools, TOOL_NAMES[:3])

    def test_root_owned_tools_and_protected_sticky_ancestor_are_allowed(self):
        original = Path.lstat
        def root_owned(path, *args, **kwargs):
            info = original(path, *args, **kwargs)
            if path in (self.root, self.tools, *(self.tools / name for name in TOOL_NAMES)):
                fields = list(info); fields[4] = 0
                if path == self.root:
                    fields[0] = (fields[0] & ~0o7777) | 0o1777
                return os.stat_result(fields)
            return info
        with patch.object(Path, "lstat", root_owned):
            self.assertEqual(selected_tools(self.tools, TOOL_NAMES[:3]),
                             {name: str(self.tools / name) for name in TOOL_NAMES[:3]})


if __name__ == "__main__":
    unittest.main(verbosity=2)
