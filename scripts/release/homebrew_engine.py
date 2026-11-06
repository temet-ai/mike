"""Explicit same-byte tap update: one complete tree, one non-force branch advance."""

import hashlib
from pathlib import PurePosixPath

from homebrew_proposal import git_blob
from publication_engine import preflight as archive_preflight
from publication_inputs import COMMIT, digest, require


def make_plan(proposal, archive_plan, qualification_fingerprint):
    return {"schema_version": 1, "kind": "mike-homebrew-plan", "proposal": proposal,
            "archive_plan": archive_plan, "homebrew_qualification_sha256": qualification_fingerprint,
            "operations": ["verify already-published archive bytes", "verify exact tap base and predecessors",
                           "create two-formula Git tree and commit", "non-force advance existing default branch"],
            "post_publication": {"AC26": "NOT_RUN", "native_channel_install": "NOT_RUN"}}


def commit_message(plan):
    return "Release mike " + plan["proposal"]["candidate_identity"]["version"] + "\n\nmike-homebrew-plan-sha256: " + digest(plan)


def checked_commit(value, sha):
    require(isinstance(value, dict) and value.get("sha") == sha, "tap_commit_identity_mismatch")
    tree = value.get("tree", {}).get("sha")
    require(isinstance(tree, str) and COMMIT.fullmatch(tree), "invalid_tap_commit_tree")
    require(isinstance(value.get("parents"), list)
            and all(isinstance(p.get("sha"), str) and COMMIT.fullmatch(p["sha"]) for p in value["parents"]),
            "invalid_tap_commit_parents")
    return value


def checked_tree(value, sha):
    require(isinstance(value, dict) and value.get("sha") == sha and value.get("truncated") is False
            and isinstance(value.get("tree"), list), "incomplete_tap_tree")
    entries, seen, directories = {}, set(), set()
    for item in value["tree"]:
        name = item.get("path")
        require(isinstance(name, str) and name and not name.startswith("/") and "\\" not in name
                and "\0" not in name and PurePosixPath(name).as_posix() == name
                and not any(part in ("", ".", "..") for part in name.split("/")) and name not in seen,
                "unsafe_or_duplicate_tap_path")
        seen.add(name)
        require(isinstance(item.get("sha"), str) and COMMIT.fullmatch(item["sha"]), "invalid_tap_object_sha")
        kind, mode = item.get("type"), item.get("mode")
        require((kind, mode) in (("tree", "040000"), ("blob", "100644"), ("blob", "100755"),
                                 ("blob", "120000"), ("commit", "160000")), "invalid_tap_object_kind")
        if kind == "tree":
            directories.add(name)
        else:
            entries[name] = {"mode": mode, "type": kind, "sha": item["sha"]}
    for name in seen:
        for parent in PurePosixPath(name).parents:
            if str(parent) != ".":
                require(str(parent) in directories, "tap_parent_not_a_tree")
    leaf_parents = {str(parent) for name in entries for parent in PurePosixPath(name).parents if str(parent) != "."}
    require(directories == leaf_parents, "empty_tap_directory_not_supported")
    require("Formula" not in entries, "formula_directory_is_not_a_tree")
    require(not directories.intersection({"Formula/mike.rb", "Formula/mike-mcp.rb"}),
            "formula_path_is_a_directory")
    return entries


def expected_tree(plan, base):
    expected = dict(base)
    for item in plan["proposal"]["formulas"]:
        expected[item["path"]] = {"mode": "100644", "type": "blob", "sha": item["git_blob_sha1"]}
    return expected


def verify_formula_bytes(plan, tap_transport):
    for item in plan["proposal"]["formulas"]:
        data = tap_transport.blob(item["git_blob_sha1"])
        require(isinstance(data, bytes) and data == item["content"].encode()
                and hashlib.sha256(data).hexdigest() == item["sha256"]
                and git_blob(data) == item["git_blob_sha1"], "remote_formula_bytes_changed")


def preflight(plan, archive_transport, tap_transport):
    archive = archive_preflight(plan["archive_plan"], archive_transport)
    require(archive["status"] == "already_published", "qualified_archives_must_already_be_published")
    tap = plan["proposal"]["tap"]
    repository = tap_transport.repository()
    require(repository.get("full_name") == tap["repository"] and repository.get("visibility") == "public",
            "tap_repository_identity_or_visibility_changed")
    require(repository.get("default_branch") == tap["branch"], "tap_default_branch_changed")
    require(repository.get("archived") is False and repository.get("disabled") is False
            and repository.get("permissions", {}).get("push") is True, "tap_write_permission_denied")
    head = tap_transport.branch(tap["branch"])
    require(isinstance(head, str) and COMMIT.fullmatch(head), "existing_tap_branch_required")
    base = checked_commit(tap_transport.commit(tap["base_commit"]), tap["base_commit"])
    tree_sha = base["tree"]["sha"]
    files = checked_tree(tap_transport.tree(tree_sha), tree_sha)
    for path, sha in tap["formula_blobs"].items():
        expected = None if sha is None else {"mode": "100644", "type": "blob", "sha": sha}
        require(files.get(path) == expected, "predecessor_formula_identity_conflict")
    if head == tap["base_commit"]:
        return {"status": "tap_preflight_passed", "base_tree": tree_sha, "base_files": files, "head": head}
    actual = checked_commit(tap_transport.commit(head), head)
    require(actual.get("message") == commit_message(plan)
            and [p["sha"] for p in actual["parents"]] == [tap["base_commit"]], "tap_branch_moved_or_foreign_commit")
    observed = checked_tree(tap_transport.tree(actual["tree"]["sha"]), actual["tree"]["sha"])
    require(observed == expected_tree(plan, files), "published_tap_tree_conflict")
    verify_formula_bytes(plan, tap_transport)
    return {"status": "tap_already_published", "head": head, "base_tree": tree_sha, "base_files": files}


def publish(plan, archive_transport, tap_transport, approved_plan, approved_tap, approved_archives):
    require(approved_plan == digest(plan) and approved_tap == plan["proposal"]["tap"]["repository"]
            and approved_archives == plan["proposal"]["archive_destination"]["repository"],
            "exact_homebrew_plan_and_repositories_approval_required")
    state = preflight(plan, archive_transport, tap_transport)
    if state["status"] == "tap_already_published":
        return state
    # No local path or evidence is transmitted. These exact formula bytes have
    # already been regenerated, compared and covered by the approved plan hash.
    entries = [{"path": item["path"], "mode": "100644", "type": "blob", "content": item["content"]}
               for item in plan["proposal"]["formulas"]]
    tree_sha = tap_transport.create_tree(state["base_tree"], entries)
    require(isinstance(tree_sha, str) and COMMIT.fullmatch(tree_sha), "invalid_created_tap_tree")
    tree = checked_tree(tap_transport.tree(tree_sha), tree_sha)
    require(tree == expected_tree(plan, state["base_files"]), "created_tap_tree_changed_unapproved_files")
    verify_formula_bytes(plan, tap_transport)
    base = plan["proposal"]["tap"]["base_commit"]
    new_sha = tap_transport.create_commit(tree_sha, base, commit_message(plan))
    require(isinstance(new_sha, str) and COMMIT.fullmatch(new_sha), "invalid_created_tap_commit")
    commit = checked_commit(tap_transport.commit(new_sha), new_sha)
    require(commit["tree"]["sha"] == tree_sha and commit.get("message") == commit_message(plan)
            and [p["sha"] for p in commit["parents"]] == [base], "created_tap_commit_conflict")
    current = preflight(plan, archive_transport, tap_transport)
    if current["status"] == "tap_already_published":
        return current
    # force=False prevents overwriting a concurrently advanced branch. Unknown
    # outcomes stop; another explicit same-plan invocation discovers completion.
    tap_transport.advance_branch(plan["proposal"]["tap"]["branch"], new_sha)
    result = preflight(plan, archive_transport, tap_transport)
    require(result["status"] == "tap_already_published" and result["head"] == new_sha,
            "tap_publication_not_observed")
    result["status"] = "tap_published_same_bytes"
    return result
