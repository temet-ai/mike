"""Offline, exact-byte publication inputs. No candidate is executed or qualified here."""

from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import re
import tempfile

from archive import BINARIES
from common import ReleaseError, sha256
from inputs import copy_regular, snapshot_inputs

SHA = re.compile(r"[0-9a-f]{64}")
COMMIT = re.compile(r"[0-9a-f]{40}")
ALPHA_VERSION = re.compile(r"0\.1\.0-alpha\.[1-9][0-9]*")
REPOSITORY = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*")
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?")


def canonical(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True) + "\n").encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def require(condition, code):
    if not condition:
        raise ReleaseError(code)


def text_field(value):
    return isinstance(value, str) and bool(value.strip()) and len(value) <= 4096


def read_json(path, maximum=8 * 1024 * 1024):
    # The same no-follow, bounded copy used for archive inputs rejects FIFOs and
    # symlinks. Evidence/approval bytes are never executed or sent to GitHub.
    with tempfile.TemporaryDirectory(prefix="mike-publication-json-") as temporary:
        snapshot = Path(temporary) / "input.json"
        copy_regular(path, snapshot, maximum)
        raw = snapshot.read_bytes()
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate_json_key")
            result[key] = value
        return result
    def invalid_constant(_):
        raise ReleaseError("nonfinite_json_number")
    value = json.loads(raw, object_pairs_hook=unique, parse_constant=invalid_constant)
    require(isinstance(value, dict), "json_object_required")
    return value, hashlib.sha256(raw).hexdigest()


def candidate_identity(snapshot, manifest):
    require(type(manifest.get("schema_version")) is int and manifest["schema_version"] == 1 and manifest.get("state") == "prepared"
            and manifest.get("qualified") is False, "unsupported_prepared_manifest")
    version, target, commit = (manifest[key] for key in ("version", "target", "source_commit"))
    require(isinstance(version, str) and VERSION.fullmatch(version), "invalid_version")
    require(isinstance(commit, str) and COMMIT.fullmatch(commit), "invalid_source_commit")
    artifacts = manifest["artifacts"]
    require(isinstance(artifacts, list) and len(artifacts) == 2
            and sorted(item["binary"] for item in artifacts) == sorted(BINARIES), "binary_pair_required")
    files = []
    for artifact in sorted(artifacts, key=lambda item: item["binary"]):
        name = f"{artifact['binary']}-v{version}-{target}.tar.gz"
        require(artifact["archive"] == name, "archive_name_mismatch")
        observed = sha256(snapshot / name)
        require(artifact["sha256"] == observed, "archive_hash_mismatch")
        require((snapshot / (name + ".sha256")).read_text() == f"{observed}  {name}\n", "checksum_mismatch")
        for filename in (name, name + ".sha256"):
            files.append({"name": filename, "sha256": sha256(snapshot / filename),
                          "size": (snapshot / filename).stat().st_size})
    manifest_name = f"manifest-{target}.json"
    files.append({"name": manifest_name, "sha256": sha256(snapshot / "manifest.json"),
                  "size": (snapshot / "manifest.json").stat().st_size})
    return {"version": version, "source_commit": commit, "target": target,
            "manifest_sha256": sha256(snapshot / "manifest.json"), "files": sorted(files, key=lambda x: x["name"])}


def destination(value, identities):
    keys = {"schema_version", "kind", "repository", "source_repository", "visibility",
            "tag", "tag_commit", "source_exposure", "channels", "prerelease"}
    require(set(value) == keys and type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["kind"] == "mike-publication-destination", "invalid_destination_contract")
    for key in ("repository", "source_repository"):
        require(isinstance(value[key], str) and REPOSITORY.fullmatch(value[key]), "invalid_repository")
    require(value["visibility"] == "public", "alpha_destination_must_be_public")
    require(ALPHA_VERSION.fullmatch(identities[0]["version"]), "only_0_1_0_alpha_tags_supported")
    require(value["channels"] == ["archive"], "unsupported_channel_tap_requires_separate_qualification_and_adapter")
    require(value["prerelease"] is True, "only_explicit_prereleases_supported")
    require(type(value["source_exposure"]) is bool, "source_exposure_must_be_explicit")
    require(isinstance(value["tag_commit"], str) and COMMIT.fullmatch(value["tag_commit"]), "invalid_tag_commit")
    require(value["tag"] == "v" + identities[0]["version"], "tag_version_mismatch")
    if value["source_exposure"]:
        require(value["tag_commit"] == identities[0]["source_commit"], "source_tag_mapping_mismatch")
    else:
        require(value["repository"].lower() != value["source_repository"].lower()
                and value["tag_commit"] != identities[0]["source_commit"], "binary_only_requires_separate_reviewed_destination")
    return value


@contextmanager
def selected_inputs(candidates):
    require(1 <= len(candidates) <= 2, "one_or_two_native_candidates_required")
    with tempfile.TemporaryDirectory(prefix="mike-publication-") as temporary:
        identities, paths = [], {}
        for index, candidate in enumerate(candidates):
            snapshot = Path(temporary) / str(index)
            snapshot.mkdir(mode=0o700)
            manifest = snapshot_inputs(candidate.absolute(), snapshot)
            # Reject duplicate JSON keys instead of accepting the last one.
            manifest, _ = read_json(snapshot / "manifest.json")
            identity = candidate_identity(snapshot, manifest)
            identities.append(identity)
            for item in identity["files"]:
                require(item["name"] not in paths, "duplicate_target_or_asset")
                local_name = "manifest.json" if item["name"].startswith("manifest-") else item["name"]
                paths[item["name"]] = snapshot / local_name
        require(len({(i["version"], i["source_commit"]) for i in identities}) == 1, "mixed_version_or_source")
        yield sorted(identities, key=lambda i: i["target"]), paths
