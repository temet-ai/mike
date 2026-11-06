"""Coordinated private recovery bundles; explicit targets, no purge or upload."""

import argparse
import configparser
import ctypes
import errno
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile
import tomllib

from common import ReleaseError
from inputs import interruption_cleanup, verified_inputs
from install import check_upgrade, install
from install_state import (STATE_NAME, check_directory, check_links, check_owner, current_generation,
                           installation_lock, safe_prefix, sync_directory)
from recovery_database import database_connection, execute
from recovery_files import (copy_inventory, file_digest, inventory, new_destination, plain_path,
                            private_json, read_json, stopped_installation, validate_index, validated_roots)

MANIFEST_FIELDS = {"schema_version", "state", "created_utc", "installation_id", "version",
                   "source_commit", "installed_manifest_sha256", "database_method", "postgres_major",
                   "files", "bytes", "database_dump", "private_index", "migration_ledger", "database_change_token"}


def installed(prefix):
    prefix = safe_prefix(prefix)
    # Validate every intermediate directory before following command/current
    # links or opening the installation lock. An ownership marker beneath a
    # symlinked lib/generations directory does not establish prefix ownership.
    for directory in (prefix, prefix / "bin", prefix / "lib"):
        check_directory(directory, owned=True)
    state = prefix / "lib" / STATE_NAME
    check_owner(state)
    check_directory(state / "generations", owned=True)
    check_links(prefix, True)
    identity, manifest = current_generation(state)
    if identity is None:
        raise ReleaseError("no active owned archive installation; select its matching installation first")
    return prefix, state, identity, manifest



@contextmanager
def active_installation(prefix):
    _, state, _, _ = installed(prefix)
    with installation_lock(state):
        yield installed(prefix)


def runtime_command(prefix, config, arguments):
    environment = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C"}
    for key in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"):
        if key in os.environ:
            environment[key] = os.environ[key]
    binary = prefix / "lib" / STATE_NAME / "current" / "mike"
    return execute([str(binary), "--config", str(config), *arguments], environment, timeout=15)


def runtime_paths(prefix, config):
    result = runtime_command(prefix, config, ["print-config", "--paths"])
    if len(result) > 65536:
        raise ReleaseError("installed binary returned oversized path metadata")
    return json.loads(result)


def require_runtime_paths(prefix, config, state, data=None):
    config = plain_path(config, private=True)
    paths = runtime_paths(prefix, config)
    if (not isinstance(paths, dict) or set(paths) != {
            "config_file", "installation_id", "data_dir", "state_dir", "log_dir"} or
            paths["installation_id"] != installation_id(config) or paths["config_file"] != str(config) or
            paths["state_dir"] != str(Path(os.path.abspath(state))) or
            (data is not None and paths["data_dir"] != str(Path(os.path.abspath(data))))):
        raise ReleaseError("selected roots differ from the installed binary's authoritative paths")


def selected_roots(config, data, state):
    roots = validated_roots({"config": config, "data": data, "state": state})
    with roots["config"].open("rb") as stream:
        config_value = tomllib.load(stream)
    if not config_value.get("persist_enabled", False) or not config_value.get("database_url"):
        raise ReleaseError("backup requires explicit durable persistence in the selected boot config")
    configured = config_value.get("data_dir")
    if configured is not None:
        value = Path(configured)
        if not value.is_absolute():
            value = roots["config"].parent / value
        if Path(os.path.abspath(value)) != roots["data"]:
            raise ReleaseError("selected data root differs from the boot config; resolve all overrides first")
    return roots


def installation_id(config):
    return hashlib.sha256(os.fsencode(config)).hexdigest()[:32]


def publish_directory_noreplace(source, destination):
    """Publish atomically; never emulate exclusivity with check-then-rename."""
    try:
        library = ctypes.CDLL(None, use_errno=True)
        if sys.platform.startswith("linux"):
            rename = library.renameat2
            rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int,
                               ctypes.c_char_p, ctypes.c_uint]
            arguments = (-100, os.fsencode(source), -100, os.fsencode(destination), 1)
            # Linux AT_FDCWD=-100; RENAME_NOREPLACE=1. Paths are absolute.
        elif sys.platform == "darwin":
            rename = library.renamex_np
            rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
            arguments = (os.fsencode(source), os.fsencode(destination), 0x00000004)
            # Darwin's public sys/stdio.h declares RENAME_EXCL=0x00000004.
        else:
            raise ReleaseError("atomic no-replace directory publication is unsupported on this platform")
        rename.restype = ctypes.c_int
    except (AttributeError, OSError) as exc:
        raise ReleaseError("atomic no-replace directory publication is unavailable; no fallback rename is permitted") from exc
    if rename(*arguments) != 0:
        error = ctypes.get_errno()
        if error in (errno.EEXIST, errno.ENOTEMPTY):
            raise ReleaseError("destination appeared during maintenance; existing destination was preserved")
        if error in (errno.ENOSYS, errno.EINVAL, errno.ENOTSUP, errno.EOPNOTSUPP):
            raise ReleaseError("filesystem lacks atomic no-replace publication; no fallback rename is permitted")
        raise ReleaseError(f"atomic no-replace directory publication failed (errno {error}); inspect the private parent")


def same_directory(path, identity):
    try:
        current = path.lstat()
    except FileNotFoundError:
        return False
    return stat.S_ISDIR(current.st_mode) and (current.st_dev, current.st_ino) == identity


@contextmanager
def staging(destination, roots=()):
    destination = new_destination(destination)
    if any(destination == root or root in destination.parents or destination in root.parents for root in roots):
        raise ReleaseError("backup/output must be outside every selected input root")
    temporary = Path(tempfile.mkdtemp(prefix=".mike-recovery-incomplete-", dir=destination.parent))
    info = temporary.lstat()
    identity = (info.st_dev, info.st_ino)
    try:
        yield temporary
        sync_directory(temporary)
        publish_directory_noreplace(temporary, destination)
        try:
            sync_directory(destination.parent)
        except OSError:
            # A failed final durability step is not a completed backup. Move
            # our just-published output back to its private incomplete name.
            if same_directory(destination, identity):
                publish_directory_noreplace(destination, temporary)
            raise
    finally:
        # The temporary name is free after publication. Never clean an
        # unrelated directory created at that name by another actor.
        if same_directory(temporary, identity):
            shutil.rmtree(temporary)


def backup(prefix, roots, database, destination, *, exclusive=False):
    if not exclusive:
        raise ReleaseError("confirm that all external writers and capture clients are stopped first")
    with active_installation(prefix) as (prefix, _, identity, version):
        require_runtime_paths(prefix, roots["config"], roots["state"], roots["data"])
        with stopped_installation(roots["state"]), staging(destination, roots.values()) as stage:
            runtime_command(prefix, roots["config"], ["database", "check"])
            return backup_snapshot(roots, database, stage, identity, version)


def backup_snapshot(roots, database, stage, identity, version):
    database.assert_source(roots["config"])
    files = inventory(roots)
    facts = database.facts()
    copy_inventory(roots, files, stage / "files")
    database.dump(stage / "database.dump")
    if facts != database.facts() or files != inventory(roots):
        raise ReleaseError("state changed during backup; no completed backup was published")
    private_json(stage / "restore-index.json", {"files": files, "migration_ledger": facts["migration_ledger"]})
    manifest = {"schema_version": 1, "state": "complete", "created_utc": datetime.now(timezone.utc).isoformat(),
                "installation_id": installation_id(roots["config"]), "version": version["version"],
                "source_commit": version["source_commit"], "installed_manifest_sha256": identity,
                "database_method": database.kind, "postgres_major": facts["postgres_major"],
                "database_change_token": facts["database_change_token"],
                "files": sum(item["kind"] == "file" for item in files),
                "bytes": sum(item.get("size", 0) for item in files),
                "database_dump": file_digest(stage / "database.dump"),
                "private_index": file_digest(stage / "restore-index.json"),
                "migration_ledger": hashlib.sha256(json.dumps(facts["migration_ledger"], sort_keys=True).encode()).hexdigest()}
    private_json(stage / "manifest.json", manifest)
    verify(stage)
    return receipt(manifest, "backup-complete")


def verify(bundle):
    bundle = plain_path(bundle, directory=True, private=True)
    manifest = read_json(bundle / "manifest.json", 16384)
    if (not isinstance(manifest, dict) or set(manifest) != MANIFEST_FIELDS or
            type(manifest["schema_version"]) is not int or manifest["schema_version"] != 1 or manifest["state"] != "complete" or
            type(manifest["postgres_major"]) is not int or manifest["postgres_major"] != 19 or manifest["database_method"] not in ("managed-docker", "postgres-service")):
        raise ReleaseError("unsupported or incomplete recovery manifest")
    if any(type(manifest[key]) is not int or manifest[key] < 0 for key in ("files", "bytes")):
        raise ReleaseError("invalid recovery inventory totals")
    for key, length in (("installation_id", 32), ("installed_manifest_sha256", 64),
                        ("source_commit", 40), ("migration_ledger", 64), ("database_change_token", 64)):
        if not isinstance(manifest[key], str) or not re.fullmatch(r"[0-9a-f]{" + str(length) + "}", manifest[key]):
            raise ReleaseError("invalid recovery identity")
    from install import semver
    semver(manifest["version"])
    created = datetime.fromisoformat(manifest["created_utc"])
    if created.tzinfo is None or (datetime.now(timezone.utc) - created).total_seconds() < -60:
        raise ReleaseError("invalid recovery timestamp")
    if {p.name for p in bundle.iterdir()} != {"manifest.json", "restore-index.json", "files", "database.dump"}:
        raise ReleaseError("unexpected recovery bundle contents")
    for name, key in (("database.dump", "database_dump"), ("restore-index.json", "private_index")):
        plain_path(bundle / name, private=True)
        if file_digest(bundle / name) != manifest[key]:
            raise ReleaseError("recovery bundle checksum mismatch")
    with (bundle / "database.dump").open("rb") as stream:
        if stream.read(5) != b"PGDMP":
            raise ReleaseError("recovery database is not a custom PostgreSQL archive")
    index = read_json(bundle / "restore-index.json")
    if not isinstance(index, dict) or set(index) != {"files", "migration_ledger"}:
        raise ReleaseError("unexpected recovery index contents")
    if hashlib.sha256(json.dumps(index["migration_ledger"], sort_keys=True).encode()).hexdigest() != manifest["migration_ledger"]:
        raise ReleaseError("migration ledger checksum mismatch")
    expected = validate_index(index["files"])
    plain_path(bundle / "files", directory=True, private=True)
    if {p.name for p in (bundle / "files").iterdir()} != expected:
        raise ReleaseError("recovery payload set differs from its index")
    for item in index["files"]:
        if item["kind"] == "file":
            path = bundle / "files" / item["payload"]
            plain_path(path, private=True)
            if file_digest(path) != {key: item[key] for key in ("size", "sha256")}:
                raise ReleaseError("recovery payload checksum mismatch")
    if manifest["files"] != len(expected) or manifest["bytes"] != sum(i.get("size", 0) for i in index["files"]):
        raise ReleaseError("recovery inventory totals mismatch")
    return manifest, index


def receipt(manifest, status):
    # A small allowlist is intentionally distinct from the private restore index.
    return {key: manifest[key] for key in ("created_utc", "version", "source_commit", "postgres_major", "files", "bytes")} | {
        "status": status, "qualified": False}


def require_matching_binary(prefix, manifest):
    _, state, identity, version = installed(prefix)
    if identity != manifest["installed_manifest_sha256"] or version["version"] != manifest["version"]:
        raise ReleaseError("recovery requires the exact backed-up binary pair; use a fresh prefix with the matching candidate")
    return state


def restore(bundle, prefix, database, destination, *, exclusive=False):
    if not exclusive:
        raise ReleaseError("confirm exclusive access to the fresh target database before restoring")
    manifest, index = verify(bundle)
    with active_installation(prefix):
        require_matching_binary(prefix, manifest)
        return restore_snapshot(bundle, manifest, index, database, destination)


def restore_snapshot(bundle, manifest, index, database, destination):
    with staging(destination, (Path(os.path.abspath(bundle)),)) as stage:
        for scope in ("data", "state"):
            (stage / scope).mkdir(mode=0o700)
        for item in index["files"]:
            target = stage / ("config.toml" if item["scope"] == "config" else item["scope"] + "/" + item["path"])
            if item["kind"] == "directory":
                target.mkdir(mode=0o700)
            else:
                copied = file_digest(bundle / "files" / item["payload"], target)
                if copied != {key: item[key] for key in ("sha256", "size")}:
                    raise ReleaseError("backup changed during recovery; target database was not changed")
        # The database dump is copied into the private stage and verified again
        # before pg_restore: input paths may otherwise change after validation.
        if file_digest(bundle / "database.dump", stage / "database.dump") != manifest["database_dump"]:
            raise ReleaseError("database backup changed before restore")
        database.restore(stage / "database.dump")
        observed = database.facts()
        if {key: observed[key] for key in ("postgres_major", "migration_ledger")} != {
                "postgres_major": manifest["postgres_major"], "migration_ledger": index["migration_ledger"]}:
            raise ReleaseError("restored ledger differs; keep target offline and inspect the disposable database")
        (stage / "database.dump").unlink()
        private_json(stage / "RESTORED.json", receipt(manifest, "restored-offline-review-required"))
        for path in reversed(sorted(stage.rglob("*"))):
            if path.is_dir():
                sync_directory(path)
    return receipt(manifest, "restored-offline-review-required")


def upgrade_check(bundle, prefix, roots, database, candidate, *, exclusive=False):
    if not exclusive:
        raise ReleaseError("confirm that all external writers are stopped before checking upgrade readiness")
    manifest, index = verify(bundle)
    if installation_id(roots["config"]) != manifest["installation_id"]:
        raise ReleaseError("backup belongs to a different selected configuration")
    with active_installation(prefix) as (prefix, state, _, _):
        require_matching_binary(prefix, manifest)
        require_runtime_paths(prefix, roots["config"], roots["state"], roots["data"])
        with stopped_installation(roots["state"]):
            return check_upgrade_snapshot(manifest, index, roots, database, candidate, state)


def check_upgrade_snapshot(manifest, index, roots, database, candidate, state):
    database.assert_source(roots["config"])
    if inventory(roots) != index["files"] or database.facts() != {
            "postgres_major": manifest["postgres_major"], "migration_ledger": index["migration_ledger"],
            "database_change_token": manifest["database_change_token"]}:
        raise ReleaseError("backup no longer matches current state; make a new coordinated backup")
    with verified_inputs(candidate) as (_, proposed):
        _, previous = current_generation(state)
        check_upgrade(previous, proposed, False)
    return receipt(manifest, "backup-matches-pre-upgrade-state") | {
        "candidate_version": proposed["version"], "schema_compatibility": "NOT_RUN",
        "next_step": "Run the candidate's read-only database check, then install while all writers remain stopped."}


def uninstall(prefix, state_root, config, *, exclusive=False):
    if not exclusive:
        raise ReleaseError("stop every installation using this prefix and confirm external writers are stopped")
    with active_installation(prefix) as (prefix, state, _, _):
        require_runtime_paths(prefix, config, state_root)
        with stopped_installation(state_root):
            return unlink_commands(prefix, state)


def unlink_commands(prefix, state):
    # Validate everything before touching either entry; a changed link means
    # ownership was lost. Keep immutable generations for recovery/reinstall.
    check_links(prefix, True)
    current_generation(state)
    for name in ("mike", "mike-mcp"):
        link = prefix / "bin" / name
        if link.is_symlink():
            link.unlink()
    sync_directory(prefix / "bin")
    return {"status": "owned-commands-unlinked", "data_retained": True, "configuration_retained": True,
            "database_resources_retained": True, "backups_retained": True,
            "recovery_generations_retained": True, "integrations_changed": False}


def add_database(parser):
    parser.add_argument("--pg-service-file", type=Path)
    parser.add_argument("--pg-service")
    parser.add_argument("--docker-identity", help="exact selected configuration's 32-character installation ID")
    parser.add_argument("--tool-directory", type=Path,
                        help="trusted absolute physical directory with psql/pg_dump/pg_restore or docker; default /usr/bin:/bin")


def add_sources(parser):
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--data-root", required=True, type=Path)
    parser.add_argument("--state-root", required=True, type=Path)


def main():
    parser = argparse.ArgumentParser(prog="mike-maintenance", description=__doc__, epilog=
        "Optional maintenance requires Bash and Python 3.11+; mike and mike-mcp do not. "
        "Database operations also need PostgreSQL client tools or the selected local Docker container. "
        "Backups and private service files can contain secrets. No upload, purge or service start is performed.")
    commands = parser.add_subparsers(dest="command", required=True)
    installer = commands.add_parser("install", help="verify and install an explicitly trusted local archive candidate")
    installer.add_argument("--candidate", required=True, type=Path,
                           help="directory containing manifest.json and both archive/checksum pairs; no download")
    installer.add_argument("--prefix", required=True, type=Path,
                           help="explicit user-owned install prefix; no service start or data migration")
    for name in ("backup", "restore", "upgrade-check", "uninstall", "verify"):
        sub = commands.add_parser(name)
        if name != "verify":
            sub.add_argument("--prefix", required=True, type=Path)
            sub.add_argument("--confirm-stopped-writers", action="store_true",
                             help="all capture clients, other prefixes/instances and external DB/file writers are stopped")
        if name in ("backup", "upgrade-check"):
            add_sources(sub)
        if name in ("backup", "restore", "upgrade-check"):
            add_database(sub)
        if name in ("backup", "restore"):
            sub.add_argument("--destination", required=True, type=Path)
        if name in ("verify", "restore", "upgrade-check"):
            sub.add_argument("--backup", required=True, type=Path)
        if name == "upgrade-check":
            sub.add_argument("--candidate", required=True, type=Path)
        if name == "uninstall":
            sub.add_argument("--state-root", required=True, type=Path)
            sub.add_argument("--config", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "install":
        result = install(args.candidate, args.prefix)
    elif args.command == "verify":
        result = receipt(verify(args.backup)[0], "verified-checksums")
    elif args.command == "uninstall":
        result = uninstall(args.prefix, args.state_root, args.config, exclusive=args.confirm_stopped_writers)
    else:
        roots = selected_roots(args.config, args.data_root, args.state_root) if args.command in ("backup", "upgrade-check") else None
        if roots and args.docker_identity and installation_id(roots["config"]) != args.docker_identity:
            raise ReleaseError("Docker identity differs from the selected configuration")
        with database_connection(service_file=args.pg_service_file, service=args.pg_service,
                                 docker_identity=args.docker_identity, tool_directory=args.tool_directory) as database:
            if args.command == "backup":
                result = backup(args.prefix, roots, database, args.destination, exclusive=args.confirm_stopped_writers)
            elif args.command == "restore":
                result = restore(args.backup, args.prefix, database, args.destination, exclusive=args.confirm_stopped_writers)
            else:
                result = upgrade_check(args.backup, args.prefix, roots, database, args.candidate, exclusive=args.confirm_stopped_writers)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    try:
        with interruption_cleanup():
            main()
    except ReleaseError as exc:
        print(f"release-maintenance: {exc}", file=sys.stderr)
        sys.exit(1)
    except (OSError, ValueError, KeyError, TypeError, AttributeError, configparser.Error):
        print("release-maintenance: unsafe, missing or invalid maintenance input; private values omitted", file=sys.stderr)
        sys.exit(1)
