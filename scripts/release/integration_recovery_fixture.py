"""Owned loopback PostgreSQL 19 fixture for explicit recovery integration tests.

No test starts Mike or uses repository database configuration. The caller must
supply both a disposable admin URL and its owned PGDATA directory. Test-only
client relocation is explicit and never changes production tool discovery.
"""

from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
from urllib.parse import parse_qs, urlsplit, urlunsplit
import uuid

from recovery_database import database_connection

LEDGER = [{"version": 20260604000001, "checksum": "a" * 96, "success": True},
          {"version": 20260604000002, "checksum": "b" * 96, "success": True}]
SEED_SQL = """
CREATE TABLE public._sqlx_migrations (
 version bigint PRIMARY KEY, description text NOT NULL,
 installed_on timestamptz NOT NULL DEFAULT now(), success boolean NOT NULL,
 checksum bytea NOT NULL, execution_time bigint NOT NULL);
INSERT INTO public._sqlx_migrations(version,description,success,checksum,execution_time)
 VALUES (20260604000001,'synthetic first',true,decode(repeat('a',96),'hex'),1),
        (20260604000002,'synthetic second',true,decode(repeat('b',96),'hex'),2);
CREATE SCHEMA capture;
CREATE TABLE capture.sessions(id uuid PRIMARY KEY, title text NOT NULL);
CREATE TABLE capture.requests(id uuid PRIMARY KEY,
 session_id uuid REFERENCES capture.sessions(id), body bytea NOT NULL, status integer NOT NULL);
INSERT INTO capture.sessions VALUES
 ('00000000-0000-0000-0000-000000000001','synthetic unicode café');
INSERT INTO capture.requests VALUES
 ('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000001',
  decode('0001feff00426f6479','hex'),200),
 ('00000000-0000-0000-0000-000000000012','00000000-0000-0000-0000-000000000001',
  decode('f09f8c8d0a','hex'),429);
CREATE TABLE public.settings(key text PRIMARY KEY, value jsonb NOT NULL);
INSERT INTO public.settings VALUES ('capture','{"enabled":true,"labels":["fixture","café"]}');
CREATE SEQUENCE public.fixture_sequence START 23;
SELECT nextval('public.fixture_sequence');
CREATE VIEW capture.request_counts AS SELECT status,count(*) AS requests
 FROM capture.requests GROUP BY status;
"""
SNAPSHOT_SQL = """SELECT json_build_object(
 'sessions',(SELECT json_agg(s ORDER BY id) FROM capture.sessions s),
 'requests',(SELECT json_agg(r ORDER BY id) FROM
   (SELECT id,session_id,encode(body,'hex') AS body,status FROM capture.requests) r),
 'settings',(SELECT json_agg(s ORDER BY key) FROM public.settings s),
 'ledger',(SELECT json_agg(l ORDER BY version) FROM
   (SELECT version,description,installed_on,success,encode(checksum,'hex') AS checksum,
           execution_time FROM public._sqlx_migrations) l),
 'sequence',(SELECT json_build_array(last_value,is_called) FROM public.fixture_sequence),
 'counts',(SELECT json_agg(c ORDER BY status) FROM capture.request_counts c))::text"""


def private_directory(path):
    path = Path(path).absolute()
    info = path.lstat()
    if (path.is_symlink() or not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
            or info.st_mode & 0o077 or path != path.resolve()):
        raise RuntimeError("recovery DB fixture requires an owned private non-symlink PGDATA")
    return path


class Cluster:
    def __init__(self):
        value = os.environ.get("MIKE_TEST_ADMIN_URL", "").strip()
        data = os.environ.get("MIKE_RECOVERY_TEST_PGDATA", "").strip()
        if not value or not data:
            raise RuntimeError("MIKE_TEST_ADMIN_URL and MIKE_RECOVERY_TEST_PGDATA are required; no tests skipped")
        self.url = urlsplit(value)
        if (self.url.scheme not in ("postgres", "postgresql") or self.url.hostname != "127.0.0.1"
                or self.url.username != "postgres" or self.url.password is not None
                or self.url.path != "/postgres" or self.url.fragment
                or not self.url.port or self.url.port < 1024 or self.url.port in (5432, 5440)
                or parse_qs(self.url.query) != {"sslmode": ["disable"]}):
            raise RuntimeError("recovery fixture requires a password-free loopback disposable admin URL, never capture/default ports")
        self.data = private_directory(data)
        if (self.data / "PG_VERSION").read_text().strip() != "19":
            raise RuntimeError("recovery fixture PGDATA must be PostgreSQL 19")
        if not (self.data / "postmaster.pid").is_file():
            raise RuntimeError("recovery fixture PGDATA must identify the running disposable cluster")
        self.environment = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C",
                            "PGCONNECT_TIMEOUT": "5", "PGPASSFILE": "/dev/null"}
        tool_dir = os.environ.get("MIKE_RECOVERY_TEST_TOOL_DIR")
        library_path = os.environ.get("MIKE_RECOVERY_TEST_LIBRARY_PATH")
        if tool_dir:
            directory = Path(tool_dir)
            if not directory.is_absolute() or not directory.is_dir():
                raise RuntimeError("explicit test-only PostgreSQL tool directory must be absolute")
            self.environment["PATH"] = str(directory) + ":/usr/bin:/bin"
        if library_path:
            if any(not Path(part).is_absolute() or not Path(part).is_dir()
                   for part in library_path.split(":")):
                raise RuntimeError("explicit test-only PostgreSQL library paths must be absolute directories")
            self.environment["LD_LIBRARY_PATH"] = library_path
        self.client_versions = {}
        for name in ("psql", "pg_dump", "pg_restore"):
            if not shutil.which(name, path=self.environment["PATH"]):
                raise RuntimeError("PostgreSQL 19 client tools required; no tests skipped")
            self.client_versions[name] = self.command([name, "--version"])
            if not re.search(r"\(PostgreSQL\) 19(?:\.[0-9]+|(?:beta|rc)[0-9]+)(?:\s|$)", self.client_versions[name]):
                raise RuntimeError("recovery fixture requires PostgreSQL 19 client tools")
        actual = json.loads(self.sql("postgres", "SELECT json_build_array("
            "current_setting('server_version_num'), current_setting('data_directory'),"
            "current_setting('listen_addresses'), current_setting('unix_socket_directories'),"
            "current_database(), session_user)::text"))
        if (int(actual[0]) // 10000 != 19 or Path(actual[1]) != self.data
                or actual[2:] != ["127.0.0.1", "", "postgres", "postgres"]):
            raise RuntimeError("connected server does not match the owned TCP-only disposable PGDATA")
        self.server_version = actual[0]
        self.prefix = "recovery_fixture_" + uuid.uuid4().hex[:16] + "_"
        self.created = []

    def command(self, args, *, input=None):
        result = subprocess.run(args, env=self.environment, input=input,
                                capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise RuntimeError("disposable PostgreSQL fixture command failed; private diagnostics suppressed")
        return result.stdout.strip()

    def sql(self, database, statement):
        if database != "postgres" and database not in self.created:
            raise RuntimeError("fixture SQL refused a database not created by this run")
        url = urlunsplit(self.url._replace(path="/" + database))
        return self.command(["psql", "--dbname=" + url, "--no-password", "--no-psqlrc",
                             "--quiet", "--tuples-only", "--no-align", "--set=ON_ERROR_STOP=1"],
                            input=statement)

    def create(self, *, seeded=False):
        name = self.prefix + str(len(self.created))
        self.sql("postgres", 'CREATE DATABASE "' + name + '" TEMPLATE template0;')
        self.created.append(name)
        if seeded:
            self.sql(name, SEED_SQL)
        return name

    def cleanup(self):
        for name in reversed(self.created):
            if not re.fullmatch(re.escape(self.prefix) + r"[0-9]+", name):
                raise RuntimeError("refusing to drop an unowned fixture database")
            self.sql("postgres", 'DROP DATABASE "' + name + '";')
        self.created.clear()

    def service(self, path, database):
        if database not in self.created:
            raise RuntimeError("service refused a database not created by this run")
        path.write_text("[fixture]\nhost=127.0.0.1\nport=" + str(self.url.port)
                        + "\nuser=postgres\ndbname=" + database + "\nsslmode=disable\n")
        path.chmod(0o600)
        return path

    def config_url(self, database):
        if database not in self.created:
            raise RuntimeError("config refused a database not created by this run")
        return urlunsplit(self.url._replace(path="/" + database))

    @contextmanager
    def connection(self, service):
        with database_connection(service_file=service, service="fixture") as database:
            # These test-only client/library environment overrides remain
            # separate from the installed helper's --tool-directory selection.
            database.env.update(self.environment)
            yield database

    def snapshot(self, database):
        return json.loads(self.sql(database, SNAPSHOT_SQL))
