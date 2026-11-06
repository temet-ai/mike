"""Explicit PostgreSQL tools; inherited credentials and child output stay private."""

from contextlib import contextmanager
import configparser
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib
import tempfile
from urllib.parse import parse_qs, unquote, urlsplit

from common import ReleaseError
from owned_process import OwnedProcess
from recovery_files import file_digest, plain_path
from recovery_tools import selected_tools

LEDGER_SQL = """SELECT coalesce(json_agg(json_build_object(
 'version',version,'checksum',encode(checksum,'hex'),'success',success)
 ORDER BY version),'[]'::json)::text FROM public._sqlx_migrations"""
# Not the WAL position: checkpoints and autovacuum write WAL on an idle database,
# so it falsely reported a change. Any user write consumes a transaction id and
# moves the snapshot.
CHANGE_SQL = """SELECT current_database() || '/' || session_user || '/' ||
 pg_current_snapshot()::text"""
EMPTY_SQL = """SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
 WHERE n.nspname NOT IN ('pg_catalog','information_schema')
 AND n.nspname NOT LIKE 'pg_toast%' AND n.nspname NOT LIKE 'pg_temp%'
 AND c.relkind IN ('r','p','v','m','S','f')"""


def execute(args, env, *, input_file=None, output_file=None, timeout=300):
    """Never echo commands, paths, SQL, connection values or child diagnostics."""
    process = None
    try:
        process = OwnedProcess(args, env=env, stdin=input_file or subprocess.DEVNULL,
                               stdout=output_file or subprocess.PIPE,
                               stderr=subprocess.DEVNULL)
        output, _ = process.communicate(timeout=timeout)
        if process.returncode:
            raise ReleaseError("database maintenance command failed; private diagnostics were suppressed")
        return output or b""
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ReleaseError("database maintenance command could not finish; no complete backup was published") from exc
    finally:
        if process is not None:
            process.close()


@contextmanager
def database_connection(*, service_file=None, service=None, docker_identity=None, tool_directory=None):
    # Freeze the explicitly selected service before target checks or restore.
    # A concurrent rewrite of the original file cannot redirect later commands.
    if service_file is not None and docker_identity is None:
        source = plain_path(service_file, private=True)
        with tempfile.TemporaryDirectory(prefix="mike-private-db-service-") as temporary:
            snapshot = Path(temporary) / "pg_service.conf"
            file_digest(source, snapshot)
            yield Database(service_file=snapshot, service=service, tool_directory=tool_directory)
    else:
        yield Database(service_file=service_file, service=service, docker_identity=docker_identity,
                       tool_directory=tool_directory)


class Database:
    def __init__(self, *, service_file=None, service=None, docker_identity=None, tool_directory=None):
        self.env = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C",
                    "PGCONNECT_TIMEOUT": "10", "PGOPTIONS": "-c statement_timeout=300000"}
        self.prefix = []
        if docker_identity is not None:
            if service_file is not None or service is not None or not re.fullmatch(r"[0-9a-f]{32}", docker_identity):
                raise ReleaseError("choose exactly one explicit database connection method")
            self.kind = "managed-docker"
            self.tools = selected_tools(tool_directory, ("docker",))
            # Honor the selected local Docker configuration, never a remote daemon.
            for name in ("HOME", "DOCKER_CONFIG", "DOCKER_CONTEXT", "DOCKER_HOST"):
                if name in os.environ:
                    self.env[name] = os.environ[name]
            self._docker(docker_identity)
            self.connection = ["--username=mike", "--dbname=mike", "--no-password"]
        else:
            if service_file is None or service is None or not re.fullmatch(r"[A-Za-z0-9_-]{1,80}", service):
                raise ReleaseError("BYO PostgreSQL requires a private service file and a simple service name")
            self.kind = "postgres-service"
            self.tools = selected_tools(tool_directory, ("psql", "pg_dump", "pg_restore"))
            self.service_file, self.service = service_file, service
            self.env.update(PGSERVICEFILE=str(plain_path(service_file, private=True)), PGSERVICE=service,
                            PGPASSFILE="/dev/null")
            # pg_restore requires an explicit database argument to connect;
            # PGSERVICE alone is sufficient for psql/pg_dump, but not restore.
            # The validated service name contains no credentials or endpoints.
            self.connection = [f"--dbname=service={service}", "--no-password"]

    def _docker(self, identity):
        docker = self.tools["docker"]
        host = self.env.get("DOCKER_HOST")
        if not host or self.env.get("DOCKER_CONTEXT"):
            value = execute([docker, "context", "inspect", "--format", "{{json .Endpoints.docker.Host}}"], self.env)
            host = json.loads(value)
        if not isinstance(host, str) or not host.startswith("unix://"):
            raise ReleaseError("managed recovery requires an explicitly local Docker Unix socket")
        self.env.pop("DOCKER_CONTEXT", None)
        self.env["DOCKER_HOST"] = host
        project, container, volume = f"mike-{identity}", f"mike-{identity}-database", f"mike-{identity}-data"
        records = json.loads(execute([docker, "container", "inspect", container], self.env))
        volumes = json.loads(execute([docker, "volume", "inspect", volume], self.env))
        if len(records) != 1 or len(volumes) != 1:
            raise ReleaseError("ambiguous managed database ownership")
        record = records[0]
        labels = record.get("Config", {}).get("Labels", {})
        for actual in (labels, volumes[0].get("Labels", {})):
            if (actual.get("io.temet.mike.installation") != identity or
                    actual.get("io.temet.mike.managed-database") != "1"):
                raise ReleaseError("unlabeled or foreign managed database; refusing maintenance")
        if labels.get("com.docker.compose.project") != project:
            raise ReleaseError("managed database belongs to another Compose project")
        mounts = record.get("Mounts", [])
        bindings = record.get("HostConfig", {}).get("PortBindings", {}).get("5432/tcp")
        if (not any(m.get("Name") == volume and m.get("Destination") == "/var/lib/postgresql/data" for m in mounts)
                or not bindings or any(p.get("HostIp") != "127.0.0.1" for p in bindings)):
            raise ReleaseError("managed database storage or loopback ownership is invalid")
        self.host_ports = {int(port["HostPort"]) for port in bindings}
        container_id = record.get("Id", "")
        if not re.fullmatch(r"[0-9a-f]{64}", container_id):
            raise ReleaseError("Docker did not identify an immutable selected container")
        self.prefix = [docker, "exec", "-i", container_id]

    def command(self, name):
        return self.prefix + [name] if self.prefix else [self.tools[name]]

    def assert_source(self, config):
        with config.open("rb") as stream:
            url = urlsplit(tomllib.load(stream)["database_url"])
        if (url.scheme not in ("postgres", "postgresql") or not url.hostname or not url.username
                or any(k in parse_qs(url.query) for k in ("host", "hostaddr", "port", "dbname", "user", "service"))):
            raise ReleaseError("source database URL needs an unambiguous host, user and database")
        expected = (url.hostname.lower(), url.port or 5432, unquote(url.path.lstrip("/")), unquote(url.username))
        if self.kind == "managed-docker":
            if expected[0] not in ("127.0.0.1", "localhost") or expected[1] not in self.host_ports or expected[2:] != ("mike", "mike"):
                raise ReleaseError("managed database does not match the selected boot configuration")
        else:
            values = configparser.ConfigParser(interpolation=None)
            with plain_path(self.service_file, private=True).open() as stream:
                values.read_file(stream)
            if self.service not in values:
                raise ReleaseError("selected PostgreSQL service is missing")
            value = values[self.service]
            actual = (value.get("host", "").lower(), int(value.get("port", "5432")), value.get("dbname"), value.get("user"))
            if actual != expected or any(key in value for key in ("hostaddr", "service")):
                raise ReleaseError("PostgreSQL service target differs from the selected boot configuration")
        actual = json.loads(self.query("SELECT json_build_array(current_database(), session_user)::text"))
        if actual != list(expected[2:]):
            raise ReleaseError("connected database/role differs from the selected boot configuration")

    def query(self, sql):
        args = self.command("psql") + [*self.connection, "--no-psqlrc", "--tuples-only",
                              "--no-align", "--quiet", "--set=ON_ERROR_STOP=1", "--command",
                              "BEGIN READ ONLY; " + sql + "; COMMIT"]
        return execute(args, self.env).decode().strip()

    def facts(self):
        major = int(self.query("SHOW server_version_num")) // 10000
        if major != 19:
            raise ReleaseError("recovery currently requires PostgreSQL 19 on both source and target")
        ledger = json.loads(self.query(LEDGER_SQL))
        if not isinstance(ledger, list) or not ledger:
            raise ReleaseError("backup requires a populated successful migration ledger")
        versions = []
        for row in ledger:
            if (set(row) != {"version", "checksum", "success"} or row["success"] is not True
                    or type(row["version"]) is not int or row["version"] <= 0
                    or not isinstance(row["checksum"], str) or not re.fullmatch(r"[0-9a-f]{96}", row["checksum"])):
                raise ReleaseError("invalid or failed migration ledger; backup refused")
            versions.append(row["version"])
        if sorted(set(versions)) != versions:
            raise ReleaseError("migration ledger is not strictly ordered")
        return {"postgres_major": major, "migration_ledger": ledger,
                "database_change_token": hashlib.sha256(self.query(CHANGE_SQL).encode()).hexdigest()}

    def dump(self, destination):
        descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            execute(self.command("pg_dump") + [*self.connection, "--format=custom", "--no-owner", "--no-acl"],
                    self.env, output_file=stream)
            stream.flush()
            os.fsync(stream.fileno())
        with destination.open("rb") as stream:
            if stream.read(5) != b"PGDMP":
                raise ReleaseError("database dump is not a PostgreSQL custom archive")
        with destination.open("rb") as stream:
            execute(self.command("pg_restore") + ["--list"], self.env, input_file=stream)

    def require_fresh(self):
        if int(self.query("SHOW server_version_num")) // 10000 != 19 or self.query(EMPTY_SQL) != "0":
            raise ReleaseError("restore requires an empty PostgreSQL 19 database; existing objects are retained")

    def restore(self, source):
        self.require_fresh()
        with source.open("rb") as stream:
            execute(self.command("pg_restore") + [*self.connection, "--single-transaction", "--exit-on-error",
                                   "--no-owner", "--no-acl"], self.env, input_file=stream)
