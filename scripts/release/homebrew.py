#!/usr/bin/env python3
"""Propose formulas offline; separately qualify, preflight and explicitly publish a tap pair."""

import argparse
import json
from pathlib import Path
import sys
import tarfile

from common import ReleaseError
from homebrew_engine import make_plan, preflight, publish
from homebrew_github import TapGitHub
from homebrew_proposal import (homebrew_qualification, inspect_archives, proposal,
                               tap_contract, validate_proposal, write_proposal)
from publication import emit
from publication_engine import RemoteFailure, make_plan as archive_plan
from publication_gates import check_qualification
from publication_github import GitHub
from publication_inputs import destination, digest, read_json, require, selected_inputs


def parser():
    cli = argparse.ArgumentParser(description=__doc__, epilog=
        "Propose/validate/plan are offline. No candidate, Ruby, Homebrew, Cargo or build tool is executed. "
        "Preflight/publish require independent native Homebrew evidence and an already-published qualified archive. "
        "Publish advances only the explicitly reviewed existing tap branch, without force; AC26 remains NOT_RUN.")
    cli.add_argument("command", choices=("propose", "validate", "plan", "preflight", "publish"))
    cli.add_argument("--candidate", required=True, type=Path, action="append")
    cli.add_argument("--destination", required=True, type=Path, help="public alpha archive destination JSON")
    cli.add_argument("--tap", required=True, type=Path, help="explicit tap/base/predecessor contract JSON")
    cli.add_argument("--proposal", required=True, type=Path, help="new proposal directory for propose, existing for other commands")
    cli.add_argument("--qualification", type=Path, help="independently approved archive qualification")
    cli.add_argument("--homebrew-qualification", type=Path, help="independently approved exact formula/native channel evidence")
    cli.add_argument("--confirm-archives", help="exact public archive repository; required before any network")
    cli.add_argument("--confirm-tap", help="exact existing tap repository; required before any network")
    cli.add_argument("--approve-plan", help="exact offline tap plan hash; required before any publication request")
    cli.add_argument("--output", type=Path, help="new private report JSON; existing files refused")
    return cli


def run(args, archive_factory=GitHub, tap_factory=TapGitHub):
    if args.output:
        require(not args.output.exists() and not args.output.is_symlink() and args.output.parent.is_dir(),
                "output_must_be_a_new_file_in_an_existing_directory")
    with selected_inputs(args.candidate) as (identities, paths):
        request, _ = read_json(args.destination)
        request = destination(request, identities)
        tap, _ = read_json(args.tap)
        value = proposal(identities, request, tap_contract(tap))
        inspect_archives(paths)
        if args.command == "propose":
            write_proposal(args.proposal, value, args.candidate)
        fingerprint = validate_proposal(args.proposal, value)
        result = {"state": "UNQUALIFIED_FORMULA_PROPOSAL", "proposal_sha256": fingerprint,
                  "qualified": False, "published": False, "permission_preflight": "NOT_RUN", "AC26": "NOT_RUN"}
        if args.command not in ("propose", "validate"):
            require(args.qualification is not None and args.homebrew_qualification is not None,
                    "archive_and_homebrew_qualification_required")
            archive_fingerprint = check_qualification(args.qualification, request, identities)
            homebrew_fingerprint = homebrew_qualification(args.homebrew_qualification, value, archive_fingerprint)
            plan = make_plan(value, archive_plan(request, identities, archive_fingerprint), homebrew_fingerprint)
            result.update(state="OFFLINE_HOMEBREW_PLAN", plan=plan, plan_sha256=digest(plan))
            if args.command in ("preflight", "publish"):
                require(args.confirm_archives == request["repository"] and args.confirm_tap == tap["repository"],
                        "both_named_repository_confirmations_required_before_network")
                if args.command == "publish":
                    require(args.approve_plan == digest(plan), "exact_homebrew_plan_approval_required_before_network")
                archives, remote = archive_factory(request["repository"]), tap_factory(tap["repository"])
                if args.command == "preflight":
                    status = preflight(plan, archives, remote)
                else:
                    status = publish(plan, archives, remote, args.approve_plan, args.confirm_tap, args.confirm_archives)
                result.update(state=status["status"], head_commit=status["head"],
                              published=status["status"] in ("tap_already_published", "tap_published_same_bytes"),
                              permission_preflight="PASSED_REPOSITORY_ROLE_CHECK")
        emit(result, args.output)
    return 0


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        return run(args)
    except (ReleaseError, RemoteFailure, OSError, ValueError, KeyError, TypeError, AttributeError, tarfile.TarError) as exc:
        code = str(exc) if isinstance(exc, (ReleaseError, RemoteFailure)) else "invalid_or_unavailable_homebrew_input"
        print(json.dumps({"state": "BLOCKED", "code": code, "qualified": False, "AC26": "NOT_RUN",
                          "published": None if args.command == "publish" else False,
                          "outcome_note": "Unknown writes are not retried; inspect and explicitly resume the exact plan. "
                                          "Never force, recreate a branch, edit the predecessor contract or move a release tag."}),
              file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
