"""Private, non-following file snapshots for maintenance; no arbitrary extraction."""

from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat

from common import ReleaseError
from install_state import sync_directory

EPHEMERAL = {"daemon.lock", "start.lock", "control.sock", "instance.json"}
MAX_INDEX_BYTES = 32 * 1024 * 1024


def plain_path(path, *, directory=False, private=False):
    path = Path(os.path.abspath(path))
    for parent in reversed((path.parent, *path.parent.parents)):
        if parent.is_symlink() or not parent.is_dir():
            raise ReleaseError("maintenance paths must not traverse symlinks")
    info = path.lstat()
    kind = stat.S_ISDIR if directory else stat.S_ISREG
    if not kind(info.st_mode) or info.st_uid != os.getuid():
        raise ReleaseError("maintenance input must be a plain file/directory owned by this user")
    if info.st_mode & (0o077 if private else 0o022):
        raise ReleaseError("maintenance input has unsafe shared permissions")
    return path


def new_destination(path):
    path = Path(os.path.abspath(path))
    plain_path(path.parent, directory=True, private=True)
    if path.exists() or path.is_symlink():
        raise ReleaseError("destination already exists; choose a fresh private destination")
    return path


def private_json(path, value):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "w") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())


def read_json(path, limit=MAX_INDEX_BYTES):
    plain_path(path, private=True)
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ReleaseError("maintenance metadata exceeds its size limit")
    return json.loads(data)


def file_digest(path, destination=None):
    plain_path(path)
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    digest, size = hashlib.sha256(), 0
    with os.fdopen(descriptor, "rb") as source:
        before = os.fstat(source.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise ReleaseError("special files and hard-linked maintenance inputs are refused")
        output = None
        try:
            if destination is not None:
                fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
                output = os.fdopen(fd, "wb")
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
                size += len(block)
                if output:
                    output.write(block)
            if output:
                output.flush()
                os.fsync(output.fileno())
        finally:
            if output:
                output.close()
        after = os.fstat(source.fileno())
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
                after.st_size, after.st_mtime_ns, after.st_ctime_ns) or size != before.st_size:
            raise ReleaseError("maintenance input changed while reading; stop all writers and retry")
    return {"sha256": digest.hexdigest(), "size": size}


@contextmanager
def stopped_installation(state):
    """Hold the exact Rust lifecycle leases throughout the coordinated snapshot."""
    plain_path(state, directory=True, private=True)
    held = []
    try:
        names = ["start.lock", "daemon.lock"]
        if (state / "database").exists():
            plain_path(state / "database", directory=True, private=True)
            names.append("database/operation.lock")
        for name in names:
            descriptor = os.open(state / name, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
            held.append(descriptor)
            info = os.fstat(descriptor)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise ReleaseError("unsafe lifecycle lock; no process was signalled")
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise ReleaseError("selected installation is running or starting; stop it first") from exc
        yield
    finally:
        for descriptor in reversed(held):
            os.close(descriptor)


def validated_roots(roots):
    """Allow disjoint roots or the exact historical shared-root layout only.

    Logical bundle scopes remain disjoint even when their source directories
    overlap. The installed runtime must separately attest these selected paths.
    """
    if set(roots) != {"config", "data", "state"}:
        raise ReleaseError("maintenance requires exactly config, data and state roots")
    roots = {scope: plain_path(roots[scope], directory=(scope != "config"), private=True)
             for scope in ("config", "data", "state")}
    shared = (roots["config"] == roots["data"] / "config.toml"
              and roots["state"] == roots["data"] / "state")
    if not shared:
        for scope, root in roots.items():
            if any(root == other or root in other.parents or other in root.parents
                   for key, other in roots.items() if key != scope):
                raise ReleaseError("unsupported overlapping maintenance roots; use separate non-overlapping roots "
                                   "or the exact config.toml and state/ shared-data layout; existing paths were not moved. "
                                   "Verify print-config --paths; changing --config or data_dir alone is not migration")
    return roots


def scope_paths(root, scope, roots):
    """Walk in stable lexical order without descending into another scope."""
    for path in sorted(root.iterdir()):
        if scope == "data" and path in (roots["config"], roots["state"]):
            # Recheck the boundary before excluding it. A replaced/symlinked
            # state directory must fail, never silently disappear from backup.
            plain_path(path, directory=(path == roots["state"]), private=True)
            continue
        relative = path.relative_to(roots[scope]).as_posix()
        if scope == "state" and (relative == "database/operation.lock"
                                  or relative.split("/")[0] in EPHEMERAL
                                  or ("/" not in relative and relative.startswith("boot-")
                                      and relative.endswith(".toml"))):
            continue
        directory = path.is_dir()
        plain_path(path, directory=directory)
        yield path
        if directory:
            yield from scope_paths(path, scope, roots)


def inventory(roots):
    roots = validated_roots(roots)
    entries = []
    for scope, root in roots.items():
        paths = [root] if scope == "config" else scope_paths(root, scope, roots)
        for path in paths:
            relative = "config.toml" if scope == "config" else path.relative_to(root).as_posix()
            plain_path(path, directory=path.is_dir())
            entry = {"scope": scope, "path": relative, "kind": "directory" if path.is_dir() else "file"}
            if entry["kind"] == "file":
                entry.update(file_digest(path))
                entry["payload"] = f"{len(entries):08d}"
            entries.append(entry)
    return entries


def copy_inventory(roots, entries, payload):
    roots = validated_roots(roots)
    payload.mkdir(mode=0o700)
    for item in entries:
        if item["kind"] != "file":
            continue
        source = roots[item["scope"]]
        if item["scope"] != "config":
            source = source / item["path"]
        copied = file_digest(source, payload / item["payload"])
        if copied != {key: item[key] for key in ("sha256", "size")}:
            raise ReleaseError("file changed between inventory and copy; backup discarded")
    sync_directory(payload)


def validate_index(entries):
    if not isinstance(entries, list):
        raise ReleaseError("invalid private recovery index")
    seen, directories, payloads, configs = set(), set(), set(), 0
    for item in entries:
        scope, name, kind = (item.get(key) for key in ("scope", "path", "kind"))
        if (scope not in ("config", "state", "data") or not isinstance(name, str)
                or not name or "\\" in name or "\x00" in name or
                PurePosixPath(name).as_posix() != name or
                any(part in ("", ".", "..") for part in name.split("/")) or name.startswith("/")):
            raise ReleaseError("unsafe recovery path")
        key = (scope, name)
        if key in seen or kind not in ("directory", "file"):
            raise ReleaseError("duplicate or invalid recovery entry")
        seen.add(key)
        if kind == "directory":
            directories.add(key)
        if scope == "config":
            if name != "config.toml" or kind != "file":
                raise ReleaseError("invalid boot configuration entry")
            configs += 1
        required = {"scope", "path", "kind"}
        if kind == "file":
            required |= {"payload", "size", "sha256"}
            number = item.get("payload")
            if not isinstance(number, str) or len(number) != 8 or not number.isascii() or not number.isdigit() or number in payloads:
                raise ReleaseError("invalid recovery payload identity")
            payloads.add(number)
            if (type(item.get("size")) is not int or item["size"] < 0 or
                    not isinstance(item.get("sha256"), str) or len(item["sha256"]) != 64 or
                    any(c not in "0123456789abcdef" for c in item["sha256"])):
                raise ReleaseError("invalid recovery payload checksum")
        if set(item) != required:
            raise ReleaseError("unexpected recovery index fields")
    if configs != 1:
        raise ReleaseError("backup must contain exactly one boot configuration")
    for item in entries:
        parent = PurePosixPath(item["path"]).parent
        while str(parent) != ".":
            if (item["scope"], str(parent)) not in directories:
                raise ReleaseError("recovery entry has a missing/non-directory parent")
            parent = parent.parent
    return payloads
