#!/usr/bin/env python3
"""Local PUBLIC01/PUBLIC02 evidence only. No scan can approve distribution."""

import argparse
from collections import Counter
import json
from pathlib import Path
import sys

from security_common import ReviewError, Scanner, open_regular, write_private_json
from security_export import export_check, export_template
from security_rights import inventory_rights
from security_sources import scan_archive, scan_history, scan_tree

LIMITATIONS = [
    "Pattern matches are review candidates; absence of matches is not privacy or security approval.",
    "Conversation prose, screenshot pixels, image metadata and distribution rights require human review.",
    "ASCII and UTF-16 strings are searched; encrypted, compressed and obfuscated embedded data are not decoded.",
    "Resource string references do not prove the complete embedded SPA, font, asset or source-map inventory.",
    "Candidate files are never executed; no binary-to-source provenance, behavior or vulnerability verdict is produced.",
    "Only tracked working-tree files, requested local reachable history and explicitly named archives are read.",
    "Untracked/ignored files, remote-only refs, reflogs, unreachable Git objects and submodule contents are excluded.",
    "LFS pointers do not include their payload. Shallow/missing objects and size limits prevent complete coverage.",
    "License manifests and notice presence do not establish redistribution rights or notice sufficiency.",
    "No network, credential verification/rotation, extraction to disk, publication or visibility changes are performed.",
]


def report(scanner):
    candidates = Counter()
    for item in scanner.inventory:
        for category in set(item.get("filename_categories", []) + item.get("byte_categories", [])
                            + item.get("string_categories", [])):
            candidates[category] += 1
    result = {"schema_version": 1, "tool": "mike-local-content-review", "qualified": False,
              "PUBLIC01": {"status": "NOT_RUN", "scope": "artifacts_and_distribution"},
              "PUBLIC02": {"status": "NOT_RUN", "scope": "source_and_history_exposure"},
              "manual_reviews": {key: "NOT_RUN" for key in (
                  "conversation_and_screenshot_privacy", "embedded_resource_completeness",
                  "license_and_notices", "confirmed_credential_rotation", "owner_approval",
                  "legal_approval", "distribution_destination_and_access", "generated_source_archive")},
              "redaction": "Path/package/resource references are SHA-256; no matched values or snippets are emitted",
              "limits": scanner.limits, "scanned_bytes": scanner.total,
              "inventory": scanner.inventory, "candidate_category_counts": dict(sorted(candidates.items())),
              "coverage_gaps": scanner.gaps, "limitations": LIMITATIONS}
    result["scan_status"] = ("INCOMPLETE" if scanner.gaps else
                             "REVIEW_CANDIDATES" if candidates else "NO_PATTERN_MATCHES")
    return result


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="command", required=True)
    for name in ("scan", "export-template", "export-check"):
        command = commands.add_parser(name)
        command.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
        command.add_argument("--output", type=Path, required=name == "export-template",
                             help="New private JSON file; existing files are never overwritten")
        if name == "scan":
            command.add_argument("--history", action="store_true", help="Inspect locally reachable Git objects; never fetch")
            command.add_argument("--archive", type=Path, action="append", default=[])
            command.add_argument("--artifact-only", action="store_true", help="Scan only explicitly supplied archives")
        elif name == "export-template":
            command.add_argument("--source", required=True, help="Full reviewed development commit SHA")
            command.add_argument("--path", action="append", required=True, help="Exact source path, no globs or directories")
        else:
            command.add_argument("--allowlist", type=Path, required=True)
            command.add_argument("--archive", type=Path)
    return cli


def scan(args, scanner):
    details = {"history": {"status": "NOT_RUN"}, "archives": []}
    if args.artifact_only:
        if args.history or not args.archive:
            raise ReviewError("artifact_only_requires_archives_and_no_history")
        details["tracked_tree"] = {"status": "NOT_RUN"}
        details["rights_inventory"] = {"status": "NOT_RUN"}
    else:
        details["tracked_tree"] = scan_tree(args.repo, scanner)
        details["rights_inventory"] = inventory_rights(args.repo, scanner)
        if args.history:
            details["history"] = scan_history(args.repo, scanner)
    for path in args.archive:
        details["archives"].append(scan_archive(path, scanner))
    return details


def main(argv=None):
    args = parser().parse_args(argv)
    scanner = Scanner()
    try:
        if args.command == "scan":
            details = scan(args, scanner)
        elif args.command == "export-template":
            template = export_template(args.repo, args.source, args.path, scanner)
            write_private_json(args.output, template)
            print(json.dumps({"status": "UNAPPROVED_TEMPLATE", "file_count": len(template["files"]),
                              "source_commit": template["source_commit"], "qualified": False}))
            return 0
        else:
            with open_regular(args.allowlist.parent, args.allowlist.name) as stream:
                data = stream.read(16 * 1024 * 1024 + 1)
                if len(data) > 16 * 1024 * 1024:
                    raise ReviewError("allowlist_must_be_bounded_regular_file")
                proposal = json.loads(data)
            details = {"export": export_check(args.repo, proposal, scanner, args.archive)}
        result = dict(report(scanner), **details)
        if args.output:
            write_private_json(args.output, result)
        else:
            print(json.dumps(result, indent=2, sort_keys=True, ensure_ascii=True))
        if scanner.gaps:
            return 2
        if args.command == "export-check" and result["export"]["missing_review_claims"]:
            return 1
        return 1 if result["candidate_category_counts"] else 0
    except (ReviewError, OSError, ValueError, KeyError, TypeError, AttributeError, UnicodeError, RecursionError) as exc:
        code = str(exc) if isinstance(exc, ReviewError) else "input_unavailable_or_invalid"
        print(json.dumps({"status": "ERROR", "code": code, "qualified": False,
                          "PUBLIC01": "NOT_RUN", "PUBLIC02": "NOT_RUN"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
