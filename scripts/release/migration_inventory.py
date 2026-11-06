"""Exact, tracked SQLx migration inputs for an already identified clean source.

Only the repository's simple, forward-only embedded migrations are supported.
SQLx hashes the unmodified UTF-8 SQL bytes with SHA-384, including line endings.
This inventory is source evidence, never a replacement for executable identity.
"""

import hashlib
import os
from pathlib import Path
import re
import stat

from common import ReleaseError, clean_source, source_git

SOURCE_PATH = "crates/mike/src/store/persistence/migrations"
MAX_VERSION = 2**63 - 1


def migration_version(filename):
    if not isinstance(filename, str) or not re.fullmatch(r"[0-9]+_[A-Za-z0-9][A-Za-z0-9_]*\.sql", filename):
        raise ReleaseError("unsupported or malformed embedded migration filename")
    version = int(filename.split("_", 1)[0])
    if not 0 < version <= MAX_VERSION:
        raise ReleaseError("embedded migration version must be a positive SQLx i64")
    return version


def validate_migration_inventory(value, source_commit):
    if (not isinstance(source_commit, str) or not re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", source_commit)
            or not isinstance(value, dict) or set(value) != {
                "schema_version", "source_commit", "source_path", "checksum_algorithm", "migrations"}
            or type(value["schema_version"]) is not int or value["schema_version"] != 1
            or value["source_commit"] != source_commit or value["source_path"] != SOURCE_PATH
            or value["checksum_algorithm"] != "sha384"
            or not isinstance(value["migrations"], list) or not value["migrations"]):
        raise ReleaseError("missing or invalid candidate migration inventory")
    versions = []
    for item in value["migrations"]:
        if (not isinstance(item, dict) or set(item) != {"version", "filename", "checksum"}
                or type(item["version"]) is not int
                or item["version"] != migration_version(item["filename"])
                or not isinstance(item["checksum"], str)
                or not re.fullmatch(r"[0-9a-f]{96}", item["checksum"])):
            raise ReleaseError("invalid candidate migration inventory entry")
        versions.append(item["version"])
    if versions != sorted(set(versions)):
        raise ReleaseError("candidate migration versions must be unique and numerically ordered")
    return value


def tracked_migrations(root, commit):
    records = source_git(root, "ls-tree", "-r", "-z", "--full-tree", commit, "--", SOURCE_PATH)
    result = {}
    for record in filter(None, records.split("\0")):
        try:
            metadata, path = record.split("\t", 1)
            mode, kind, digest = metadata.split(" ")
        except ValueError as error:
            raise ReleaseError("malformed tracked migration entry") from error
        prefix = SOURCE_PATH + "/"
        if not path.startswith(prefix) or mode not in ("100644", "100755") or kind != "blob":
            raise ReleaseError("embedded migration input must be a tracked regular file")
        filename = path[len(prefix):]
        migration_version(filename)
        if filename in result or not re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", digest):
            raise ReleaseError("duplicate or invalid tracked migration entry")
        result[filename] = digest
    if not result:
        raise ReleaseError("tracked embedded migration inputs are missing")
    return result


def migration_directory(root):
    descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for component in SOURCE_PATH.split("/"):
            child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def migration_checksum(directory, filename, expected_blob):
    descriptor = os.open(filename, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
    with os.fdopen(descriptor, "rb") as source:
        metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(metadata.st_mode):
            raise ReleaseError("embedded migration input must be a regular file")
        data = source.read()
    try:
        data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ReleaseError("embedded migration SQL must be valid UTF-8") from error
    algorithm = "sha1" if len(expected_blob) == 40 else "sha256"
    actual_blob = hashlib.new(algorithm, b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    if actual_blob != expected_blob:
        raise ReleaseError("embedded migration bytes differ from the clean source commit")
    return hashlib.sha384(data).hexdigest()


def source_migration_inventory(root, source_commit):
    root = Path(root)
    clean_source(root, source_commit)
    tracked = tracked_migrations(root, source_commit)
    try:
        descriptor = migration_directory(root)
        try:
            if set(os.listdir(descriptor)) != set(tracked):
                raise ReleaseError("embedded migration directory has missing or untracked inputs")
            migrations = [{"version": migration_version(name), "filename": name,
                           "checksum": migration_checksum(descriptor, name, tracked[name])}
                          for name in sorted(tracked, key=migration_version)]
        finally:
            os.close(descriptor)
    except OSError as error:
        raise ReleaseError("embedded migration input is missing, nonregular or unreadable") from error
    clean_source(root, source_commit)
    value = {"schema_version": 1, "source_commit": source_commit, "source_path": SOURCE_PATH,
             "checksum_algorithm": "sha384", "migrations": migrations}
    return validate_migration_inventory(value, source_commit)
