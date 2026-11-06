"""Hash-pinned source-export proposal and validation. Never writes an export tree."""

import io
import json

from security_common import ReviewError, digest, git, reference, safe_relative, source_commit
from security_sources import scan_archive

REVIEW_KINDS = ("privacy", "rights")
APPROVAL_KINDS = ("source_owner", "legal", "distribution")


def tree_map(root, commit):
    result = {}
    for record in git(root, "ls-tree", "-r", "-z", "--full-tree", commit).split(b"\0"):
        if not record:
            continue
        info, raw_name = record.split(b"\t", 1)
        mode, kind, oid = info.decode("ascii").split()
        result[raw_name.decode("utf-8", "surrogateescape")] = (mode, kind, oid)
    return result


def source_file(root, name, tree, scanner):
    if not safe_relative(name) or name not in tree:
        raise ReviewError("export_path_unsafe_or_missing")
    mode, kind, oid = tree[name]
    if kind != "blob" or mode not in {"100644", "100755"}:
        raise ReviewError("export_requires_regular_blobs")
    size = int(git(root, "cat-file", "-s", oid))
    if size > scanner.limits["file_bytes"] or scanner.total + size > scanner.limits["total_bytes"]:
        raise ReviewError("export_source_byte_limit")
    data = git(root, "cat-file", "blob", oid)
    item = scanner.scan(io.BytesIO(data), size, name, "export_source", source_blob=oid, mode=mode)
    if item is None or item["status"] != "SCANNED":
        raise ReviewError("export_source_incomplete")
    return {"path": name, "source_blob": oid, "sha256": digest(data), "mode": mode}


def unreviewed():
    return {"status": "NOT_RUN", "evidence_reference": None}


def export_template(root, commit, paths, scanner):
    commit = source_commit(root, commit)
    if not paths or len(paths) != len(set(paths)):
        raise ReviewError("export_paths_empty_or_duplicate")
    tree = tree_map(root, commit)
    files = []
    for name in sorted(paths):
        entry = source_file(root, name, tree, scanner)
        entry["review"] = {key: unreviewed() for key in REVIEW_KINDS}
        files.append(entry)
    return {"schema_version": 1, "proposal": "UNAPPROVED", "source_commit": commit,
            "strategy": "new_root_reviewed_export", "source_archive_prefix": "",
            "destination": {"repository": None, "visibility": "NOT_SELECTED", "access_review": unreviewed()},
            "approvals": {key: unreviewed() for key in APPROVAL_KINDS}, "files": files,
            "generated_source_archive_review": unreviewed(),
            "history_retained": False, "publication_authorized": False}


def approved_claim(value):
    return (isinstance(value, dict) and value.get("status") == "APPROVED"
            and isinstance(value.get("evidence_reference"), str) and bool(value["evidence_reference"].strip()))


def check_proposal(proposal):
    if (not isinstance(proposal, dict) or proposal.get("schema_version") != 1
            or proposal.get("strategy") != "new_root_reviewed_export"
            or proposal.get("history_retained") is not False
            or proposal.get("publication_authorized") is not False):
        raise ReviewError("unsupported_export_proposal")
    files = proposal.get("files")
    if not isinstance(files, list) or not files or not all(isinstance(item, dict) for item in files):
        raise ReviewError("export_files_missing_or_invalid")
    names = [item.get("path") for item in files]
    if any(not safe_relative(name) for name in names) or len(set(names)) != len(names):
        raise ReviewError("export_paths_unsafe_or_duplicate")
    prefix = proposal.get("source_archive_prefix", "")
    if prefix and (not prefix.endswith("/") or not safe_relative(prefix[:-1])):
        raise ReviewError("export_archive_prefix_invalid")
    return files, prefix


def export_check(root, proposal, scanner, archive=None):
    files, prefix = check_proposal(proposal)
    commit = source_commit(root, proposal.get("source_commit", ""))
    tree = tree_map(root, commit)
    review_missing, mapping = [], []
    for entry in files:
        actual = source_file(root, entry["path"], tree, scanner)
        if any(actual.get(key) != entry.get(key) for key in ("source_blob", "sha256", "mode")):
            raise ReviewError("export_source_hash_or_mode_mismatch")
        mapping.append(actual)
        for kind in REVIEW_KINDS:
            if not approved_claim(entry.get("review", {}).get(kind)):
                review_missing.append({"kind": kind, "path_ref": reference(entry["path"])})
    for kind in APPROVAL_KINDS:
        if not approved_claim(proposal.get("approvals", {}).get(kind)):
            review_missing.append({"kind": kind})
    destination = proposal.get("destination", {})
    destination_ok = (isinstance(destination.get("repository"), str) and bool(destination["repository"].strip())
                      and destination.get("visibility") in ("private", "public")
                      and approved_claim(destination.get("access_review")))
    if not destination_ok:
        review_missing.append({"kind": "destination_and_access"})
    if not approved_claim(proposal.get("generated_source_archive_review")):
        review_missing.append({"kind": "generated_source_archive_review"})
    archive_result = {"status": "NOT_RUN"}
    if archive is not None:
        archive_result = compare_archive(archive, scanner, mapping, prefix)
    plan_hash = digest(json.dumps(sorted(mapping, key=lambda item: item["path"]),
                                  sort_keys=True, separators=(",", ":")).encode())
    return {"source_commit": commit, "source_tree": git(root, "rev-parse", commit + "^{tree}").strip().decode(),
            "allowlist_sha256": plan_hash, "file_count": len(files), "mapping_validation": "PASS",
            "archive_validation": archive_result, "missing_review_claims": review_missing,
            "recorded_review_claims": "UNVERIFIED", "release_approval": "NOT_RUN", "dry_run": True,
            "export_written": False, "publication_authorized": False}


def compare_archive(archive, scanner, mapping, prefix):
    start_inventory, start_gaps = len(scanner.inventory), len(scanner.gaps)
    summary = scan_archive(archive, scanner)
    actual = [item for item in scanner.inventory[start_inventory:] if item["scope"] == "archive_member"]
    expected = {reference(prefix + item["path"]): item for item in mapping}
    observed = {item["path_ref"]: item for item in actual}
    if len(actual) != len(observed) or set(observed) != set(expected):
        raise ReviewError("source_archive_membership_mismatch")
    for name, item in expected.items():
        if observed[name].get("sha256") != item["sha256"]:
            raise ReviewError("source_archive_hash_mismatch")
        mode = int(observed[name].get("mode", "0"), 8)
        if mode != int(item["mode"][-3:], 8):
            raise ReviewError("source_archive_mode_mismatch")
    if len(scanner.gaps) != start_gaps:
        raise ReviewError("source_archive_scan_incomplete")
    return {"status": "PASS", "sha256": summary["sha256"], "membership": "EXACT_ALLOWLIST",
            "automatic_host_generated_archive": "NOT_VERIFIED"}
