"""Read-only preflight and explicit build inputs for local release candidates."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


class ReleaseError(Exception):
    pass


# Every target an archive is built for, each natively on its own host:
# triple -> (uname -s, uname -m, `mike doctor` os, `mike doctor` architecture).
TARGETS = {"x86_64-unknown-linux-gnu": ("Linux", "x86_64", "linux", "x86_64"),
           "aarch64-unknown-linux-gnu": ("Linux", "aarch64", "linux", "aarch64"),
           "aarch64-apple-darwin": ("Darwin", "arm64", "macos", "aarch64")}


def run(args, *, cwd, env=None, timeout=None):
    try:
        result = subprocess.run(args, cwd=cwd, env=env, text=True,
                                capture_output=True, timeout=timeout, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ReleaseError(f"could not run {args[0]}: {exc}") from exc
    if result.returncode:
        raise ReleaseError(f"{' '.join(map(str, args))} failed ({result.returncode}):\n"
                           f"{result.stdout}{result.stderr}")
    return result.stdout.strip()


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def read_toml(path):
    with path.open("rb") as stream:
        return tomllib.load(stream)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def build_environment():
    # Do not inherit credentials, DATABASE_URL, MIKE_*, VITE_*, Rust overrides,
    # build-cache identity suppression, or other shell/project configuration.
    env = {key: os.environ[key] for key in ("HOME", "PATH", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME")
           if key in os.environ}
    env.update(SQLX_OFFLINE="true", CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="1",
               CARGO_INCREMENTAL="0", RUSTUP_AUTO_INSTALL="0",
               COREPACK_ENABLE_NETWORK="0", PNPM_MANAGE_PACKAGE_MANAGER_VERSIONS="false",
               CI="true", LC_ALL="C", LANG="C",
               npm_config_manage_package_manager_versions="false")
    return env


def source_git(root, *arguments):
    """Read only the selected checkout, independent of ambient Git overrides."""
    root = Path(root).resolve()
    env = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_NO_REPLACE_OBJECTS="1", GIT_NO_LAZY_FETCH="1",
               GIT_OPTIONAL_LOCKS="0", GIT_TERMINAL_PROMPT="0", LC_ALL="C", LANG="C")
    command = ["git", "-c", "core.fsmonitor=false", "-c", "core.pager=cat",
               "-c", "protocol.allow=never"]
    selected = run([*command, "rev-parse", "--show-toplevel"], cwd=root, env=env)
    if Path(selected).resolve() != root:
        raise ReleaseError("source Git worktree differs from the requested checkout root")
    return run([*command, *arguments], cwd=root, env=env)


def clean_source(root, expected_sha=None):
    from source_inputs import verify_source_inputs

    entries = source_git(root, "ls-files", "-v", "-z").split("\0")
    if any(entry and (entry[0] == "S" or entry[0].islower()) for entry in entries):
        raise ReleaseError("source index hides tracked inputs; use a complete checkout without "
                           "skip-worktree or assume-unchanged entries")
    status = source_git(root, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        raise ReleaseError("source tree is not clean; commit reviewed changes first:\n" + status)
    commit = source_git(root, "rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40,64}", commit):
        raise ReleaseError("could not identify the source commit")
    if expected_sha is not None and commit != expected_sha:
        raise ReleaseError("source commit changed during preparation")
    verify_source_inputs(root)
    return commit


def workspace_version(root, requested):
    version = read_toml(root / "Cargo.toml")["workspace"]["package"]["version"]
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
                        r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        raise ReleaseError("workspace version is not a supported SemVer")
    if requested is not None and requested != version:
        raise ReleaseError(f"requested version {requested} differs from reviewed workspace version "
                           f"{version}; make and review a version commit first")
    packages = read_toml(root / "Cargo.lock")["package"]
    for name in ("mike", "mike-mcp"):
        locked = [p["version"] for p in packages if p["name"] == name]
        declared = read_toml(root / "crates" / name / "Cargo.toml")["package"]["version"]
        if locked != [version] or declared != {"workspace": True}:
            raise ReleaseError(f"{name} must inherit the workspace version and match Cargo.lock")
    return version


def native_target(root, env):
    system = run(["uname", "-s"], cwd=root, env=env)
    machine = run(["uname", "-m"], cwd=root, env=env)
    for target, host in TARGETS.items():
        if host[:2] == (system, machine):
            return target
    raise ReleaseError(f"unsupported native host {system}/{machine}; no cross-compilation")


def toolchain(root, env, target):
    rust = run(["rustc", "-vV"], cwd=root, env=env)
    fields = dict(line.split(": ", 1) for line in rust.splitlines() if ": " in line)
    pin = read_toml(root / "rust-toolchain.toml")["toolchain"]["channel"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", pin):
        raise ReleaseError("rust-toolchain.toml must pin an exact stable Rust version")
    if fields.get("release") != pin or fields.get("host") != target:
        raise ReleaseError(f"need preinstalled Rust {pin} for native target {target}")
    cargo = run(["cargo", "--version"], cwd=root, env=env)
    if cargo.split()[:2] != ["cargo", pin]:
        raise ReleaseError(f"need preinstalled Cargo {pin} from the pinned Rust toolchain")
    node = run(["node", "--version"], cwd=root, env=env)
    node_pin = (root / ".nvmrc").read_text().strip().lstrip("v")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", node_pin):
        raise ReleaseError(".nvmrc must pin an exact Node version")
    if node.lstrip("v") != node_pin:
        raise ReleaseError(f"need preinstalled Node {node_pin}, found {node}")
    package_manager = json.loads((root / "frontend/package.json").read_text())["packageManager"]
    pnpm = run(["pnpm", "--version"], cwd=root / "frontend", env=env)
    if package_manager != f"pnpm@{pnpm}":
        raise ReleaseError(f"need preinstalled {package_manager}; found pnpm@{pnpm}")
    return {"rustc": rust, "cargo": cargo, "node": node, "node_declared_pin": node_pin,
            "pnpm": pnpm, "python": sys.version.split()[0]}


def regular_file(path):
    if path.is_symlink() or not path.is_file():
        raise ReleaseError(f"expected a regular, non-symlink file: {path}")


def spa_inventory(root):
    build = root / "frontend/build"
    regular_file(build / "index.html")
    if not (build / "index.html").stat().st_size:
        raise ReleaseError("SPA index.html is empty")
    files = []
    for path in sorted(build.rglob("*")):
        if path.is_symlink():
            raise ReleaseError(f"SPA contains a symlink: {path}")
        if path.is_file():
            relative = path.relative_to(build).as_posix()
            if path.name.startswith(".env") or path.suffix in (".pem", ".key"):
                raise ReleaseError(f"SPA contains a private-file candidate: {relative}")
            files.append({"path": relative, "sha256": sha256(path), "size": path.stat().st_size})
    if not any(item["path"].endswith(".js") for item in files):
        raise ReleaseError("SPA has no JavaScript assets")
    return files
