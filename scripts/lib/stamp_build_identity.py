#!/usr/bin/env python3
"""Write a commit's build identity into a linked mike binary.

A deploy builds mike through the mbx cache with a fixed identity, so a new commit
does not change the compile key of crate mike-base and every crate above it. The
binary holds a 256-byte slot (crates/mike-base/src/change_log/build.rs) that opens
with MARKER; this script writes `sha TAB short TAB dirty TAB built_at NUL` after
the marker, and the binary reports that identity on /api/health and --build-info.

Usage:
  stamp_build_identity.py BINARY --repo ROOT        identity of ROOT's HEAD and tree
  stamp_build_identity.py BINARY --sha S --short S --dirty true|false --built-at T

built_at is SOURCE_DATE_EPOCH when set, else HEAD's commit time for a clean tree (so the
same commit stamps the same bytes, and deploy-proxy.sh's byte-identical smart-skip still
holds), else the current time.

The binary is rewritten through a new file (never through a hard link into the mbx
store) and, on macOS, signed again ad hoc. Exits 1 when the binary does not hold
exactly one slot or the identity does not fit.
"""

from __future__ import annotations

import argparse
import datetime
import os
import subprocess
import sys
import tempfile

MARKER = b"MIKE-BUILD-STAMP-SLOT-V1:"
SLOT_LEN = 256


class StampError(Exception):
    pass


def payload_for(sha: str, short: str, dirty: bool, built_at: str) -> bytes:
    fields = [sha, short, "true" if dirty else "false", built_at]
    if any(not field or "\t" in field or "\0" in field for field in fields):
        raise StampError(f"identity fields must be non-empty and free of tabs: {fields}")
    payload = "\t".join(fields).encode("utf-8") + b"\0"
    if len(payload) > SLOT_LEN - len(MARKER):
        raise StampError(f"identity is {len(payload)} bytes; the slot holds {SLOT_LEN - len(MARKER)}")
    return payload.ljust(SLOT_LEN - len(MARKER), b"\0")


def stamp_bytes(data: bytes, payload: bytes) -> bytes:
    count = data.count(MARKER)
    if count != 1:
        raise StampError(f"the binary holds {count} identity slots; exactly one is needed")
    start = data.index(MARKER) + len(MARKER)
    return data[:start] + payload + data[start + len(payload):]


def stamp_file(path: str, payload: bytes) -> None:
    with open(path, "rb") as handle:
        data = handle.read()
    stamped = stamp_bytes(data, payload)
    folder = os.path.dirname(os.path.abspath(path))
    fd, tmp = tempfile.mkstemp(prefix=".stamp.", dir=folder)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(stamped)
        os.chmod(tmp, os.stat(path).st_mode & 0o7777 | 0o200)
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise
    if sys.platform == "darwin":
        subprocess.run(["codesign", "--force", "--sign", "-", path], check=True,
                       stdout=subprocess.DEVNULL)


def git(repo: str, *args: str) -> str:
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, GIT_OPTIONAL_LOCKS="0")
    out = subprocess.run(["git", "-C", repo, *args], env=env, check=True,
                         capture_output=True, text=True)
    return out.stdout.strip()


def utc_text(epoch: int) -> str:
    moment = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc)
    return moment.strftime("%Y-%m-%dT%H:%M:%SZ")


def built_at(repo: str, dirty: bool) -> str:
    epoch = os.environ.get("SOURCE_DATE_EPOCH")
    if epoch:
        return utc_text(int(epoch))
    if not dirty:
        return utc_text(int(git(repo, "log", "-1", "--format=%ct", "HEAD")))
    return utc_text(int(datetime.datetime.now(datetime.timezone.utc).timestamp()))


def identity_from_repo(repo: str) -> tuple[str, str, bool, str]:
    sha = git(repo, "rev-parse", "HEAD")
    short = git(repo, "rev-parse", "--short", "HEAD")
    dirty = git(repo, "status", "--porcelain", "--untracked-files=no") != ""
    return sha, short, dirty, built_at(repo, dirty)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("binary")
    parser.add_argument("--repo")
    parser.add_argument("--sha")
    parser.add_argument("--short")
    parser.add_argument("--dirty", choices=["true", "false"])
    parser.add_argument("--built-at")
    args = parser.parse_args(argv)
    try:
        if args.repo:
            sha, short, dirty, built_at = identity_from_repo(args.repo)
        elif args.sha and args.short and args.dirty and args.built_at:
            sha, short, dirty, built_at = args.sha, args.short, args.dirty == "true", args.built_at
        else:
            parser.error("pass --repo, or all of --sha --short --dirty --built-at")
        stamp_file(args.binary, payload_for(sha, short, dirty, built_at))
    except (StampError, OSError, subprocess.CalledProcessError) as error:
        print(f"stamp_build_identity: {args.binary}: {error}", file=sys.stderr)
        return 1
    print(f"{sha} {'dirty' if dirty else 'clean'} {built_at}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
