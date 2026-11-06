"""Read-only tracked tree, reachable Git object, and unextracted archive review."""

import bz2
from contextlib import nullcontext
import gzip
import io
import json
import lzma
import os
import stat
import tarfile
import zipfile

from security_common import (ReviewError, digest, file_identity, git, open_regular,
                             reference, safe_relative, source_commit)


def scan_tree(root, scanner):
    commit = source_commit(root)
    index_snapshot = git(root, "ls-files", "--stage", "-z")
    entries = index_snapshot.split(b"\0")
    seen, changed = set(), False
    for entry in entries:
        if not entry:
            continue
        header, raw_name = entry.split(b"\t", 1)
        mode, oid, stage = header.decode("ascii").split()
        name = os.fsdecode(raw_name)
        if name in seen or stage != "0":
            scanner.gap("unmerged_index", "tracked_tree", reference(name))
            continue
        seen.add(name)
        if len(seen) > scanner.limits["entries"]:
            scanner.gap("entry_limit", "tracked_tree")
            break
        if mode not in {"100644", "100755"}:
            code = "submodule_not_scanned" if mode == "160000" else "symlink_not_followed"
            scanner.gap(code, "tracked_tree", reference(name))
            scanner.inventory.append(dict(file_identity(name, "tracked_tree"), status="NOT_SCANNED", mode=mode))
            continue
        try:
            with open_regular(root, name) as stream:
                before = os.fstat(stream.fileno())
                scanner.scan(stream, before.st_size, name, "tracked_tree", index_object=oid, mode=mode)
                after = os.fstat(stream.fileno())
            if (before.st_mtime_ns, before.st_size, before.st_ino) != (after.st_mtime_ns, after.st_size, after.st_ino):
                scanner.gap("file_changed_during_scan", "tracked_tree", reference(name))
                changed = True
        except (ReviewError, OSError) as exc:
            scanner.gap(str(exc) if isinstance(exc, ReviewError) else "file_read_failed", "tracked_tree", reference(name))
    if source_commit(root) != commit:
        scanner.gap("head_changed_during_scan", "tracked_tree")
        changed = True
    if git(root, "ls-files", "--stage", "-z") != index_snapshot:
        scanner.gap("index_changed_during_scan", "tracked_tree")
        changed = True
    status = git(root, "status", "--porcelain=v1", "--untracked-files=no", "--ignore-submodules=all", "-z")
    snapshot = [item for item in scanner.inventory if item["scope"] == "tracked_tree"]
    return {"source_commit": commit, "tracked_paths": len(seen), "tracked_worktree_dirty": bool(status),
            "content_snapshot_sha256": digest(json.dumps(snapshot, sort_keys=True).encode()),
            "concurrent_change_observed": changed, "untracked_and_ignored_files": "NOT_SCANNED"}


def scan_history(root, scanner):
    """Inspect locally reachable objects only; never fetch missing/promisor objects."""
    result = {"status": "INCOMPLETE", "scope": "objects reachable from local refs (--all)",
              "remote_refs_not_fetched": True, "reflogs_and_unreachable_objects": "NOT_SCANNED",
              "shallow": git(root, "rev-parse", "--is-shallow-repository").strip() == b"true",
              "missing_objects": 0, "scanned_objects": 0}
    if result["shallow"]:
        scanner.gap("shallow_history", "history")
    try:
        ref_snapshot = git(root, "for-each-ref", "--format=%(refname) %(objectname)")
        result["ref_snapshot_sha256"] = digest(ref_snapshot)
        refs = git(root, "for-each-ref", "--format=%(refname)").splitlines()
        for name in refs[:scanner.limits["entries"]]:
            scanner.scan(io.BytesIO(name), len(name), os.fsdecode(name), "history_ref")
            if name.startswith(b"refs/replace/"):
                scanner.gap("replacement_refs_not_applied", "history")
        if len(refs) > scanner.limits["entries"]:
            scanner.gap("history_ref_limit", "history")
        raw_objects = git(root, "rev-list", "--objects", "--all", "--missing=print", "--no-object-names")
    except ReviewError:
        scanner.gap("history_enumeration_failed", "history")
        return result
    objects = raw_objects.splitlines()
    if len(objects) > scanner.limits["history_objects"]:
        scanner.gap("history_object_limit", "history")
        objects = objects[:scanner.limits["history_objects"]]
    available = [oid for oid in objects if not oid.startswith(b"?")]
    metadata = {}
    if available:
        try:
            lines = git(root, "cat-file", "--batch-check", input_data=b"\n".join(available) + b"\n").splitlines()
            metadata = {line.split()[0]: line.split()[1:] for line in lines}
        except ReviewError:
            scanner.gap("history_object_metadata_failed", "history")
            return result
    for raw_oid in objects:
        if raw_oid.startswith(b"?"):
            result["missing_objects"] += 1
            scanner.gap("missing_git_object", "history")
            continue
        oid = raw_oid.decode("ascii")
        # Names come from binary tree objects, avoiding rev-list's ambiguous path quoting.
        try:
            kind_raw, size_raw = metadata[raw_oid]
            kind, size = kind_raw.decode("ascii"), int(size_raw)
            if size > scanner.limits["file_bytes"] or scanner.total + size > scanner.limits["total_bytes"]:
                scanner.scan(io.BytesIO(), size, oid, "history", object_id=oid, object_type=kind)
                continue
            data = git(root, "cat-file", kind, oid)
            item = scanner.scan(io.BytesIO(data), size, oid, "history", object_id=oid, object_type=kind)
            if item is not None and item["status"] == "SCANNED":
                result["scanned_objects"] += 1
            if kind == "tree" and item:
                scan_tree_names(data, oid, scanner, len(oid) // 2)
        except (ReviewError, ValueError, UnicodeError, KeyError):
            result["missing_objects"] += 1
            scanner.gap("git_object_unavailable", "history", reference(oid))
    if git(root, "for-each-ref", "--format=%(refname) %(objectname)") != ref_snapshot:
        scanner.gap("history_refs_changed_during_scan", "history")
    if not any(gap["scope"] in ("history", "history_filename", "history_ref") for gap in scanner.gaps):
        result["status"] = "SCANNED_LOCAL_REACHABLE"
    return result


def scan_tree_names(data, tree_oid, scanner, oid_bytes):
    offset = 0
    while offset < len(data):
        try:
            end = data.index(b"\0", offset)
            header = data[offset:end]
            mode, raw_name = header.split(b" ", 1)
            if end + 1 + oid_bytes > len(data):
                raise ValueError
            name = os.fsdecode(raw_name)
            item = file_identity(name, "history_filename")
            if item["filename_categories"] or mode in (b"160000", b"120000"):
                if len(scanner.inventory) >= scanner.limits["entries"]:
                    scanner.gap("entry_limit", "history_filename")
                    return
                item.update(tree_object=tree_oid, status="NAME_SCANNED", mode=mode.decode("ascii"))
                scanner.inventory.append(item)
            if mode == b"160000":
                scanner.gap("historical_submodule_not_scanned", "history", reference(name))
            offset = end + 1 + oid_bytes
        except (ValueError, UnicodeError):
            scanner.gap("git_tree_parse_failed", "history", reference(tree_oid))
            return


def inspect_member(scanner, name, size, regular, stream_factory, seen, **metadata):
    name_ref = reference(name)
    if not safe_relative(name):
        scanner.gap("unsafe_archive_member_path", "archive", name_ref)
        return
    if name in seen:
        scanner.gap("duplicate_archive_member", "archive", name_ref)
        return
    seen.add(name)
    if not regular:
        scanner.gap("archive_link_or_special_file", "archive", name_ref)
        scanner.inventory.append(dict(file_identity(name, "archive_member"), status="NOT_SCANNED", **metadata))
        return
    if size > scanner.limits["file_bytes"] or scanner.total + size > scanner.limits["total_bytes"]:
        scanner.scan(io.BytesIO(), size, name, "archive_member", **metadata)
        return
    with stream_factory() as stream:
        item = scanner.scan(stream, size, name, "archive_member", **metadata)
    if item and "nested_container_not_decoded" in item["filename_categories"]:
        scanner.gap("nested_container_not_decoded", "archive", name_ref)


def scan_tar(source, scanner, archive_ref):
    seen = set()
    source.seek(0)
    magic = source.read(6)
    source.seek(0)
    decoded_stream = (gzip.GzipFile(fileobj=source) if magic.startswith(b"\x1f\x8b") else
                      bz2.BZ2File(source) if magic.startswith(b"BZh") else
                      lzma.LZMAFile(source) if magic.startswith(b"\xfd7zXZ") else nullcontext(source))
    with decoded_stream as decoded:
        bounded = TarInput(decoded, scanner)
        with tarfile.open(fileobj=bounded, mode="r:") as archive:
            scan_tar_members(archive, scanner, archive_ref, seen)
        # Consume the compression trailer (including its CRC) and reject concealed
        # nonzero bytes after the tar end marker. No trailing payload is approved.
        while True:
            trailing = bounded.read(65536)
            if not trailing:
                break
            if any(trailing):
                scanner.gap_once("archive_trailing_data", "archive", archive_ref)
                scanner.scan(io.BytesIO(trailing), len(trailing), "tar-trailer", "archive_trailer", archive_ref=archive_ref)


def scan_tar_members(archive, scanner, archive_ref, seen):
    for index, member in enumerate(archive):
        if index >= scanner.limits["entries"]:
            scanner.gap("archive_entry_limit", "archive", archive_ref)
            break
        if member.isdir():
            if not safe_relative(member.name.rstrip("/")):
                scanner.gap("unsafe_archive_member_path", "archive", reference(member.name))
            scanner.scan(io.BytesIO(), 0, member.name, "archive_directory", archive_ref=archive_ref)
            continue
        metadata = {"archive_ref": archive_ref, "mode": oct(member.mode & 0o7777)}
        # PAX, owner, group and link metadata can expose names or private values too.
        metadata_bytes = "\n".join([member.uname, member.gname, member.linkname,
                                     *[str(k) + "=" + str(v) for k, v in member.pax_headers.items()]]).encode()
        scanner.scan(io.BytesIO(metadata_bytes), len(metadata_bytes), member.name, "archive_metadata", **metadata)
        inspect_member(scanner, member.name, member.size, member.isreg(),
                       lambda: archive.extractfile(member), seen, **metadata)


def scan_zip(source, scanner, archive_ref):
    seen = set()
    source.seek(0)
    with zipfile.ZipFile(source) as archive:
        scanner.scan(io.BytesIO(archive.comment), len(archive.comment), "zip-comment", "archive_metadata", archive_ref=archive_ref)
        for index, member in enumerate(archive.infolist()):
            if index >= scanner.limits["entries"]:
                scanner.gap("archive_entry_limit", "archive", archive_ref)
                break
            if member.is_dir():
                if not safe_relative(member.filename.rstrip("/")):
                    scanner.gap("unsafe_archive_member_path", "archive", reference(member.filename))
                scanner.scan(io.BytesIO(), 0, member.filename, "archive_directory", archive_ref=archive_ref)
                continue
            mode = member.external_attr >> 16
            regular = stat.S_IFMT(mode) in (0, stat.S_IFREG) and not member.flag_bits & 1
            scanner.scan(io.BytesIO(member.comment + member.extra), len(member.comment + member.extra),
                         member.filename, "archive_metadata", archive_ref=archive_ref)
            inspect_member(scanner, member.filename, member.file_size, regular,
                           lambda: archive.open(member), seen, archive_ref=archive_ref,
                           mode=oct(mode & 0o7777))


def scan_archive(path, scanner):
    """Candidate explicitly supplied by the operator, never auto-discovered."""
    archive_ref = reference(path.name)
    start_gaps, start_inventory = len(scanner.gaps), len(scanner.inventory)
    with open_regular(path.parent, path.name) as stream:
        before = os.fstat(stream.fileno())
        item = scanner.scan(stream, before.st_size, path.name, "archive_bytes")
        if not item or item["status"] != "SCANNED":
            return {"archive_ref": archive_ref, "status": "INCOMPLETE"}
        try:
            stream.seek(0)
            if zipfile.is_zipfile(stream):
                scan_zip(stream, scanner, archive_ref)
            else:
                scan_tar(stream, scanner, archive_ref)
        except ReviewError as exc:
            scanner.gap(str(exc), "archive", archive_ref)
        except (tarfile.TarError, zipfile.BadZipFile, OSError, RuntimeError, EOFError, ValueError, NotImplementedError):
            scanner.gap("archive_decode_failed", "archive", archive_ref)
        after = os.fstat(stream.fileno())
    if (before.st_size, before.st_mtime_ns, before.st_ino) != (after.st_size, after.st_mtime_ns, after.st_ino):
        scanner.gap("archive_changed_during_scan", "archive", archive_ref)
    members = [entry for entry in scanner.inventory[start_inventory:] if entry["scope"] == "archive_member"]
    return {"archive_ref": archive_ref, "sha256": item["sha256"], "bytes": before.st_size,
            "status": "SCANNED" if len(scanner.gaps) == start_gaps else "CHECK_COVERAGE_GAPS",
            "notice_refs": [entry["path_ref"] for entry in members if "license_notice" in entry["filename_categories"]],
            "required_notices": "NOT_VERIFIED", "rights": "UNRESOLVED"}


class TarInput:
    """Bound extended headers and total decoded tar bytes; never seek backwards."""

    def __init__(self, stream, scanner):
        self.stream, self.scanner, self.position = stream, scanner, 0

    def read(self, size=-1):
        if size < 0 or size > self.scanner.limits["archive_metadata_read_bytes"]:
            raise ReviewError("archive_metadata_read_limit")
        if self.position + size > self.scanner.limits["total_bytes"]:
            raise ReviewError("archive_decoded_byte_limit")
        data = self.stream.read(size)
        self.position += len(data)
        return data

    def tell(self):
        return self.position

    def seek(self, offset, whence=0):
        target = self.position + offset if whence == 1 else offset
        if whence not in (0, 1) or target < self.position:
            raise ReviewError("archive_requires_unsupported_seek")
        while self.position < target:
            if not self.read(min(65536, target - self.position)):
                raise ReviewError("archive_truncated")
        return self.position
