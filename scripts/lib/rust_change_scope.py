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


def package_args(names):
    return [arg for name in sorted(names) for arg in ("-p", name)]


def lint_args(root, paths, packages):
    """The crates whose own files changed: only their source has anything new to lint."""
    selected = changed_packages(root, paths, packages)
    return ["--workspace"] if selected is None else package_args(selected)


def dependent_args(root, paths, packages):
    """The crates that depend on a changed crate, for a compile check without lints or tests."""
    selected = changed_packages(root, paths, packages)
    if not selected:
        return []
    affected = affected_args(root, paths, packages)
    return package_args(set(affected[1::2]) - selected)


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
    return package_args(selected)


sys.path.insert(0, str(Path(__file__).resolve().parent))
import split_crates  # noqa: E402

# crate mike and the crates split from it keep module paths (docs/adr/0002)
SPLIT_CRATES = split_crates.names()


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


# Words too common in path and test-module names to tie a lower crate's change to
# a test in crate mike.
GENERIC_WORDS = {
    "src", "mod", "lib", "main", "rs", "test", "tests", "store", "persistence", "api", "launch",
    "handler", "handlers", "cli", "db", "http", "worker", "session", "model", "type", "types",
    "util", "state", "repo", "run", "live", "support", "cross", "crate", "config", "service",
}


def words(name):
    return {word[:-1] if word.endswith("s") and len(word) > 3 else word
            for word in re.split(r"[_/.\-]", name) if word} - GENERIC_WORDS


def mike_tests_of(root, path, package, packages):
    """Tests in crate mike for a lower split crate's file: the integration modules and
    cross_crate_tests files whose names share a word with the file's path."""
    if package["name"] == "mike" or package["name"] not in SPLIT_CRATES:
        return None
    mike = next((p for p in packages if p["name"] == "mike"), None)
    if mike is None:
        return None
    directory = Path(package["manifest_path"]).resolve().parent
    relative = (root / path).relative_to(directory).as_posix()
    if not (relative.startswith("src/") and relative.endswith(".rs")):
        return None
    changed = words(relative[4:-3])
    mike_root = Path(mike["manifest_path"]).resolve().parent
    integration = sorted({entry.name.removesuffix(".rs")
                          for entry in (mike_root / "tests/integration").glob("*")
                          if words(entry.name.removesuffix(".rs")) & changed})
    cross = sorted({entry.stem for entry in mike_root.glob("src/**/cross_crate_tests/*.rs")
                    if entry.stem != "mod" and words(entry.stem) & changed})
    found = []
    if integration and any(t["name"] == "integration" for t in mike["targets"]):
        found.append((["--test", "integration"],
                      f"binary(=integration) & test(/^({'|'.join(integration)})::/)"))
    if cross:
        found.append((["--lib"], f"test(/(^|::)cross_crate_tests::({'|'.join(cross)})::/)"))
    return found


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
        for targets, expression in mike_tests_of(root, path, package, packages) or []:
            mike_group = groups.setdefault("mike", {"targets": [], "filters": set()})
            if targets not in mike_group["targets"] and ["--tests"] not in mike_group["targets"]:
                mike_group["targets"].append(targets)
            mike_group["filters"].add(expression)
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
    parser.add_argument("mode", choices=["packages", "dependents", "area"])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--print", action="store_true", dest="print_filter")
    parser.add_argument("--print-command", action="store_true")
    parser.add_argument("--skip-default-checks", action="store_true",
                        help="the landing gate already ran the default-profile checks")
    parser.add_argument("base", nargs="?", default="origin/main")
    args = parser.parse_args()
    root = args.root.resolve()
    from_stdin = args.mode in ("packages", "dependents")
    paths = sys.stdin.read().splitlines() if from_stdin else git_paths(root, args.base)
    try:
        packages = metadata(root)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError) as error:
        print(f"rust-change-scope: metadata unavailable; checking the whole workspace ({error})", file=sys.stderr)
        packages = []
        paths = ["Cargo.toml"]
    if args.mode == "packages":
        print(" ".join(lint_args(root, paths, packages)))
        return 0
    if args.mode == "dependents":
        print(" ".join(dependent_args(root, paths, packages)))
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
