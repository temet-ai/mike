"""Bounded, offline source/package/SPA pins for an operator notice review.

Lockfile membership is conservative input inventory, not proof of linked code or
redistribution rights. Package-record hashes pin provenance metadata, not package
payloads. Human review must resolve actual shipped applicability and upstreams.
"""

import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tomllib

from common import ReleaseError, TARGETS, clean_source, source_git

MAX_ENTRIES = 20000
MAX_INPUT_BYTES = 16 * 1024**2
MAX_ASSET_BYTES = 64 * 1024**2
HEX = re.compile(r"[0-9a-f]{64}")


def require(value, reason):
    if not value:
        raise ReleaseError("third-party notices: " + reason)


def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def text_field(value, limit=4096, empty=False):
    return (isinstance(value, str) and len(value) <= limit and (empty or bool(value.strip()))
            and not any(ord(character) < 32 or 127 <= ord(character) < 160
                        or 0xD800 <= ord(character) <= 0xDFFF for character in value))


def safe_relative(value):
    require(isinstance(value, str) and 0 < len(value) <= 4096 and "\\" not in value,
            "invalid relative input path")
    parts = value.split("/")
    require(len(parts) <= 32 and all(text_field(p, 255) and p not in (".", "..", ".git") for p in parts),
            "unsafe relative input path")
    return parts


def open_directory(path):
    require(".." not in Path(path).parts, "input path must not traverse parent components")
    path = Path(os.path.abspath(path))
    descriptor = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in path.parts[1:]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except OSError as error:
        os.close(descriptor)
        raise ReleaseError("third-party notices: directory missing, symlinked or unreadable") from error


def read_bounded(path, limit):
    """Walk from / without following any symlink; reject raced or special inputs."""
    path = Path(path)
    descriptor = open_directory(path.parent)
    try:
        opened = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptor)
        with os.fdopen(opened, "rb") as stream:
            before = os.fstat(stream.fileno())
            require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1,
                    "input must be a regular file without hard links")
            require(before.st_size <= limit, "input byte limit exceeded")
            data = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
            require(len(data) == before.st_size <= limit and
                    (before.st_size, before.st_mtime_ns, before.st_ctime_ns) ==
                    (after.st_size, after.st_mtime_ns, after.st_ctime_ns), "input changed while reading")
            return data
    except OSError as error:
        raise ReleaseError("third-party notices: input missing, symlinked, special or unreadable") from error
    finally:
        os.close(descriptor)


def read_json(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON key")
            result[key] = value
        return result
    try:
        return json.loads(data.decode("utf-8"), object_pairs_hook=unique,
                          parse_constant=lambda value: require(False, "nonfinite JSON number"))
    except (ValueError, UnicodeError, RecursionError) as error:
        raise ReleaseError("third-party notices: invalid JSON input") from error


def checked_assets(assets):
    require(isinstance(assets, list) and 0 < len(assets) <= MAX_ENTRIES, "missing or excessive SPA inventory")
    paths = []
    for item in assets:
        require(isinstance(item, dict) and set(item) == {"path", "sha256", "size"}, "invalid SPA inventory")
        safe_relative(item["path"])
        require(isinstance(item["sha256"], str) and HEX.fullmatch(item["sha256"])
                and type(item["size"]) is int and 0 <= item["size"] <= MAX_ASSET_BYTES,
                "invalid SPA inventory pins")
        paths.append(item["path"])
    require(paths == sorted(set(paths)), "SPA inventory must be complete, unique and sorted")
    return assets


def subject(kind, name, version, source, content_sha256, size=None):
    value = {"kind": kind, "name": name, "version": version, "source": source,
             "content_sha256": content_sha256, "size": size,
             "hash_scope": "spa_file_bytes" if kind == "spa_asset" else "package_lock_record"}
    return dict(value, id=digest(canonical(value)))


def cargo_subjects(data, commit):
    try:
        packages = tomllib.loads(data.decode("utf-8"))["package"]
        require(isinstance(packages, list) and 0 < len(packages) <= MAX_ENTRIES, "missing Cargo package inventory")
        result, seen = [], set()
        for package in packages:
            require(isinstance(package, dict) and text_field(package.get("name"), 256)
                    and text_field(package.get("version"), 256), "invalid Cargo package identity")
            source = package.get("source", "source:" + commit)
            require(text_field(source), "invalid Cargo package source")
            checksum = package.get("checksum")
            require(checksum is None or isinstance(checksum, str) and HEX.fullmatch(checksum),
                    "invalid Cargo package checksum")
            identity = (package["name"], package["version"], source)
            require(identity not in seen, "duplicate Cargo package identity")
            seen.add(identity)
            result.append(subject("cargo_package", package["name"], package["version"], source,
                                  digest(canonical(package))))
        return result
    except (ValueError, UnicodeError, KeyError, TypeError) as error:
        raise ReleaseError("third-party notices: unsupported Cargo lockfile") from error


def npm_subjects(data):
    """Enumerate pnpm v9 package stanzas only; unsupported key syntax fails closed."""
    try:
        text = data.decode("utf-8")
    except UnicodeError as error:
        raise ReleaseError("third-party notices: invalid pnpm lockfile encoding") from error
    require(len(re.findall(r"(?m)^lockfileVersion: ['\"]?9\.0['\"]?\s*$", text)) == 1,
            "unsupported pnpm lockfile version")
    require(len(re.findall(r"(?m)^packages:$", text)) == 1, "missing or duplicate pnpm package inventory")
    lines = text.splitlines(keepends=True)
    start = next(i + 1 for i, line in enumerate(lines) if line.rstrip("\r\n") == "packages:")
    records, key, block = [], None, []
    for line in lines[start:]:
        if line.strip() and not line[0].isspace():
            require(re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*:\s*", line),
                    "unsupported pnpm package section boundary")
            break
        if line.startswith("  ") and not line.startswith("   ") and line.strip():
            match = re.fullmatch(r"  (?:'([^']+)'|\"([^\"]+)\"|([^:'\"]+)):\s*", line)
            require(match, "unsupported pnpm package key")
            if key is not None:
                records.append((key, "".join(block).encode()))
            key = next(value for value in match.groups() if value is not None)
            block = []
        elif line.strip():
            require(key is not None and line.startswith("    "), "unsupported pnpm package body")
        if key is not None:
            block.append(line)
        require(len(records) <= MAX_ENTRIES, "pnpm package limit exceeded")
    if key is not None:
        records.append((key, "".join(block).encode()))
    require(records, "missing pnpm package inventory")
    result, seen = [], set()
    for key, block in records:
        name, separator, version = key.rpartition("@")
        require(separator and re.fullmatch(r"(?:@[A-Za-z0-9_.-]+/)?[A-Za-z0-9_.-]+", name)
                and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.+-]+)?", version),
                "unsupported pnpm package identity")
        require(key not in seen, "duplicate pnpm package identity")
        seen.add(key)
        result.append(subject("npm_package", name, version, "frontend/pnpm-lock.yaml#packages/" + key,
                              digest(block)))
    return result


def validate_inventory(inventory):
    require(isinstance(inventory, list) and 0 < len(inventory) <= MAX_ENTRIES, "missing or excessive review inventory")
    identifiers, identities, kinds = [], set(), set()
    for item in inventory:
        require(isinstance(item, dict) and set(item) == {
            "id", "kind", "name", "version", "source", "content_sha256", "size", "hash_scope"}, "invalid review inventory entry")
        kind = item["kind"]
        require(kind in ("cargo_package", "npm_package", "spa_asset") and text_field(item["name"])
                and text_field(item["source"]) and text_field(item["version"], 256, empty=kind == "spa_asset")
                and isinstance(item["content_sha256"], str) and HEX.fullmatch(item["content_sha256"]),
                "invalid review inventory provenance")
        require(item["hash_scope"] == ("spa_file_bytes" if kind == "spa_asset" else "package_lock_record"),
                "invalid inventory hash scope")
        require(item["size"] is None if kind != "spa_asset" else
                type(item["size"]) is int and 0 <= item["size"] <= MAX_ASSET_BYTES, "invalid inventory size")
        if kind == "spa_asset":
            safe_relative(item["name"])
            require(item["version"] == "" and item["source"] == "frontend/build/" + item["name"],
                    "invalid SPA provenance")
        require(item["id"] == digest(canonical({k: v for k, v in item.items() if k != "id"})),
                "review inventory identity mismatch")
        identity = (kind, item["name"], item["version"], item["source"])
        require(identity not in identities, "duplicate inventory subject")
        identities.add(identity)
        identifiers.append(item["id"])
        kinds.add(kind)
    require(identifiers == sorted(set(identifiers)), "review inventory must be unique and sorted")
    require(kinds == {"cargo_package", "npm_package", "spa_asset"}, "incomplete review inventory categories")
    return inventory


def binding(source_commit, version, target, cargo_sha, pnpm_sha, assets):
    require(isinstance(source_commit, str) and re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", source_commit)
            and text_field(version, 256) and re.fullmatch(r"[0-9A-Za-z.+-]+", version)
            and isinstance(target, str) and target in TARGETS and all(isinstance(s, str) and HEX.fullmatch(s) for s in (cargo_sha, pnpm_sha)),
            "invalid source, version, target or lockfile binding")
    return {"source_commit": source_commit, "version": version, "target": target,
            "cargo_lock_sha256": cargo_sha, "pnpm_lock_sha256": pnpm_sha,
            "spa_inventory_sha256": digest(canonical(checked_assets(assets)))}


def tracked_bytes(root, commit, path):
    record = source_git(root, "ls-tree", "-z", commit, "--", path).rstrip("\0")
    try:
        metadata, observed_path = record.split("\t")
        mode, kind, expected = metadata.split(" ")
    except ValueError as error:
        raise ReleaseError("third-party notices: missing tracked lockfile") from error
    require(mode in ("100644", "100755") and kind == "blob" and observed_path == path,
            "lockfile must be a tracked regular file")
    data = read_bounded(Path(root) / path, MAX_INPUT_BYTES)
    algorithm = "sha1" if len(expected) == 40 else "sha256"
    actual = hashlib.new(algorithm, b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    require(actual == expected, "lockfile differs from candidate source")
    return data


def source_inventory(root, source_commit, version, target, assets):
    clean_source(root, source_commit)
    checked_assets(assets)
    cargo = tracked_bytes(root, source_commit, "Cargo.lock")
    npm = tracked_bytes(root, source_commit, "frontend/pnpm-lock.yaml")
    actual_paths = []
    for count, path in enumerate((Path(root) / "frontend/build").rglob("*"), 1):
        require(count <= MAX_ENTRIES, "SPA inventory traversal limit exceeded")
        require(not path.is_symlink(), "symlink in SPA inventory")
        if path.is_dir():
            continue
        relative = path.relative_to(Path(root) / "frontend/build").as_posix()
        data = read_bounded(path, MAX_ASSET_BYTES)
        actual_paths.append({"path": relative, "sha256": digest(data), "size": len(data)})
        require(len(actual_paths) <= MAX_ENTRIES, "SPA inventory limit exceeded")
    actual_paths.sort(key=lambda item: item["path"])
    require(actual_paths == assets, "SPA inventory changed or is incomplete")
    inventory = cargo_subjects(cargo, source_commit) + npm_subjects(npm)
    inventory.extend(subject("spa_asset", item["path"], "", "frontend/build/" + item["path"],
                             item["sha256"], item["size"]) for item in assets)
    inventory.sort(key=lambda item: item["id"])
    validate_inventory(inventory)
    pins = binding(source_commit, version, target, digest(cargo), digest(npm), assets)
    clean_source(root, source_commit)
    return pins, inventory
