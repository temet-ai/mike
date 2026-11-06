#!/usr/bin/env python3
"""Prepare current-version local archives without publication or source edits."""

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile

from archive import prepare_archives, source_timestamp, verify_candidate
from common import (ReleaseError, build_environment, clean_source, native_target, run,
                    sha256, source_git, spa_inventory, toolchain, workspace_version, write_json)
from migration_inventory import source_migration_inventory
from notice_inputs import prepare_notice, verify_notice
from rust_paths import PATH_DESTINATIONS, cargo_path_config

ROOT = Path(__file__).resolve().parents[2]


def plain_directory(path):
    if path.is_symlink() or (path.exists() and not path.is_dir()):
        raise ReleaseError(f"expected a non-symlink directory: {path}")
    path.mkdir(exist_ok=True)


def preflight(root, requested, target):
    version = workspace_version(root, requested)
    commit = clean_source(root)
    env = build_environment()
    native = native_target(root, env)
    if target is not None and target != native:
        raise ReleaseError(f"requested target {target} is not native {native}; cross-builds are unsupported")
    versions = toolchain(root, env, native)
    inspector = "readelf" if native.endswith("linux-gnu") else "otool"
    if shutil.which(inspector, path=env.get("PATH")) is None:
        raise ReleaseError(f"install {inspector} before preparing; no tools are installed automatically")
    for filename in (".env", ".env.local", ".env.production", ".env.production.local"):
        if (root / "frontend" / filename).exists():
            raise ReleaseError(f"frontend/{filename} could inject private build values; use a clean checkout")
    for path in ("dist-out/.ignore-check", "frontend/build/.ignore-check", "frontend/node_modules/.ignore-check"):
        source_git(root, "check-ignore", "--no-index", "--quiet", path)
    epoch = int(source_git(root, "show", "-s", "--format=%ct", "HEAD"))
    source_timestamp(epoch)
    return version, commit, native, versions, env, epoch


def build(root, target, build_dir, env):
    frontend = root / "frontend"
    output = frontend / "build"
    if output.is_symlink() or (output.exists() and not output.is_dir()):
        raise ReleaseError("frontend/build must be a non-symlink directory")
    if output.exists():
        shutil.rmtree(output)
    print("==> Building the production SPA from frozen, offline dependencies", flush=True)
    run(["pnpm", "install", "--offline", "--frozen-lockfile"], cwd=frontend, env=env)
    run(["pnpm", "build"], cwd=frontend, env=dict(env, NODE_ENV="production"))
    assets = spa_inventory(root)
    print(f"==> Building native {target} with locked dependencies and SQLX_OFFLINE=true", flush=True)
    for name in ("mike", "mike-mcp"):
        previous = build_dir / target / "dist" / name
        if previous.is_symlink():
            raise ReleaseError(f"refusing symlink build output: {previous}")
        if previous.exists():
            previous.unlink()
    run(["cargo", "build", "--offline", "--locked", "--profile", "dist", "--target", target,
         "-p", "mike", "-p", "mike-mcp", "--config", cargo_path_config(root, env)], cwd=root,
        env=dict(env, CARGO_TARGET_DIR=str(build_dir)))
    return assets


def finalize_candidate(staging, output, manifest):
    # Local finalization only. The directory is content-addressed and never overwritten.
    candidate_id = hashlib.sha256((staging / "manifest.json").read_bytes()).hexdigest()[:16]
    parent = output / f"v{manifest['version']}"
    plain_directory(parent)
    parent = parent / manifest["target"]
    plain_directory(parent)
    destination = parent / f"{manifest['source_commit']}-{candidate_id}"
    if destination.exists() or destination.is_symlink():
        if destination.is_symlink() or not destination.is_dir():
            raise ReleaseError("candidate destination is not a safe directory")
        verify_candidate(destination)
        if (destination / "manifest.json").read_bytes() != (staging / "manifest.json").read_bytes():
            raise ReleaseError("candidate already exists with different bytes; refusing overwrite")
        print("==> Identical candidate already exists; preserved without modification", flush=True)
    else:
        staging.rename(destination)
    return destination


def prepare(root, requested, target, reviewed_notices=None):
    version, commit, target, versions, env, epoch = preflight(root, requested, target)
    migrations = source_migration_inventory(root, commit)
    output = root / "dist-out"
    plain_directory(output)
    lock = output / ".prepare.lock"
    try:
        lock.mkdir()
    except FileExistsError as exc:
        raise ReleaseError("another preparation may be running (dist-out/.prepare.lock); "
                           "do not remove its lock until that process has stopped") from exc
    staging = None
    try:
        (lock / "pid").write_text(str(os.getpid()) + "\n")
        build_dir = output / ".build"
        plain_directory(build_dir)
        for path in (build_dir / target, build_dir / target / "dist"):
            plain_directory(path)
        staging = Path(tempfile.mkdtemp(prefix=".prepare-", dir=output))
        env["SOURCE_DATE_EPOCH"] = str(epoch)
        assets = build(root, target, build_dir, env)
        notice = prepare_notice(root, reviewed_notices, commit, version, target, assets)
        artifacts = prepare_archives(root, staging, build_dir, version, target, epoch, env, commit, notice)
        clean_source(root, commit)
        if source_migration_inventory(root, commit) != migrations:
            raise ReleaseError("embedded migration inputs changed during preparation")
        manifest = {
            "schema_version": 1, "state": "prepared", "qualified": False,
            "version": version, "source_commit": commit, "source_date_epoch": epoch,
            "target": target, "toolchain": versions,
            "build": {"profile": "dist", "cargo_locked": True, "offline": True, "sqlx_offline": True,
                      "rust_path_remapping": PATH_DESTINATIONS,
                      "cargo_lock_sha256": sha256(root / "Cargo.lock"),
                      "pnpm_lock_sha256": sha256(root / "frontend/pnpm-lock.yaml")},
            "spa_source_assets": assets, "artifacts": artifacts, "migration_inventory": migrations,
            "checks": {"archive_integrity": "passed", "archived_cli_smoke": "passed",
                       "source_unchanged": "passed", "executable_source_identity": "passed",
                       "embedded_spa_runtime": "not_run",
                       "private_material_review": "not_run", "third_party_notices": "not_run",
                       "clean_account_install": "not_run", "platform_runtime_qualification": "not_run"},
            "publication": "unavailable; preparation is not approval to publish",
        }
        manifest["third_party_notice_inputs"] = verify_notice(notice, manifest)
        write_json(staging / "manifest.json", manifest)
        destination = finalize_candidate(staging, output, manifest)
        print(f"Candidate: {destination}")
        print("Prepared, not qualified. No tag, release, source edit, or push was performed.")
        return destination
    finally:
        if staging is not None and staging.exists():
            shutil.rmtree(staging)
        shutil.rmtree(lock)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", nargs="?")
    parser.add_argument("--target")
    parser.add_argument("--verify", type=Path, metavar="CANDIDATE_DIRECTORY")
    parser.add_argument("--reviewed-notices", type=Path,
                        help="optional offline source/target-bound notice review manifest with adjacent license texts; never grants publication permission")
    parser.add_argument("--publish", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.publish:
        raise ReleaseError("publication is unavailable: qualify the prepared archive and implement "
                           "permission-preflighted, same-byte publication before enabling it")
    if args.verify:
        if args.version or args.target or args.reviewed_notices:
            raise ReleaseError("--verify takes only a candidate directory, not a version, target or review input")
        manifest = verify_candidate(args.verify.resolve())
        print(f"Verified archived bytes and local CLI smoke: v{manifest['version']} ({manifest['target']})")
        print("Candidate remains unqualified; this does not replace clean-account/runtime checks.")
    else:
        prepare(ROOT, args.version, args.target, args.reviewed_notices)


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError, ValueError, KeyError, TypeError, tarfile.TarError, subprocess.SubprocessError) as exc:
        print(f"release.sh: {exc}", file=sys.stderr)
        sys.exit(1)
