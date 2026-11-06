"""Complete release case inventory; no helper result promotes a case to PASS."""

POLICY = "docs/RELEASE_ACCEPTANCE_MATRIX.md"
SOURCE_GATES = {
    "SG01": "isolated DB-free source gates and script regressions",
    "SG02": "disposable database tier, SQLx and immutable migrations",
    "SG03": "frontend, advisory, license and hosted routing gates",
}
EXTERNAL_GATES = {
    "PUBLIC01": "candidate content, rights and distribution review",
    "PUBLIC02": "proposed source tree and reachable-history review",
    "FEEDBACK01": "independent installation, repeat use and corrective update",
}


# Alpha bar (operator ruling): install and run, plus every case that can lose data.
# Other cases stay in the ledger as NOT_RUN and do not gate an alpha.
ALPHA_GATING = ("AC01", "AC02", "AC03", "AC07", "AC08", "AC22")
INSTALL_CHECKS = ("disposable_local_install", "installed_cli_smoke", "repeat_install_and_cli_smoke")


def journey_passed(journey, manifest_sha256):
    """True only for a journey that ran, bound to this candidate, with every check PASSED."""
    binding = journey.get("binding") or {}
    checks = journey.get("checks") or []
    return (journey.get("state") != "FAILED" and bool(checks)
            and binding.get("manifest_sha256") == manifest_sha256
            and all(item.get("state") == "PASSED" for item in checks))


def record_pass(report, case_ids, source, check_ids):
    """Record a PASS from a real journey's observation. Command/exit stay null: the runner is in-process."""
    for case in report["cases"]:
        if case["id"] in case_ids:
            case.update(state="PASS", reason=None, blockers=[],
                        observed={"source": source, "checks_passed": list(check_ids)},
                        evidence=[source])


def alpha_complete(report):
    gating = [c for c in report["cases"] if c["gates_alpha"]]
    return report["state"] != "FAILED" and all(c["state"] == "PASS" for c in gating)


def requirements(identifier):
    if identifier == "PUBLIC02":
        return ["before_source_exposure"]
    if identifier in ("AC26", "FEEDBACK01"):
        return ["alpha_after_publication"]
    return ["alpha_before_publication"]


def case_ledger(names):
    cases = [(f"AC{number:02}", name) for number, name in enumerate(names, 1)]
    cases += list(SOURCE_GATES.items()) + list(EXTERNAL_GATES.items())
    return [{"id": identifier, "name": name, "required": identifier in ALPHA_GATING,
             "gates_alpha": identifier in ALPHA_GATING, "state": "NOT_RUN",
             "required_before": requirements(identifier),
             "scope_note": scope_note(identifier),
             "reason": "complete case evidence has not been recorded by this helper runner",
             "expected": "all observations in the pinned acceptance matrix for this case",
             "observed": None, "runtime_input_identity": None,
             "command": None, "exit_status": None, "elapsed_seconds": None,
             "evidence": [], "counts": {key: None for key in ("selected", "executed", "ignored", "excluded")},
             "blockers": [{"code": "FULL_CASE_NOT_EXECUTED", "kind": "missing_evidence"}]}
            for identifier, name in cases]


def scope_note(identifier):
    if identifier == "AC16":
        return "Requires config/lifecycle negatives and the full platform case."
    if identifier == "AC19":
        return "Requires authorized real-client smoke; fixture clients do not qualify it."
    if identifier == "AC15":
        return "Manual restart required; unadvertised autostart remains NOT SUPPORTED."
    if identifier == "AC20":
        return "900 seconds of real monotonic time."
    if identifier == "AC23":
        return "Uses the frozen internal predecessor or the published predecessor."
    if identifier == "PUBLIC02":
        return "Conditional on source exposure; not a prerequisite to a reviewed binary-only release."
    if identifier in ("AC26", "FEEDBACK01"):
        return "Post-publication evidence; not a circular prerequisite to first publication."
    return "Each advertised platform/channel needs its own applicable observations."
