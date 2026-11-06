"""Resumable additive publication over a mockable transport; never rebuild or tag."""

from publication_inputs import canonical, digest, require


class RemoteFailure(Exception):
    """A remote request failed or its outcome is unknown; explicit resume required."""


def make_plan(request, identities, qualification_sha256):
    return {"schema_version": 1, "kind": "mike-publication-plan", "destination": request,
            "candidate_identities": identities, "qualification_sha256": qualification_sha256,
            "permission_checks": ["exact existing repository and visibility", "repository push role",
                                  "exact existing tag commit", "all remote assets verified by download"],
            "assets": sorted([item for identity in identities for item in identity["files"]],
                             key=lambda item: item["name"]),
            "post_publication": {"AC26": "NOT_RUN", "FEEDBACK01": "NOT_RUN"}}


def release_spec(plan):
    destination = plan["destination"]
    # Only intentionally public provenance is transmitted. Local evidence,
    # reviewer identities, filesystem paths and credentials are never uploaded.
    provenance = {"publication_plan_sha256": digest(plan),
                  "candidates": [{key: identity[key] for key in
                                  ("version", "source_commit", "target", "manifest_sha256")}
                                 for identity in plan["candidate_identities"]],
                  "channel_qualification": "AC26 NOT_RUN; publication is not installation qualification"}
    return {"tag_name": destination["tag"], "target_commitish": destination["tag_commit"],
            "name": f"mike {destination['tag']} (alpha)",
            "body": canonical(provenance).decode(), "draft": True, "prerelease": True,
            "generate_release_notes": False}


def check_release(plan, release):
    require(type(release.get("id")) is int and release["id"] > 0, "invalid_release_id")
    for key, expected in release_spec(plan).items():
        if key not in ("draft", "generate_release_notes", "target_commitish"):
            require(release.get(key) == expected, "existing_release_identity_conflict_" + key)
    # GitHub documents target_commitish as unused for an existing tag. The
    # exact resolved tag commit is checked independently before every operation.
    require(type(release.get("draft")) is bool, "invalid_release_state")


def inventory(plan, release, transport):
    expected = {item["name"]: item for item in plan["assets"]}
    observed = transport.assets(release["id"])
    require(isinstance(observed, list), "invalid_remote_asset_inventory")
    present = set()
    for item in observed:
        name = item.get("name")
        require(name in expected and name not in present, "unexpected_or_duplicate_remote_asset")
        require(item.get("state") == "uploaded" and item.get("size") == expected[name]["size"],
                "remote_asset_incomplete_or_size_conflict")
        require(type(item.get("id")) is int and item["id"] > 0, "invalid_remote_asset_id")
        actual = transport.asset_sha256(item["id"], expected[name]["size"])
        require(actual == expected[name]["sha256"], "remote_asset_hash_conflict")
        present.add(name)
    missing = sorted(set(expected) - present)
    require(release["draft"] or not missing, "published_release_is_incomplete_refusing_mutation")
    return missing


def preflight(plan, transport):
    destination = plan["destination"]
    repository = transport.repository()
    require(repository.get("full_name") == destination["repository"], "destination_repository_identity_changed")
    require(repository.get("visibility") == destination["visibility"], "destination_visibility_changed")
    require(repository.get("archived") is False and repository.get("disabled") is False,
            "destination_not_writable")
    require(repository.get("permissions", {}).get("push") is True, "destination_write_permission_denied")
    require(transport.tag_commit(destination["tag"]) == destination["tag_commit"],
            "existing_tag_missing_or_changed_no_tag_will_be_created")
    release = transport.release(destination["tag"])
    if release is None:
        missing = [item["name"] for item in plan["assets"]]
    else:
        check_release(plan, release)
        missing = inventory(plan, release, transport)
    return {"release": release, "missing": missing,
            "status": "already_published" if release and not release["draft"] else "preflight_passed",
            "AC26": "NOT_RUN", "FEEDBACK01": "NOT_RUN"}


def publish(plan, paths, transport, approved_plan, approved_repository):
    require(approved_plan == digest(plan), "exact_plan_approval_required")
    require(approved_repository == plan["destination"]["repository"], "named_repository_approval_required")
    state = preflight(plan, transport)
    if state["status"] == "already_published":
        return state
    if state["release"] is None:
        # Unknown outcomes are deliberately not retried here. Another invocation
        # discovers a created draft by its exact provenance marker and resumes.
        transport.create_draft(release_spec(plan))
        state = preflight(plan, transport)
        require(state["release"] is not None, "draft_creation_not_observed")
    release_id = state["release"]["id"]
    for asset in plan["assets"]:
        state = preflight(plan, transport)
        require(state["release"] is not None and state["release"]["id"] == release_id,
                "release_changed_during_publication")
        if state["status"] == "already_published":
            return state
        if asset["name"] in state["missing"]:
            # Transport consumes the private snapshot, never mutable caller paths.
            transport.upload(release_id, asset["name"], paths[asset["name"]], asset["sha256"])
    state = preflight(plan, transport)
    require(state["release"] is not None and state["release"]["id"] == release_id,
            "release_changed_before_publication")
    require(not state["missing"], "draft_incomplete")
    if state["release"]["draft"]:
        transport.publish(release_id)
    result = preflight(plan, transport)
    require(result["status"] == "already_published", "publication_not_observed_resume_required")
    result["status"] = "published_same_bytes"
    return result
