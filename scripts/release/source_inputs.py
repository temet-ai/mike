"""Verify the bounded production-source trees against actual Git-index blobs.

This is a source-binding guard, not a hermetic or reproducible-build claim.
Build outputs and dependency caches outside these roots remain out of scope.
There are deliberately no ignored/generated-directory exceptions inside them.
"""

import hashlib
import os
import re
import stat

from common import ReleaseError, source_git

SOURCE_ROOTS = ("crates", "frontend/src", "frontend/static", ".sqlx", ".cargo")
DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW


def tracked_inputs(root):
    records = source_git(root, "ls-files", "--stage", "-z", "--", *SOURCE_ROOTS)
    trees = {path: {} for path in SOURCE_ROOTS}
    for record in filter(None, records.split("\0")):
        try:
            metadata, path = record.split("\t", 1)
            mode, digest, stage = metadata.split(" ")
        except ValueError as error:
            raise ReleaseError("malformed tracked source input") from error
        scope = next((prefix for prefix in SOURCE_ROOTS if path.startswith(prefix + "/")), None)
        if (scope is None or mode not in ("100644", "100755") or stage != "0"
                or not re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", digest)):
            raise ReleaseError(f"source input must be an unmerged-free tracked regular file: {path}")
        parts = path[len(scope) + 1:].split("/")
        if any(part in ("", ".", "..") for part in parts):
            raise ReleaseError(f"invalid tracked source input path: {path}")
        directory = trees[scope]
        for part in parts[:-1]:
            directory = directory.setdefault(part, {})
            if not isinstance(directory, dict):
                raise ReleaseError(f"conflicting tracked source input path: {path}")
        if parts[-1] in directory:
            raise ReleaseError(f"duplicate tracked source input path: {path}")
        directory[parts[-1]] = (mode, digest)
    return trees


def verify_file(directory, name, path, expected):
    mode, expected_blob = expected
    descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
    with os.fdopen(descriptor, "rb") as source:
        metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(metadata.st_mode):
            raise ReleaseError(f"source input must be a regular file: {path}")
        # Git records the owner executable bit, independent of the checkout umask.
        if bool(metadata.st_mode & stat.S_IXUSR) != (mode == "100755"):
            raise ReleaseError(f"source input executable mode differs from Git: {path}")
        algorithm = "sha1" if len(expected_blob) == 40 else "sha256"
        digest = hashlib.new(algorithm, b"blob " + str(metadata.st_size).encode() + b"\0")
        length = 0
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
            length += len(block)
        after = os.fstat(source.fileno())
    if (length != metadata.st_size or after.st_size != metadata.st_size
            or after.st_mtime_ns != metadata.st_mtime_ns or after.st_ctime_ns != metadata.st_ctime_ns
            or digest.hexdigest() != expected_blob):
        raise ReleaseError(f"source input bytes differ from Git: {path}")


def verify_directory(descriptor, path, expected):
    names = set(os.listdir(descriptor))
    missing = set(expected) - names
    if missing:
        raise ReleaseError(f"source tree has missing tracked inputs: {path}/{sorted(missing)[0]}")
    extra = names - set(expected)
    if extra:
        raise ReleaseError(f"source tree has untracked inputs (including ignored entries): "
                           f"{path}/{sorted(extra)[0]}")
    for name in sorted(names):
        child_path = f"{path}/{name}"
        metadata = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
        if stat.S_ISLNK(metadata.st_mode):
            raise ReleaseError(f"source input symlink is unsupported: {child_path}")
        if isinstance(expected[name], dict):
            child = os.open(name, DIRECTORY_FLAGS, dir_fd=descriptor)
            try:
                verify_directory(child, child_path, expected[name])
            finally:
                os.close(child)
        else:
            verify_file(descriptor, name, child_path, expected[name])


def open_source_directory(root, path):
    descriptor = os.open(root, DIRECTORY_FLAGS)
    try:
        for component in path.split("/"):
            child = os.open(component, DIRECTORY_FLAGS, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def verify_source_inputs(root):
    """Reject undeclared entries and hash regular inputs without following links."""
    trees = tracked_inputs(root)
    for path, expected in trees.items():
        try:
            try:
                descriptor = open_source_directory(root, path)
            except FileNotFoundError:
                if expected:
                    raise ReleaseError(f"source tree has missing tracked inputs: {path}")
                continue
            try:
                if not expected:
                    raise ReleaseError(f"source tree has untracked inputs (unexpected directory): {path}")
                verify_directory(descriptor, path, expected)
            finally:
                os.close(descriptor)
        except OSError as error:
            raise ReleaseError(f"source input is missing, unreadable, symlinked or nonregular: {path}") from error
