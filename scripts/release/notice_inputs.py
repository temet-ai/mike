#!/usr/bin/env python3
"""Package explicit offline notice-review claims without granting release rights."""

import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import re
import sys

from common import ReleaseError, spa_inventory
from notice_inventory import (HEX, MAX_INPUT_BYTES, binding, canonical, checked_assets, digest,
                              open_directory, read_bounded, read_json, require, safe_relative, source_inventory,
                              text_field, validate_inventory)

MAX_TEXT_BYTES = 256 * 1024
MAX_TOTAL_TEXT_BYTES = 8 * 1024**2
MAX_TEXTS = 10000
MAX_NOTICE_BYTES = 32 * 1024**2
STATUS = "unqualified local candidate; not for publication"
REVIEW_STATUS = "recorded_unverified"
BASE = {"project": "mike", "license": "MIT", "source_license": "LICENSE",
        "third_party_notice_review": "not_run", "status": STATUS}


def encoded(value):
    data = (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True) + "\n").encode()
    require(len(data) <= MAX_NOTICE_BYTES, "packaged notice byte limit exceeded")
    return data


def template(root, source_commit, version, target, assets):
    pins, inventory = source_inventory(root, source_commit, version, target, assets)
    return {"schema_version": 1, "binding": pins, "inventory": inventory,
            "inventory_sha256": digest(canonical(inventory)), "publication_authorized": False,
            "review": {"status": "NOT_RUN", "reviewer": "", "reviewed_at": "", "evidence_reference": ""},
            "reviews": [{"subject_id": item["id"], "status": "NOT_RUN", "disposition": "UNRESOLVED",
                         "binaries": [], "notice_ids": [], "license_expression": "", "upstream": "",
                         "basis": "", "evidence_reference": ""} for item in inventory], "texts": []}


def validate_review(review):
    require(isinstance(review, dict) and set(review) == {"status", "reviewer", "reviewed_at", "evidence_reference"},
            "missing human review claim")
    require(review["status"] == "REVIEWED" and text_field(review["reviewer"], 256)
            and text_field(review["evidence_reference"]), "human review claim is missing or unreviewed")
    require(isinstance(review["reviewed_at"], str)
            and re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", review["reviewed_at"]),
            "review timestamp must be an explicit UTC timestamp")
    try:
        datetime.strptime(review["reviewed_at"], "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as error:
        raise ReleaseError("third-party notices: invalid review timestamp") from error


def validate_texts(texts):
    require(isinstance(texts, list) and 0 < len(texts) <= MAX_TEXTS, "missing or excessive notice texts")
    ids, paths, total = [], set(), 0
    for item in texts:
        require(isinstance(item, dict) and set(item) == {"id", "path", "sha256", "size"},
                "invalid notice text descriptor")
        require(isinstance(item["id"], str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", item["id"]),
                "invalid notice text identity")
        parts = safe_relative(item["path"])
        require(len(parts) == 1 and item["path"] not in paths, "notice texts must have distinct adjacent filenames")
        require(isinstance(item["sha256"], str) and HEX.fullmatch(item["sha256"])
                and type(item["size"]) is int and 0 < item["size"] <= MAX_TEXT_BYTES, "invalid notice text pins")
        ids.append(item["id"])
        paths.add(item["path"])
        total += item["size"]
    require(ids == sorted(set(ids)), "notice texts must be unique and sorted")
    require(total <= MAX_TOTAL_TEXT_BYTES, "total notice text byte limit exceeded")
    return set(ids)


def validate_dispositions(reviews, inventory, text_ids):
    require(isinstance(reviews, list) and len(reviews) == len(inventory), "missing or extra inventory reviews")
    identifiers, used = [], set()
    for item in reviews:
        require(isinstance(item, dict) and set(item) == {
            "subject_id", "status", "disposition", "binaries", "notice_ids", "license_expression",
            "upstream", "basis", "evidence_reference"}, "invalid inventory review")
        require(item["status"] == "REVIEWED" and item["disposition"] in (
            "notice_required", "no_notice_required", "not_shipped"), "unreviewed inventory subject")
        require(all(text_field(item[field]) for field in ("license_expression", "upstream", "basis", "evidence_reference")),
                "missing license, provenance, basis or review evidence")
        binaries, notices = item["binaries"], item["notice_ids"]
        require(isinstance(binaries, list) and all(isinstance(i, str) for i in binaries)
                and binaries == sorted(set(binaries)) and set(binaries) <= {"mike", "mike-mcp"},
                "invalid reviewed binary applicability")
        require(isinstance(notices, list) and all(isinstance(i, str) for i in notices)
                and notices == sorted(set(notices)) and set(notices) <= text_ids, "invalid required notice mapping")
        if item["disposition"] == "notice_required":
            require(binaries and notices, "shipped subject is missing required notices")
        elif item["disposition"] == "no_notice_required":
            require(binaries and not notices, "invalid no-notice-required disposition")
        else:
            require(not binaries and not notices, "excluded subject must not claim shipped notices")
        identifiers.append(item["subject_id"])
        used.update(notices)
    require(identifiers == [item["id"] for item in inventory], "missing, duplicate, reordered or extra inventory reviews")
    require(used == text_ids, "unreferenced notice text")


def validate_manifest(value, pins, inventory=None):
    require(isinstance(value, dict) and set(value) == {
        "schema_version", "binding", "inventory", "inventory_sha256", "publication_authorized", "review", "reviews", "texts"},
        "invalid reviewed manifest schema")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["publication_authorized"] is False, "notice input must not authorize publication")
    require(value["binding"] == pins, "candidate source, target, version, lockfile or SPA binding mismatch")
    validate_inventory(value["inventory"])
    require(value["inventory_sha256"] == digest(canonical(value["inventory"])), "review inventory digest mismatch")
    if inventory is not None:
        require(value["inventory"] == inventory, "full candidate review inventory mismatch")
    validate_review(value["review"])
    text_ids = validate_texts(value["texts"])
    validate_dispositions(value["reviews"], value["inventory"], text_ids)
    return value


def checked_text(data, item):
    require(len(data) == item["size"] and digest(data) == item["sha256"], "notice text hash or size mismatch")
    try:
        text = data.decode("utf-8")
    except UnicodeError as error:
        raise ReleaseError("third-party notices: license and notice texts must be UTF-8") from error
    require(text.strip() and "\0" not in text, "license or notice text is empty or contains NUL")
    return text


def prepare_notice(root, reviewed_manifest, source_commit, version, target, assets):
    """No input preserves the predecessor placeholder; explicit input must validate."""
    if reviewed_manifest is None:
        return encoded(BASE)
    path = Path(reviewed_manifest)
    raw = read_bounded(path, MAX_INPUT_BYTES)
    pins, inventory = source_inventory(root, source_commit, version, target, assets)
    value = validate_manifest(read_json(raw), pins, inventory)
    require(all(item["path"] != path.name for item in value["texts"]), "manifest cannot be its own notice text")
    payloads = [{"id": item["id"], "text": checked_text(read_bounded(path.parent / item["path"], MAX_TEXT_BYTES), item)}
                for item in value["texts"]]
    notice = dict(BASE, third_party_notice_review=REVIEW_STATUS, reviewed_notices={
        "manifest": value, "manifest_sha256": digest(canonical(value)), "text_payloads": payloads,
        "mechanical_validation": "passed", "human_review_claims": "UNVERIFIED",
        "rights_approval": "NOT_RUN", "publication_authorized": False})
    return encoded(notice)


def candidate_binding(candidate):
    try:
        build = candidate["build"]
        return binding(candidate["source_commit"], candidate["version"], candidate["target"],
                       build["cargo_lock_sha256"], build["pnpm_lock_sha256"], candidate["spa_source_assets"])
    except (KeyError, TypeError) as error:
        raise ReleaseError("third-party notices: candidate lacks notice source binding") from error


def verify_notice(data, candidate_manifest):
    """Verify only mechanics and recorded claims; never authenticate a reviewer."""
    require(isinstance(data, bytes) and len(data) <= MAX_NOTICE_BYTES, "packaged notice byte limit exceeded")
    value = read_json(data)
    if value == BASE:
        return "not_run"
    require(isinstance(value, dict) and set(value) == set(BASE) | {"reviewed_notices"}
            and all(value.get(k) == v for k, v in BASE.items() if k != "third_party_notice_review")
            and value["third_party_notice_review"] == REVIEW_STATUS, "unsupported packaged notice")
    package = value["reviewed_notices"]
    require(isinstance(package, dict) and set(package) == {
        "manifest", "manifest_sha256", "text_payloads", "mechanical_validation", "human_review_claims",
        "rights_approval", "publication_authorized"}, "invalid packaged notice schema")
    require(package["mechanical_validation"] == "passed" and package["human_review_claims"] == "UNVERIFIED"
            and package["rights_approval"] == "NOT_RUN" and package["publication_authorized"] is False,
            "packaged notice must not elevate human claims or grant rights")
    pins = candidate_binding(candidate_manifest)
    manifest = validate_manifest(package["manifest"], pins)
    require(package["manifest_sha256"] == digest(canonical(manifest)), "packaged review manifest hash mismatch")
    actual_assets = [{"path": item["name"], "sha256": item["content_sha256"], "size": item["size"]}
                     for item in manifest["inventory"] if item["kind"] == "spa_asset"]
    require(sorted(actual_assets, key=lambda item: item["path"]) == checked_assets(candidate_manifest["spa_source_assets"]),
            "packaged SPA inventory mismatch")
    payloads = package["text_payloads"]
    require(isinstance(payloads, list) and len(payloads) == len(manifest["texts"]), "missing or extra packaged notice texts")
    for payload, expected in zip(payloads, manifest["texts"]):
        require(isinstance(payload, dict) and set(payload) == {"id", "text"} and payload["id"] == expected["id"]
                and isinstance(payload["text"], str), "invalid or reordered packaged notice text")
        try:
            checked_text(payload["text"].encode("utf-8"), expected)
        except UnicodeError as error:
            raise ReleaseError("third-party notices: invalid packaged UTF-8 text") from error
    return REVIEW_STATUS


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("template",))
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    value = template(args.repo, args.source, args.version, args.target, spa_inventory(args.repo))
    data = encoded(value)
    require(len(data) <= MAX_INPUT_BYTES, "template exceeds input manifest byte limit")
    directory = open_directory(args.output.parent)
    try:
        descriptor = os.open(args.output.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                             0o600, dir_fd=directory)
        with os.fdopen(descriptor, "wb") as output:
            output.write(data)
    finally:
        os.close(directory)
    print("Wrote an unreviewed notice template. Review and supply actual texts before preparation.")


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError, ValueError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
