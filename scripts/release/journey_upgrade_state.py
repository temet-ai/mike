"""Owned two-archive upgrade boundaries; no database or process work at import."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys

from common import ReleaseError, sha256
from install import install_verified, semver
from journey import verify_binding
from journey_persistent import checked_migration_inventory, verified_migration_inventory
from journey_process import JourneyError, require, runtime_environment
from owned_process import OwnedProcess
from recovery import installed
from recovery_database import execute
from recovery_files import file_digest, inventory


def compatible_pair(previous, candidate):
    require(previous["target"] == candidate["target"], "UPGRADE_TARGET_MISMATCH")
    require(semver(previous["version"]) < semver(candidate["version"]), "UPGRADE_PREDECESSOR_NOT_OLDER")
    require(previous["source_commit"] != candidate["source_commit"], "UPGRADE_SAME_SOURCE_COMMIT")
    old = checked_migration_inventory(previous.get("migration_inventory"), previous["source_commit"])
    new = checked_migration_inventory(candidate.get("migration_inventory"), candidate["source_commit"])
    count = len(old["migrations"])
    require(len(new["migrations"]) > count and new["migrations"][:count] == old["migrations"],
            "UPGRADE_REQUIRES_STRICT_MIGRATION_EXTENSION")
    return {"strictly_newer_version": True, "distinct_source_commits": True,
            "same_target": True, "unchanged_predecessor_migration_prefix": True,
            "predecessor_migrations": count, "candidate_migrations": len(new["migrations"]),
            "incompatible_predecessor_schema_is_real": True}


class Archive:
    def __init__(self, snapshot, manifest, identity):
        self.snapshot, self.manifest, self.identity = Path(snapshot), manifest, identity
        self.snapshot_binding()
        self.paths = self.binding = self.inventory = self.prefix = None

    def snapshot_binding(self):
        require(sha256(self.snapshot / "manifest.json") == self.identity
                and json.loads((self.snapshot / "manifest.json").read_bytes()) == self.manifest,
                "UPGRADE_SNAPSHOT_CHANGED")
        for artifact in self.manifest["artifacts"]:
            require(Path(artifact["archive"]).name == artifact["archive"]
                    and sha256(self.snapshot / artifact["archive"]) == artifact["sha256"],
                    "UPGRADE_SNAPSHOT_ARCHIVE_CHANGED")

    def select(self, prefix):
        self.snapshot_binding()
        receipt = install_verified(self.snapshot, self.manifest, prefix)
        self.bind(prefix)
        return {"status": receipt["status"], "manifest_sha256": self.identity,
                "exact_binary_pair": True, "qualified": False}

    def bind(self, prefix):
        self.prefix = Path(prefix)
        self.paths, self.binding = verify_binding(self.prefix / "bin", self.manifest, self.identity)
        self.inventory = verified_migration_inventory(self.paths, self.manifest)
        self.recheck()

    def recheck(self, *, active=True):
        require(self.paths is not None, "UPGRADE_ARCHIVE_NOT_BOUND")
        if active:
            prefix, state, identity, manifest = installed(self.prefix)
            require(prefix == self.prefix and identity == self.identity and manifest == self.manifest,
                    "UPGRADE_ACTIVE_GENERATION_CHANGED")
            require(all(path.parent == state / "generations" / identity for path in self.paths.values()),
                    "UPGRADE_GENERATION_MISMATCH")
        paths, binding = verify_binding(self.paths["mike"].parent, self.manifest, self.identity)
        require(paths == self.paths and binding == self.binding
                and verified_migration_inventory(paths, self.manifest) == self.inventory,
                "UPGRADE_RETAINED_ARCHIVE_CHANGED")
        return {"exact_archive_and_source_identity": True, "active_generation": active}


def tree_digest(root):
    """Hash private install structure including links/modes; never follow links."""
    root = Path(root)
    rows = []
    def visit(path):
        info = path.lstat()
        require(info.st_uid == os.getuid(), "UPGRADE_TREE_NOT_OWNED")
        row = {"path": path.relative_to(root).as_posix(), "mode": stat.S_IMODE(info.st_mode)}
        if stat.S_ISLNK(info.st_mode):
            row.update(kind="symlink", target=os.readlink(path))
        elif stat.S_ISREG(info.st_mode):
            row.update(kind="file", **file_digest(path))
        else:
            require(stat.S_ISDIR(info.st_mode), "UPGRADE_TREE_SPECIAL_FILE")
            row["kind"] = "directory"
        rows.append(row)
        require(len(rows) <= 20000, "UPGRADE_TREE_ENTRY_LIMIT")
        if row["kind"] == "directory":
            for item in sorted(path.iterdir()):
                visit(item)
    visit(root)
    return hashlib.sha256(json.dumps(rows, sort_keys=True).encode()).hexdigest()


def database_digest(cluster, database, service):
    require(database in cluster.created, "UPGRADE_DATABASE_NOT_OWNED")
    with cluster.connection(service) as connection:
        raw = execute(connection.command("pg_dump") + [*connection.connection,
                      "--format=plain", "--create"], connection.env, timeout=60)
    require(raw.startswith(b"--\n-- PostgreSQL database dump\n"), "UPGRADE_DUMP_FORMAT")
    # PostgreSQL clients randomize only their psql restrict token. Preserve every
    # schema, data, sequence, comment and setting byte, including COPY payloads.
    lines = raw.splitlines(keepends=True)
    tokens = [line for line in lines if re.fullmatch(rb"\\restrict [A-Za-z0-9]+\n", line)]
    if tokens:
        require(len(tokens) == 1, "UPGRADE_DUMP_RESTRICT_TOKEN")
        token = tokens[0].split()[1]
        closing = b"\\unrestrict " + token + b"\n"
        require(lines.count(closing) == 1, "UPGRADE_DUMP_RESTRICT_TOKEN")
        raw = b"".join(line for line in lines if line not in (tokens[0], closing))
    return hashlib.sha256(raw).hexdigest()


def durable_snapshot(cluster, database, service, roots):
    files = inventory(roots)
    modes = {scope + "/": stat.S_IMODE(path.stat().st_mode) for scope, path in roots.items()}
    for entry in files:
        path = roots[entry["scope"]]
        if entry["scope"] != "config":
            path /= entry["path"]
        modes[entry["scope"] + "/" + entry["path"]] = stat.S_IMODE(path.stat().st_mode)
    return {"database_logical_dump_sha256": database_digest(cluster, database, service),
            "files_sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
            "file_modes_sha256": hashlib.sha256(json.dumps(modes, sort_keys=True).encode()).hexdigest()}


INTERRUPT_CODE = '''import json, os, signal, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import install, install_state
from journey_process import private_write
snapshot, prefix, marker = map(Path, sys.argv[2:5])
phase, identity = sys.argv[5:7]
from common import sha256
assert phase in ("before_swap", "after_swap")
assert sha256(snapshot / "manifest.json") == identity
destination_path = prefix / "lib/mike-archive/current"
original = install_state.os.replace
def replace(source, destination):
    if Path(destination) != destination_path:
        return original(source, destination)
    if phase == "after_swap":
        original(source, destination)
    private_write(marker, phase.encode())
    os.kill(os.getpid(), signal.SIGKILL)
install_state.os.replace = replace
install.install_verified(snapshot, json.loads((snapshot / "manifest.json").read_bytes()), prefix)
raise RuntimeError("interruption point was not reached")
'''


def interrupted_install(candidate, prefix, phase, root, home, cwd):
    require(phase in ("before_swap", "after_swap"), "UPGRADE_INTERRUPTION_PHASE")
    marker = root / (phase + ".marker")
    require(not marker.exists() and not marker.is_symlink(), "UPGRADE_INTERRUPTION_MARKER_EXISTS")
    candidate.snapshot_binding()
    args = [sys.executable, "-I", "-B", "-c", INTERRUPT_CODE, str(Path(__file__).resolve().parent),
            str(candidate.snapshot), str(prefix), str(marker), phase, candidate.identity]
    child = OwnedProcess(args, cwd=cwd, env=runtime_environment(home),
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        child.communicate(timeout=90)
        require(child.returncode == -signal.SIGKILL and marker.read_bytes() == phase.encode(),
                "UPGRADE_INTERRUPTION_NOT_OBSERVED")
    except subprocess.TimeoutExpired as error:
        raise JourneyError("UPGRADE_INSTALLER_TIMEOUT") from error
    finally:
        child.close()
    return {"point": phase, "owned_installer_exit_status": -signal.SIGKILL,
            "exact_candidate_archives": True, "injected_boundary": "atomic_current_link_replace",
            "migration_interruption": "NOT_RUN"}


def refuse_install(previous, candidate):
    candidate.recheck()
    before = tree_digest(candidate.prefix)
    try:
        install_verified(previous.snapshot, previous.manifest, candidate.prefix)
    except ReleaseError as error:
        require(str(error).startswith("downgrade or changed same-version candidate refused"),
                "UPGRADE_DOWNGRADE_WRONG_REFUSAL")
    else:
        raise JourneyError("UPGRADE_INSTALLER_ACCEPTED_DOWNGRADE")
    require(tree_digest(candidate.prefix) == before, "UPGRADE_DOWNGRADE_CHANGED_INSTALLATION")
    candidate.recheck()
    return {"older_pair_refused": True, "complete_install_tree_unchanged": True}
