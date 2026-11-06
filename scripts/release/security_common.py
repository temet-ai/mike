"""Bounded, value-redacted release review primitives (no network or execution)."""

import hashlib
import json
import os
from contextlib import contextmanager
from pathlib import PurePosixPath
import re
import stat
import subprocess


class ReviewError(Exception):
    """Only constant error codes belong in this exception, never inspected input."""


LIMITS = {"file_bytes": 256 * 1024 * 1024, "total_bytes": 2 * 1024 * 1024 * 1024,
          "entries": 50000, "history_objects": 20000, "resource_refs": 1000,
          "archive_metadata_read_bytes": 1024 * 1024}
EXTENSIONS = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".ico", ".woff",
              ".woff2", ".ttf", ".otf", ".js", ".css", ".html", ".map", ".json",
              ".jsonl", ".log", ".sql", ".db", ".sqlite", ".zip", ".gz", ".tar",
              ".toml", ".yaml", ".yml", ".md", ".txt", ".pem", ".key"}
PATTERNS = {
    "private_key": rb"-----BEGIN (?:[A-Z0-9 ]{0,32})PRIVATE KEY-----",
    "api_token": rb"(?:sk-(?:ant-|proj-)?[a-zA-Z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[A-Z0-9]{16})",
    "credential_assignment": rb"(?i)(?:password|passwd|api[_-]?key|access[_-]?token|secret|authorization)\s*[\"']?\s*[:=]\s*[\"']?[^\s\"'`,;}{]{4,}",
    "cookie_header": rb"(?i)(?:set-cookie|cookie)\s*[\"']?\s*[:=]\s*[^\r\n]{3,}",
    "database_url": rb"(?i)(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis)://[^\s\"'<>]{3,}",
    "absolute_private_path": rb"(?:/(?:home|Users|root|mnt|Volumes|workspace)/[^\s\"'<>]{1,}|[A-Za-z]:\\(?:Users|Documents and Settings)\\[^\r\n\"']{1,})",
    "jwt_candidate": rb"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
    "conversation_structure": rb"(?i)[\"'](?:role|messages|prompt|completion)[\"']\s*:\s*(?:[\"'](?:user|assistant)|\[)",
    "source_map_reference": rb"(?i)sourceMappingURL\s*=",
    "embedded_spa_marker": rb"(?i)(?:<!doctype html|<script[\s>]|/_app/immutable/)",
    "lfs_pointer_marker": rb"version (?:https://(?:git-lfs|hawser)\.github\.com/spec/v1|http://git-media\.io/v/2)",
}
COMPILED = {key: re.compile(value) for key, value in PATTERNS.items()}
RESOURCE = re.compile(rb"(?<![A-Za-z0-9_/@.+-])[A-Za-z0-9_/@.+-]{1,240}\.(?:woff2?|ttf|otf|png|jpe?g|svg|webp|gif|ico|js|css|map)(?:\b|$)")
# The version must be the first complete line of an actual pointer. Preserve a
# possible payload gap even if later fields are malformed, truncated or oversized;
# this deliberately recognizes candidates, not validated pointer documents.
LFS_HEADER_BYTES = 128
LFS_POINTER_HEADER = re.compile(rb"\A" + PATTERNS["lfs_pointer_marker"] + rb"(?:\r?\n|\Z)")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def reference(value):
    if isinstance(value, str):
        value = value.encode("utf-8", "surrogateescape")
    return "sha256:" + digest(value)


def safe_relative(name):
    if not isinstance(name, str) or not name or "\x00" in name or "\\" in name:
        return False
    path = PurePosixPath(name)
    return (not path.is_absolute() and not re.match(r"^[A-Za-z]:", name)
            and all(part.lower() not in ("", ".", "..", ".git") for part in name.split("/")))


def filename_categories(name):
    lower = name.lower()
    base = PurePosixPath(lower).name
    categories = set()
    if base.startswith(".env") or re.search(r"(?:cookie|credential|secret|token|id_rsa|id_ed25519)", lower):
        categories.add("private_filename")
    if re.search(r"(?:capture|conversation|transcript|session|screenshot|dump)", lower):
        categories.add("private_material_filename")
    extension = PurePosixPath(lower).suffix
    if extension in {".jsonl", ".log", ".sql", ".db", ".sqlite"}:
        categories.add("capture_log_database_candidate")
    if extension in {".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg"}:
        categories.add("image_manual_review")
    if extension in {".woff", ".woff2", ".ttf", ".otf"}:
        categories.add("font_rights_review")
    if extension == ".map":
        categories.add("source_map_review")
    if extension in {".pem", ".key", ".p12", ".pfx"}:
        categories.add("key_material_candidate")
    if extension in {".zip", ".pyz", ".pyzw", ".gz", ".tar", ".xz", ".bz2", ".7z", ".br"}:
        categories.add("nested_container_not_decoded")
    if re.search(r"(?:^|/)(?:license|licence|notice|copying|ofl)(?:[./_-]|$)", lower):
        categories.add("license_notice")
    categories.update(key for key, regex in COMPILED.items() if regex.search(name.encode("utf-8", "surrogateescape")))
    return sorted(categories)


def file_identity(name, scope):
    suffix = PurePosixPath(name.lower()).suffix
    return {"scope": scope, "path_ref": reference(name),
            "extension": suffix if suffix in EXTENSIONS else "other",
            "filename_categories": filename_categories(name)}


@contextmanager
def open_regular(root, name):
    """Open beneath the root with no-follow descriptors, including parent races."""
    if not safe_relative(name):
        raise ReviewError("unsafe_relative_path")
    descriptors = []
    try:
        directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
        directory = os.open(root, directory_flags)
        descriptors.append(directory)
        for part in name.split("/")[:-1]:
            directory = os.open(part, directory_flags, dir_fd=directory)
            descriptors.append(directory)
        descriptor = os.open(name.split("/")[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                             dir_fd=directory)
        descriptors.append(descriptor)
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ReviewError("non_regular_file")
        with os.fdopen(os.dup(descriptor), "rb") as stream:
            yield stream
    except OSError as exc:
        raise ReviewError("file_unavailable_or_symlink_not_followed") from exc
    finally:
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def git(root, *args, input_data=None):
    env = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT") if key in os.environ}
    env.update(GIT_NO_LAZY_FETCH="1", GIT_TERMINAL_PROMPT="0", GIT_CONFIG_NOSYSTEM="1",
               GIT_CONFIG_GLOBAL=os.devnull, GIT_OPTIONAL_LOCKS="0", GIT_NO_REPLACE_OBJECTS="1", LC_ALL="C")
    try:
        result = subprocess.run(["git", "-c", "core.fsmonitor=false", "-c", "core.pager=cat",
                                 "-c", "protocol.allow=never", *args],
                                cwd=root, env=env, capture_output=True, timeout=120, check=False,
                                input=input_data)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ReviewError("git_unavailable_or_timeout") from exc
    if result.returncode:
        raise ReviewError("git_command_incomplete")
    return result.stdout


def source_commit(root, requested="HEAD"):
    if requested != "HEAD" and not re.fullmatch(r"[a-f0-9]{40}|[a-f0-9]{64}", requested):
        raise ReviewError("source_commit_must_be_full_sha")
    value = git(root, "rev-parse", "--verify", requested + "^{commit}").strip().decode("ascii")
    if not re.fullmatch(r"[a-f0-9]{40}|[a-f0-9]{64}", value):
        raise ReviewError("invalid_source_commit")
    return value


def write_private_json(path, value):
    """Refuse clobbering, including symlinks, and create evidence with mode 0600."""
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(value, output, indent=2, sort_keys=True, ensure_ascii=True)
            output.write("\n")
    except OSError as exc:
        raise ReviewError("output_unavailable_or_already_exists") from exc


class Scanner:
    def __init__(self, limits=None):
        self.limits = dict(LIMITS, **(limits or {}))
        self.total = 0
        self.inventory = []
        self.gaps = []

    def gap(self, code, scope, path_ref=None):
        entry = {"code": code, "scope": scope}
        if path_ref:
            entry["path_ref"] = path_ref
        self.gaps.append(entry)

    def scan(self, stream, size, name, scope, **extra):
        record = dict(file_identity(name, scope), size=size, **extra)
        if len(self.inventory) >= self.limits["entries"]:
            self.gap("entry_limit", scope)
            return None
        self.inventory.append(record)
        if size > self.limits["file_bytes"] or self.total + size > self.limits["total_bytes"]:
            record["status"] = "NOT_SCANNED"
            self.gap("byte_limit", scope, record["path_ref"])
            return record
        hashed, tail, observed = hashlib.sha256(), b"", 0
        pointer_prefix = bytearray()
        categories, string_categories, resources = set(), set(), set()
        while observed < size:
            block = stream.read(min(65536, size - observed))
            if not block:
                break
            hashed.update(block)
            observed += len(block)
            pointer_prefix.extend(block[:max(0, LFS_HEADER_BYTES - len(pointer_prefix))])
            window = tail + block
            for category, regex in COMPILED.items():
                if regex.search(window):
                    categories.add(category)
            # Printable ASCII and UTF-16 strings are inspected without emitting values.
            for encoding in ("ascii", "utf-16-le", "utf-16-be"):
                for alignment in range(2 if encoding != "ascii" else 1):
                    text = window[alignment:].decode(encoding, errors="replace").encode("ascii", errors="replace")
                    for category, regex in COMPILED.items():
                        if regex.search(text):
                            string_categories.add(category)
                    for match in RESOURCE.finditer(text):
                        resource_ref = reference(match.group())
                        if resource_ref in resources or len(resources) < self.limits["resource_refs"]:
                            resources.add(resource_ref)
                        else:
                            self.gap_once("embedded_resource_reference_limit", scope, record["path_ref"])
            tail = window[-2048:]
        self.total += observed
        if LFS_POINTER_HEADER.match(pointer_prefix):
            categories.add("lfs_pointer_payload_not_scanned")
        record.update(sha256=hashed.hexdigest(), scanned_bytes=observed,
                      byte_categories=sorted(categories), string_categories=sorted(string_categories),
                      embedded_resource_refs=sorted(resources),
                      status="SCANNED" if observed == size else "INCOMPLETE")
        if observed != size:
            self.gap("short_read", scope, record["path_ref"])
        if "lfs_pointer_payload_not_scanned" in categories:
            self.gap("lfs_pointer_payload_not_scanned", scope, record["path_ref"])
        return record

    def gap_once(self, code, scope, path_ref):
        entry = {"code": code, "scope": scope, "path_ref": path_ref}
        if entry not in self.gaps:
            self.gaps.append(entry)
