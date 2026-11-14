"""Tracked plugin assets and descriptor-relative archive paths."""

import os
from pathlib import PurePosixPath
import re
import subprocess

from common import ReleaseError, source_git

PREFIX = "share/mike/mods/"
SCRIPTS_PREFIX = "share/mike/scripts/"
MAX_ASSET_BYTES = 4 * 1024 * 1024


def is_asset(path):
    return (isinstance(path, str) and path.startswith((PREFIX, SCRIPTS_PREFIX))
            and all(re.fullmatch(r"[A-Za-z0-9_.@+-]+", part)
                    and part not in (".", "..") for part in path.split("/")))


def parents(paths):
    return {str(parent) for path in paths for parent in PurePosixPath(path).parents
            if str(parent) != "."}


def payload(root):
    if not (root / "mods").exists():
        return []
    records = source_git(root, "ls-files", "--stage", "-z", "--", "mods")
    contents = []
    for record in filter(None, records.split("\0")):
        metadata, path = record.split("\t", 1)
        mode, digest, stage = metadata.split()
        name = "share/mike/" + path
        if "tests" in path.split("/"):
            continue
        if not is_asset(name) or mode not in ("100644", "100755") or stage != "0":
            raise ReleaseError("mods must be tracked regular files without merge conflicts")
        data = blob(root, digest)
        if len(data) > MAX_ASSET_BYTES:
            raise ReleaseError("mod asset exceeds package limit")
        contents.append((name, data, int(mode, 8) & 0o777))
    return contents


def blob(root, digest):
    environment = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
    environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                       GIT_NO_REPLACE_OBJECTS="1", GIT_NO_LAZY_FETCH="1")
    return subprocess.run(["git", "cat-file", "blob", digest], cwd=root, env=environment,
                          check=True, capture_output=True).stdout


def create_file(root, path, mode):
    parts = path.split("/")
    descriptor = os.dup(root)
    try:
        for part in parts[:-1]:
            try:
                os.mkdir(part, mode=0o700, dir_fd=descriptor)
            except FileExistsError:
                pass
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                            dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                       mode, dir_fd=descriptor)
    finally:
        os.close(descriptor)
