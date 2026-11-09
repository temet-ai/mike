#!/usr/bin/env python3
"""Install an explicitly trusted local candidate; no download, build, or profile edit."""

import argparse
import json
import os
from pathlib import Path
import re
import sys

from common import ReleaseError, sha256
from inputs import extract_pair, interruption_cleanup, verified_inputs
from install_state import (check_links, clean_pending, commit_generation,
                           current_generation, initialize, installation_lock,
                           safe_prefix, sync_directory, validate_generation, write_owner)


def semver(value):
    match = re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
                         r"(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?", value)
    if not match:
        raise ReleaseError("installer requires a SemVer candidate version")
    major, minor, patch, prerelease = match.groups()
    identifiers = []
    if prerelease:
        for item in prerelease.split("."):
            if not item or (item.isdigit() and len(item) > 1 and item.startswith("0")):
                raise ReleaseError("invalid SemVer prerelease")
            identifiers.append((0, int(item)) if item.isdigit() else (1, item))
    return (int(major), int(minor), int(patch), prerelease is None, tuple(identifiers))


def check_upgrade(previous, manifest, same, rebuild_same_version=False):
    proposed = semver(manifest["version"])
    if previous is None or same:
        return
    installed = semver(previous["version"])
    if rebuild_same_version and proposed == installed:
        return
    if proposed <= installed:
        raise ReleaseError("downgrade or changed same-version candidate refused; preserve your backup "
                           "and use a separately reviewed recovery procedure")


def stage_generation(snapshot, manifest, state, identity):
    generations = state / "generations"
    destination = generations / identity
    if destination.exists() or destination.is_symlink():
        validate_generation(destination, identity)
        return
    stage = generations / (".pending-" + identity)
    clean_pending(stage)
    stage.mkdir(mode=0o700)
    write_owner(stage)
    try:
        if extract_pair(snapshot, manifest, stage) != identity:
            raise ReleaseError("candidate manifest changed after verification")
        for path in stage.rglob("*"):
            if path.is_file():
                with path.open("rb") as stream:
                    os.fsync(stream.fileno())
        validate_generation(stage, identity)
        for path in stage.iterdir():
            if path.is_dir():
                sync_directory(path)
        sync_directory(stage)
        stage.rename(destination)
        sync_directory(generations)
    finally:
        clean_pending(stage)


def install_verified(snapshot, manifest, prefix, rebuild_same_version=False):
    prefix = safe_prefix(prefix)
    # Reject invalid versions before even making the private installation root.
    semver(manifest["version"])
    state = initialize(prefix)
    identity = sha256(snapshot / "manifest.json")
    with installation_lock(state):
        check_links(prefix, True)
        old_identity, previous = current_generation(state)
        check_upgrade(previous, manifest, old_identity == identity, rebuild_same_version)
        stage_generation(snapshot, manifest, state, identity)
        commit_generation(prefix, state, identity)
        current, _ = current_generation(state)
        if current != identity:
            raise ReleaseError("installation commit did not select the candidate")
    return {"version": manifest["version"], "source_commit": manifest["source_commit"],
            "manifest_sha256": identity, "prefix": str(prefix),
            "status": "unchanged" if old_identity == identity else "installed",
            "qualified": False}


def install(candidate, prefix, rebuild_same_version=False):
    # Integrity, platform, allowlisted extraction and both executable versions are
    # checked before inspecting or modifying the installation prefix.
    with verified_inputs(candidate) as (snapshot, manifest):
        return install_verified(snapshot, manifest, prefix, rebuild_same_version)


def main():
    parser = argparse.ArgumentParser(prog="scripts/install-release", description=__doc__, epilog=
        "Python 3.11+ and the native runtime are required. Candidate files must be trusted: "
        "checksums detect corruption, not authenticity. Validation runs the selected binaries. "
        "No service is stopped or restarted; no data migration is run. This is not release qualification.")
    parser.add_argument("--candidate", required=True, type=Path,
                        help="local directory containing manifest.json and both archive/checksum pairs")
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".local",
                        help="user-owned prefix (default: ~/.local); stable commands are PREFIX/bin/mike[-mcp]")
    parser.add_argument("--json", action="store_true", help="print install receipt as JSON")
    parser.add_argument("--rebuild-same-version", action="store_true",
                        help="accept a candidate of the installed version built from another commit "
                             "(a machine deploying successive commits); an older version is still refused")
    args = parser.parse_args()
    result = install(args.candidate, args.prefix, args.rebuild_same_version)
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(f"{result['status'].capitalize()}: mike and mike-mcp {result['version']}")
        print(f"Stable commands: {result['prefix']}/bin/mike and {result['prefix']}/bin/mike-mcp")
        print(f"Add {result['prefix']}/bin to PATH yourself if needed. No shell profile was changed.")
        print("Candidate remains unqualified. Existing processes and data were not changed.")


if __name__ == "__main__":
    try:
        with interruption_cleanup():
            main()
    except (ReleaseError, OSError, ValueError, KeyError, TypeError) as exc:
        print(f"install-release: {exc}", file=sys.stderr)
        sys.exit(1)
