#!/usr/bin/env python3
"""Plan offline, preflight read-only, or explicitly publish qualified same-byte archives."""

import argparse
import json
import os
from pathlib import Path
import sys

from common import ReleaseError
from publication_engine import RemoteFailure, make_plan, preflight, publish
from publication_gates import check_qualification
from publication_github import GitHub
from publication_inputs import canonical, destination, digest, read_json, require, selected_inputs


def parser():
    cli = argparse.ArgumentParser(description=__doc__, epilog=
        "Current local acceptance/security reports are unqualified and are rejected before networking. "
        "No rebuild, tag/repository creation, tap update, credential setup, or visibility change. "
        "A successful publication still leaves AC26 and external feedback NOT_RUN.")
    cli.add_argument("command", choices=("inspect", "plan", "preflight", "publish"))
    cli.add_argument("--candidate", required=True, type=Path, action="append")
    cli.add_argument("--qualification", type=Path)
    cli.add_argument("--destination", type=Path)
    cli.add_argument("--output", type=Path, help="new private JSON report; never overwritten")
    cli.add_argument("--approve-plan", help="exact offline plan SHA-256; required only to publish")
    cli.add_argument("--confirm-repository", help="exact owner/repository; required for any network access")
    return cli


def emit(value, output):
    encoded = canonical(value)
    if output:
        descriptor = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(encoded)
    else:
        print(encoded.decode(), end="")


def run(args, transport_factory=GitHub):
    if args.output:
        require(not args.output.exists() and not args.output.is_symlink() and args.output.parent.is_dir(),
                "output_must_be_a_new_file_in_an_existing_directory")
    with selected_inputs(args.candidate) as (identities, paths):
        if args.command == "inspect":
            emit({"state": "UNQUALIFIED_INVENTORY", "candidate_identities": identities,
                  "qualified": False, "published": False, "permission_preflight": "NOT_RUN"}, args.output)
            return 0
        require(args.destination is not None and args.qualification is not None,
                "destination_and_qualification_required")
        request, _ = read_json(args.destination)
        request = destination(request, identities)
        evidence_hash = check_qualification(args.qualification, request, identities)
        plan = make_plan(request, identities, evidence_hash)
        result = {"state": "OFFLINE_PLAN", "plan_sha256": digest(plan), "plan": plan,
                  "permission_preflight": "NOT_RUN", "published": False,
                  "distribution_qualified": False, "AC26": "NOT_RUN", "FEEDBACK01": "NOT_RUN"}
        if args.command != "plan":
            require(args.confirm_repository == request["repository"], "named_repository_confirmation_required_before_network")
            if args.command == "publish":
                require(args.approve_plan == digest(plan), "exact_plan_approval_required_before_network")
            transport = transport_factory(request["repository"])
            if args.command == "preflight":
                state = preflight(plan, transport)
                result["state"] = "REMOTE_PREFLIGHT"
                result["published"] = state["status"] == "already_published"
            else:
                state = publish(plan, paths, transport, args.approve_plan, args.confirm_repository)
                result["state"] = "PUBLISHED_SAME_BYTES"
                result["published"] = True
            result["permission_preflight"] = "PASSED_REPOSITORY_ROLE_CHECK"
            result["remote_status"] = state["status"]
            result["missing_assets"] = state["missing"]
            if state["release"]:
                result["release_id"] = state["release"]["id"]
                result["release_url"] = ("https://github.com/" + request["repository"]
                                         + "/releases/tag/" + request["tag"])
        emit(result, args.output)
        return 0


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        return run(args)
    except (ReleaseError, RemoteFailure, OSError, ValueError, KeyError, TypeError, AttributeError) as exc:
        # Do not print input file contents, external stderr, local paths, or tokens.
        code = str(exc) if isinstance(exc, (ReleaseError, RemoteFailure)) else "invalid_or_unavailable_publication_input"
        print(json.dumps({"state": "BLOCKED", "code": code, "published": None if args.command == "publish" else False,
                          "distribution_qualified": False, "AC26": "NOT_RUN",
                          "outcome_note": "A failed or interrupted live operation may have left a draft or published release. "
                                          "Inspect and explicitly resume the exact approved plan; do not delete, clobber or rebuild."}),
              file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
