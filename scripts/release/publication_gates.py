"""Reviewer acceptance contract, pinned to #270's stage-specific matrix.

This validates attestations and evidence hashes; it is not an acceptance runner,
security approval, or an authority to publish. Current local reports cannot pass.
"""

from pathlib import Path

from publication_inputs import SHA, read_json, require, text_field

POLICY = "docs/RELEASE_ACCEPTANCE_MATRIX.md"
CASES = tuple(f"AC{n:02}" for n in (*range(1, 26), 27))


def accepted(item, root):
    require(isinstance(item, dict) and item.get("status") == "PASS", "required_gate_not_passed")
    require(text_field(item.get("reviewed_by")) and text_field(item.get("observed")), "reviewer_and_observation_required")
    require(isinstance(item.get("evidence"), list) and item["evidence"], "evidence_required")
    for evidence in item["evidence"]:
        require(isinstance(evidence, dict), "invalid_evidence")
        name, expected = evidence.get("file"), evidence.get("sha256")
        require(text_field(name) and isinstance(expected, str) and SHA.fullmatch(expected), "invalid_evidence_reference")
        relative = Path(name)
        require(not relative.is_absolute() and ".." not in relative.parts, "evidence_must_stay_in_review_bundle")
        path = root / relative
        for parent in (path, *path.parents):
            require(not parent.is_symlink(), "evidence_symlink_refused")
            if parent == root:
                break
        value, actual = read_json(path, 32 * 1024 * 1024)
        require(actual == expected, "evidence_hash_mismatch")
        # Evidence may be a source log envelope or an actual case report. This
        # explicit verdict prevents recycling known unqualified helper output.
        require(value.get("qualified") is not False and value.get("status") == "PASS",
                "evidence_is_not_accepted_qualification")


def check_qualification(path, request, identities):
    value, fingerprint = read_json(path)
    require(type(value.get("schema_version")) is int and value["schema_version"] == 1 and value.get("kind") == "mike-publication-qualification"
            and value.get("qualified") is True and value.get("state") == "APPROVED",
            "qualification_missing_or_incomplete")
    require(value.get("policy") == POLICY and value.get("destination") == request,
            "qualification_policy_or_destination_mismatch")
    require(value.get("candidate_identities") == identities, "qualification_candidate_identity_mismatch")
    gates = value.get("gates", {})
    require(isinstance(gates, dict), "invalid_gate_map")
    required = ["SG01", "SG02", "SG03", "PUBLIC01"]
    if request["source_exposure"]:
        required.append("PUBLIC02")
    for case in required:
        require(case in gates, "missing_" + case)
        accepted(gates[case], path.parent)
    if not request["source_exposure"]:
        require(gates["PUBLIC01"].get("binary_only_destination_and_generated_source_archives_reviewed") is True,
                "binary_only_distribution_contents_not_reviewed")
    target_cases = value.get("target_cases")
    require(isinstance(target_cases, dict) and set(target_cases) == {i["target"] for i in identities},
            "qualification_target_scope_mismatch")
    for target, cases in target_cases.items():
        require(isinstance(cases, dict), "invalid_case_map")
        for case in CASES:
            require(case in cases, "missing_" + case + "_" + target)
            accepted(cases[case], path.parent)
        require(cases["AC08"].get("channels") == request["channels"], "install_channel_not_qualified")
        require(cases["AC15"].get("manual_restart") is True, "manual_restart_not_qualified")
        require(cases["AC23"].get("predecessor_kind") in ("internal", "published"), "predecessor_scope_not_qualified")
        duration = cases["AC20"].get("duration_seconds")
        require(type(duration) in (int, float) and duration >= 900, "reliability_duration_not_qualified")
    return fingerprint
