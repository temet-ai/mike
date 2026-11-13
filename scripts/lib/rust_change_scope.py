#!/usr/bin/env python3
"""Package selection shared by the landing gate and focused Rust test runner."""

import argparse
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys


def metadata(root):
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--offline", "--locked", "--format-version", "1"],
        cwd=root, check=True, capture_output=True, text=True,
    )
    data = json.loads(result.stdout)
    members = set(data["workspace_members"])
    return [p for p in data["packages"] if p["id"] in members]


def package_roots(packages):
    return {Path(p["manifest_path"]).resolve().parent: p for p in packages}


def owner(root, path, packages):
    absolute = root / path
    matches = [(directory, p) for directory, p in package_roots(packages).items()
               if absolute.is_relative_to(directory)]
    return max(matches, key=lambda pair: len(pair[0].parts))[1] if matches else None


def global_change(path):
    return path in {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"} or path.startswith(".cargo/")


def rust_input(path):
    return global_change(path) or path.startswith(".sqlx/") or path.endswith((".rs", ".sql", "/Cargo.toml"))


def changed_packages(root, paths, packages):
    selected = set()
    for path in paths:
        if global_change(path):
            return None
        if path.startswith(".sqlx/"):
            if not any(p["name"] == "mike" for p in packages):
                return None
            selected.add("mike")
            continue
        package = owner(root, path, packages)
        if package is not None:
            if rust_input(path) or not path.endswith(".md"):
                selected.add(package["name"])
        elif rust_input(path):
            return None
    return selected


def affected_args(root, paths, packages):
    selected = changed_packages(root, paths, packages)
    if selected is None:
        return ["--workspace"]
    roots = package_roots(packages)
    while True:
        added = {
            package["name"] for package in packages
            if any(roots.get(Path(dep["path"]).resolve(), {}).get("name") in selected
                   for dep in package["dependencies"] if dep.get("path"))
        }
        if added <= selected:
            break
        selected |= added
    return [arg for name in sorted(selected) for arg in ("-p", name)]


# crate mike and the crates split from it keep module paths (docs/adr/0002)
SPLIT_CRATES = ("mike", "mike-base", "mike-features")


def path_area(root, path, package):
    directory = Path(package["manifest_path"]).resolve().parent
    relative = (root / path).relative_to(directory).as_posix()
    if relative.startswith("tests/"):
        for target in package["targets"]:
            if "test" not in target["kind"]:
                continue
            source = Path(target["src_path"]).resolve()
            if root / path == source or (root / path).is_relative_to(source.with_suffix("")):
                return ["--test", target["name"]], f"binary(={target['name']})"
        return ["--tests"], f"package(={package['name']})"
    expression = f"package(={package['name']})"
    if package["name"] in SPLIT_CRATES and relative.startswith("src/") and relative.endswith(".rs"):
        components = relative[4:-3].split("/")
        if components[-1] == "mod":
            components.pop()
        if components not in (["main"], ["lib"]):
            module = "::".join(components[:2])
            expression += f" & test(/^{re.escape(module)}(::|$)/)"
    return ["--tests"], expression


def area_commands(root, paths, packages):
    prefix = [str(root / "scripts/cx"), "nextest", "run"]
    area_paths = [path for path in paths if not global_change(path)
                  and not path.endswith("Cargo.toml") and not path.startswith(".sqlx/")]
    selected = changed_packages(root, area_paths, packages)
    suffix = ["--ignore-default-filter", "--no-fail-fast", "--no-tests=pass"]
    if selected is None:
        return [prefix + ["--workspace", "--tests", "--no-fail-fast"]]
    if not selected and any(rust_input(path) for path in paths):
        return [prefix + affected_args(root, paths, packages) + ["--tests", "--no-fail-fast"]]
    groups = {}
    for path in area_paths:
        if path.endswith(".md"):
            continue
        package = owner(root, path, packages)
        if package is not None and package["name"] in selected:
            targets, expression = path_area(root, path, package)
        else:
            continue
        group = groups.setdefault(package["name"], {"targets": [], "filters": set()})
        # Keep each target flag/value together; an identical target need only build once.
        index = 0
        while index < len(targets):
            target = targets[index:index + (1 if targets[index] == "--tests" else 2)]
            if target not in group["targets"]:
                group["targets"].append(target)
            index += len(target)
        group["filters"].add(expression)
    if any(path.startswith("crates/mike/src/api/manifest") for path in area_paths) \
            and any(p["name"] == "mike-mcp" for p in packages):
        # mike-mcp checks that every inspect route in mike's manifest has MCP coverage.
        group = groups.setdefault("mike-mcp", {"targets": [], "filters": set()})
        if ["--test", "manifest_coverage"] not in group["targets"]:
            group["targets"].append(["--test", "manifest_coverage"])
        group["filters"].add("binary(=manifest_coverage)")
    return [
        prefix + ["-p", name] + [arg for target in group["targets"] for arg in target]
        + suffix + ["-E", " | ".join(f"({expression})" for expression in sorted(group["filters"]))]
        for name, group in sorted(groups.items())
    ]


def git_paths(root, base):
    commands = [
        ["git", "diff", "--name-only", "--no-renames", f"{base}...HEAD"],
        ["git", "diff", "--name-only", "--no-renames", "HEAD"],
        ["git", "ls-files", "--others", "--exclude-standard"],
    ]
    paths = set()
    for command in commands:
        result = subprocess.run(command, cwd=root, check=True, capture_output=True, text=True)
        paths.update(result.stdout.splitlines())
    return sorted(paths)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["packages", "area"])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--print", action="store_true", dest="print_filter")
    parser.add_argument("--print-command", action="store_true")
    parser.add_argument("--skip-default-checks", action="store_true",
                        help="the landing gate already ran the default-profile checks")
    parser.add_argument("base", nargs="?", default="origin/main")
    args = parser.parse_args()
    root = args.root.resolve()
    paths = sys.stdin.read().splitlines() if args.mode == "packages" else git_paths(root, args.base)
    try:
        packages = metadata(root)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError) as error:
        print(f"rust-change-scope: metadata unavailable; checking the whole workspace ({error})", file=sys.stderr)
        packages = []
        paths = ["Cargo.toml"]
    if args.mode == "packages":
        print(" ".join(affected_args(root, paths, packages)))
        return 0
    commands = area_commands(root, paths, packages)
    if args.skip_default_checks:
        commands = [command for command in commands if "--ignore-default-filter" in command]
    if not commands:
        print(f"test-area: no Rust area changed against {args.base}")
        return 0
    if args.print_filter or args.print_command:
        for command in commands:
            expression = command[-1] if "-E" in command else "default()"
            print(shlex.join(command) if args.print_command else expression)
        return 0
    status = 0
    for command in commands:
        print(f"test-area: {shlex.join(command)}", flush=True)
        result = subprocess.run(command, cwd=root)
        if result.returncode:
            status = 1
    return status


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"rust-change-scope: {error}", file=sys.stderr)
        sys.exit(1)
