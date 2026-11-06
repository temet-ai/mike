"""Deterministic, offline formula proposals from exact prepared Apple Silicon bytes."""

import hashlib
import json
from pathlib import Path
import re
import tempfile
from urllib.parse import quote

from archive import SUPPORT_FILES, extract_checked
from inputs import copy_regular
from publication_gates import POLICY, accepted
from publication_inputs import COMMIT, REPOSITORY, canonical, digest, read_json, require

TARGET = "aarch64-apple-darwin"
FORMULAE = {"mike": "Mike", "mike-mcp": "MikeMcp"}
MINIMUM_MACOS = ("big_sur", "monterey", "ventura", "sonoma", "sequoia", "tahoe")


def inspect_archives(paths):
    manifest, _ = read_json(paths["manifest-" + TARGET + ".json"])
    with tempfile.TemporaryDirectory(prefix="mike-homebrew-contents-") as temporary:
        for artifact in manifest["artifacts"]:
            name = artifact["binary"]
            if name == "mike":
                require(set(SUPPORT_FILES) <= {item["path"] for item in artifact["contents"]},
                        "homebrew_proposal_requires_complete_maintenance_kit")
            root = Path(temporary).resolve() / name
            root.mkdir(mode=0o700)
            extract_checked(paths[artifact["archive"]], root, name, artifact["contents"])


def git_blob(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def tap_contract(value):
    fields = {"schema_version", "kind", "repository", "branch", "base_commit", "formula_blobs", "minimum_macos"}
    require(set(value) == fields and type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["kind"] == "mike-homebrew-destination", "invalid_homebrew_destination")
    repository, branch = value["repository"], value["branch"]
    require(isinstance(repository, str) and REPOSITORY.fullmatch(repository)
            and repository.split("/")[1].startswith("homebrew-")
            and len(repository.split("/")[1]) > len("homebrew-"), "explicit_homebrew_tap_required")
    require(isinstance(branch, str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]*", branch)
            and not any(part in ("", ".", "..") or part.startswith(".") or part.endswith((".", ".lock"))
                        for part in branch.split("/")) and ".." not in branch, "invalid_tap_branch")
    require(isinstance(value["base_commit"], str) and COMMIT.fullmatch(value["base_commit"]), "invalid_tap_base_commit")
    expected = {"Formula/" + name + ".rb" for name in FORMULAE}
    blobs = value["formula_blobs"]
    require(isinstance(blobs, dict) and set(blobs) == expected, "exact_predecessor_formula_pair_required")
    require(all(sha is None or isinstance(sha, str) and COMMIT.fullmatch(sha) for sha in blobs.values()),
            "invalid_predecessor_formula_blob")
    require(value["minimum_macos"] in MINIMUM_MACOS, "reviewed_minimum_macos_required")
    return value


DESCRIPTIONS = {"mike": "Local Claude API reverse proxy and capture browser",
                "mike-mcp": "MCP stdio bridge for Mike capture inspection"}


def formula_methods(name, version):
    """The install, caveats and test methods every mike formula carries, for any platform."""
    lines = ["  def install", f'    bin.install "{name}"', '    pkgshare.install "LICENSE", "NOTICE.json"']
    if name == "mike":
        lines.append('    libexec.install "mike-maintenance", "mike-maintenance.pyz"')
        lines.append('    (share/"mike").install "share/mike/mods" if File.directory?("share/mike/mods")')
    lines.extend(["  end", "", "  def caveats", "    <<~EOS",
                  "      No service, database, shell profile or client registration is changed.",
                  "      Install both mike and mike-mcp formulae for the complete native pair."])
    if name == "mike":
        lines.extend(["      The optional libexec/mike-maintenance kit needs Bash and Python 3.11+.",
                      "      It manages separate archive-owned prefixes; use brew to update/remove this formula."])
    lines.extend(["    EOS", "  end", "", "  test do",
                  f'    assert_match "{name} {version}", shell_output("#{{bin}}/{name} --version")'])
    lines.append('    system "#{bin}/mike", "--help"' if name == "mike" else
                 '    system "#{bin}/mike-mcp", "list-tools", "--json"')
    lines.append("  end")
    return lines


def render_formula(name, identity, destination, minimum):
    archive = f"{name}-v{identity['version']}-{TARGET}.tar.gz"
    matches = [item for item in identity["files"] if item["name"] == archive]
    require(len(matches) == 1, "native_archive_missing")
    artifact = matches[0]
    url = ("https://github.com/" + destination["repository"] + "/releases/download/"
           + quote(destination["tag"], safe="") + "/" + quote(archive, safe=""))
    lines = ["# Deterministic proposal; review and qualify before updating a tap.",
             "# Source: " + identity["source_commit"], "# Candidate manifest SHA-256: " + identity["manifest_sha256"],
             f"class {FORMULAE[name]} < Formula", f"  desc {json.dumps(DESCRIPTIONS[name])}",
             '  homepage "https://github.com/' + destination["source_repository"] + '"',
             f"  url {json.dumps(url)}", f'  version "{identity["version"]}"',
             f'  sha256 "{artifact["sha256"]}"', '  license "MIT"', "",
             f"  depends_on macos: :{minimum}", "  depends_on arch: :arm64", "",
             *formula_methods(name, identity["version"]), "end", ""]
    content = "\n".join(lines)
    return {"path": "Formula/" + name + ".rb", "binary": name, "archive": archive,
            "archive_sha256": artifact["sha256"], "content": content,
            "sha256": hashlib.sha256(content.encode()).hexdigest(), "git_blob_sha1": git_blob(content.encode())}


def proposal(identities, destination, tap):
    native = [identity for identity in identities if identity["target"] == TARGET]
    require(len(native) == 1, "homebrew_requires_one_native_apple_silicon_candidate")
    require(destination["visibility"] == "public"
            and destination["channels"] == ["archive"], "homebrew_requires_public_alpha_archive_destination")
    tap_contract(tap)
    require(tap["repository"].lower() not in (destination["repository"].lower(), destination["source_repository"].lower()),
            "tap_must_be_a_separate_reviewed_repository")
    return {"schema_version": 1, "kind": "mike-homebrew-proposal", "state": "UNQUALIFIED_FORMULA_PROPOSAL",
            "qualified": False, "candidate_identity": native[0], "archive_candidate_identities": identities,
            "archive_destination": destination,
            "tap": tap, "formulas": [render_formula(name, native[0], destination, tap["minimum_macos"])
                                    for name in FORMULAE],
            "remaining": {"native_brew_audit": "NOT_RUN", "native_install_upgrade": "NOT_RUN", "AC26": "NOT_RUN"}}


def write_proposal(root, value, candidates):
    root = root.absolute()
    forbidden = [Path(__file__).resolve().parents[2], *(path.resolve() for path in candidates)]
    require(not any(root.resolve() == path or path in root.resolve().parents for path in forbidden),
            "proposal_output_must_be_outside_source_and_candidates")
    require(root.parent.is_dir() and not root.exists() and not root.is_symlink(), "new_proposal_directory_required")
    root.mkdir(mode=0o700)
    (root / "Formula").mkdir(mode=0o700)
    for item in value["formulas"]:
        (root / item["path"]).write_text(item["content"])
        (root / item["path"]).chmod(0o600)
    # Commit marker last: partial output is never a complete validated proposal.
    (root / "proposal.json").write_bytes(canonical(value))
    (root / "proposal.json").chmod(0o600)


def validate_proposal(root, expected):
    require(root.is_dir() and not root.is_symlink(), "plain_proposal_directory_required")
    paths = {"Formula", "proposal.json", *(item["path"] for item in expected["formulas"])}
    actual = {path.relative_to(root).as_posix() for path in root.rglob("*")}
    require(actual == paths and not any(path.is_symlink() for path in root.rglob("*")), "proposal_file_inventory_changed")
    value, _ = read_json(root / "proposal.json")
    require(value == expected, "proposal_identity_changed")
    with tempfile.TemporaryDirectory(prefix="mike-homebrew-formulas-") as temporary:
        for item in expected["formulas"]:
            snapshot = Path(temporary) / (item["binary"] + ".rb")
            copy_regular(root / item["path"], snapshot, 65536)
            require(snapshot.read_bytes() == item["content"].encode(), "formula_bytes_changed")
    return digest(expected)


def homebrew_qualification(path, value, archive_fingerprint):
    report, fingerprint = read_json(path)
    require(type(report.get("schema_version")) is int and report["schema_version"] == 1
            and report.get("kind") == "mike-homebrew-qualification" and report.get("state") == "APPROVED"
            and report.get("qualified") is True and report.get("policy") == POLICY, "homebrew_qualification_required")
    require(report.get("proposal_sha256") == digest(value)
            and report.get("archive_qualification_sha256") == archive_fingerprint,
            "homebrew_qualification_identity_mismatch")
    gates = report.get("gates", {})
    require(isinstance(gates, dict), "invalid_homebrew_gate_map")
    for name in ("PUBLIC01", "HOMEBREW_AUDIT", "HOMEBREW_NATIVE", "AC08", "AC23", "AC25"):
        require(name in gates, "missing_homebrew_" + name)
        accepted(gates[name], path.parent)
    require(gates["AC08"].get("channels") == ["homebrew"]
            and gates["AC08"].get("both_binaries") is True, "homebrew_pair_install_not_qualified")
    require(gates["AC23"].get("predecessor_kind") == "published"
            and gates["AC23"].get("channel") == "homebrew"
            and all(gates["AC23"].get(key) is True for key in
                    ("data_retained", "interrupted_retry", "incompatible_downgrade_refused")),
            "homebrew_upgrade_not_qualified")
    require(gates["HOMEBREW_NATIVE"].get("target") == TARGET
            and gates["HOMEBREW_NATIVE"].get("minimum_macos") == value["tap"]["minimum_macos"],
            "homebrew_native_baseline_not_qualified")
    require(all(gates["HOMEBREW_NATIVE"].get(key) is True for key in
                ("clean_account", "runtime_dependencies_verified", "signature_policy_verified",
                 "quarantine_policy_verified")), "homebrew_native_policy_not_qualified")
    return fingerprint
