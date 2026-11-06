#!/usr/bin/env python3
"""Reject a Linux candidate whose binaries need a newer glibc than the 2.35 floor."""

import argparse
import json
from pathlib import Path
import re
import sys

from common import ReleaseError

FLOOR = (2, 35, 0)
VERSION = re.compile(r"\bGLIBC_([0-9]+)\.([0-9]+)(?:\.([0-9]+))?\b")


def require(condition, code):
    if not condition:
        raise ReleaseError(code)


def highest_glibc(report):
    versions = [tuple(int(part or 0) for part in match.groups()) for match in VERSION.finditer(report)]
    return max(versions) if versions else None


def check_manifest(manifest, floor=FLOOR):
    require(isinstance(manifest, dict) and isinstance(manifest.get("artifacts"), list), "glibc_manifest_shape")
    if not str(manifest.get("target", "")).endswith("linux-gnu"):
        return {}
    highest = {}
    for artifact in manifest["artifacts"]:
        requirements = artifact.get("runtime_requirements") if isinstance(artifact, dict) else None
        report = requirements.get("symbol_versions") if isinstance(requirements, dict) else None
        require(isinstance(report, str) and report, "glibc_symbol_version_report_missing")
        found = highest_glibc(report)
        require(found is not None, "glibc_symbol_versions_not_observed")
        require(found <= floor, "glibc_floor_exceeded_" + artifact.get("binary", "binary"))
        highest[artifact["binary"]] = ".".join(map(str, found[: 3 if found[2] else 2]))
    return highest


def main(argv=None):
    cli = argparse.ArgumentParser(description="Check a candidate manifest against the glibc 2.35 floor.")
    cli.add_argument("manifest", type=Path)
    args = cli.parse_args(argv)
    try:
        print(json.dumps(check_manifest(json.loads(args.manifest.read_text())), sort_keys=True))
    except (ReleaseError, OSError, ValueError) as error:
        print(f"glibc floor check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
