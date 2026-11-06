"""Synthetic tap state only; no native Homebrew or publication evidence."""

import argparse
import copy
import hashlib

from homebrew_engine import make_plan
from homebrew_proposal import TARGET, git_blob, proposal, write_proposal
from publication_engine import RemoteFailure, release_spec
from publication_fixture import FakeGitHub, Fixture
from publication_gates import POLICY, check_qualification
from publication_inputs import canonical, digest, selected_inputs


class HomebrewFixture(Fixture):
    def __init__(self, root):
        super().__init__(root, target=TARGET)
        cases = self.qualification["target_cases"][TARGET]
        cases["AC23"]["predecessor_kind"] = "published"
        self.save()
        self.tap = {"schema_version": 1, "kind": "mike-homebrew-destination", "repository": "fixture/homebrew-mike",
                    "branch": "main", "base_commit": "c" * 40, "minimum_macos": "ventura",
                    "formula_blobs": {"Formula/mike.rb": git_blob(b"old mike"), "Formula/mike-mcp.rb": None}}
        self.tap_file = root / "tap.json"
        self.tap_file.write_bytes(canonical(self.tap))
        self.value = proposal([self.identity], self.request, self.tap)
        self.proposal_root = root / "proposed"
        write_proposal(self.proposal_root, self.value, [self.candidate])
        self.homebrew_report = {"schema_version": 1, "kind": "mike-homebrew-qualification", "state": "APPROVED",
                               "qualified": True, "policy": POLICY, "proposal_sha256": digest(self.value),
                               "archive_qualification_sha256": check_qualification(self.qualification_file, self.request,
                                                                                   [self.identity]),
                               "gates": {name: copy.deepcopy(self.gate) for name in
                                         ("PUBLIC01", "HOMEBREW_AUDIT", "HOMEBREW_NATIVE", "AC08", "AC23", "AC25")}}
        gates = self.homebrew_report["gates"]
        gates["AC08"].update(channels=["homebrew"], both_binaries=True)
        gates["AC23"].update(predecessor_kind="published", channel="homebrew", data_retained=True,
                             interrupted_retry=True, incompatible_downgrade_refused=True)
        gates["HOMEBREW_NATIVE"].update(target=TARGET, minimum_macos=self.tap["minimum_macos"], clean_account=True,
                                        runtime_dependencies_verified=True, signature_policy_verified=True,
                                        quarantine_policy_verified=True)
        self.homebrew_file = root / "homebrew-qualification.json"
        self.save_homebrew()

    def save_homebrew(self):
        self.homebrew_file.write_bytes(canonical(self.homebrew_report))

    def tap_plan(self):
        from homebrew_proposal import homebrew_qualification
        archive_fingerprint = check_qualification(self.qualification_file, self.request, [self.identity])
        fingerprint = homebrew_qualification(self.homebrew_file, self.value, archive_fingerprint)
        return make_plan(self.value, self.plan(), fingerprint)

    def archives(self):
        remote = FakeGitHub(self.request)
        remote.current = dict(release_spec(self.plan()), id=17, draft=False)
        with selected_inputs([self.candidate]) as (_, paths):
            remote.contents = {name: path.read_bytes() for name, path in paths.items()}
        return remote

    def arguments(self, command="plan"):
        return argparse.Namespace(command=command, candidate=[self.candidate], destination=self.destination_file,
                                  tap=self.tap_file, proposal=self.proposal_root, qualification=self.qualification_file,
                                  homebrew_qualification=self.homebrew_file, output=None,
                                  approve_plan=digest(self.tap_plan()), confirm_archives=self.request["repository"],
                                  confirm_tap=self.tap["repository"])


class FakeTap:
    def __init__(self, tap):
        self.repo = {"full_name": tap["repository"], "visibility": "public", "default_branch": tap["branch"],
                     "archived": False, "disabled": False, "permissions": {"push": True}}
        self.head = tap["base_commit"]
        files = {"README.md": {"mode": "100644", "type": "blob", "sha": git_blob(b"untouched")},
                 **{path: {"mode": "100644", "type": "blob", "sha": sha}
                    for path, sha in tap["formula_blobs"].items() if sha}}
        self.trees, self.blobs, self.commits = {}, {}, {}
        tree = self.store_tree(files)
        self.commits[self.head] = {"sha": self.head, "tree": {"sha": tree}, "parents": [], "message": "base"}
        self.writes, self.reads = [], []
        self.fail, self.on_tree, self.on_commit, self.on_advance = None, None, None, None

    def store_tree(self, files):
        directories = {"/".join(path.split("/")[:n]) for path in files for n in range(1, len(path.split("/")))}
        entries = [{"path": path, **value} for path, value in sorted(files.items())]
        entries.extend({"path": path, "mode": "040000", "type": "tree", "sha": "d" * 40}
                       for path in sorted(directories))
        sha = hashlib.sha1(canonical(entries)).hexdigest()
        self.trees[sha] = {"sha": sha, "truncated": False, "tree": entries}
        return sha

    def repository(self):
        self.reads.append("repository")
        return copy.deepcopy(self.repo)

    def branch(self, name):
        self.reads.append("branch:" + name)
        return self.head

    def commit(self, sha):
        self.reads.append("commit:" + sha)
        return copy.deepcopy(self.commits[sha])

    def tree(self, sha):
        self.reads.append("tree:" + sha)
        return copy.deepcopy(self.trees[sha])

    def blob(self, sha):
        self.reads.append("blob:" + sha)
        return self.blobs[sha]

    def attempt(self, operation, when):
        if self.fail == operation + "_" + when:
            raise RemoteFailure("synthetic " + operation + " " + when)

    def create_tree(self, base, entries):
        self.writes.append("tree")
        self.attempt("tree", "before")
        files = {entry["path"]: {key: entry[key] for key in ("mode", "type", "sha")}
                 for entry in self.trees[base]["tree"] if entry["type"] != "tree"}
        for item in entries:
            data = item["content"].encode()
            sha = git_blob(data)
            self.blobs[sha] = data
            files[item["path"]] = {"mode": item["mode"], "type": item["type"], "sha": sha}
        result = self.store_tree(files)
        if self.on_tree:
            self.on_tree(result)
        self.attempt("tree", "after")
        return result

    def create_commit(self, tree, parent, message):
        self.writes.append("commit")
        self.attempt("commit", "before")
        value = {"tree": {"sha": tree}, "parents": [{"sha": parent}], "message": message}
        sha = hashlib.sha1(canonical(value)).hexdigest()
        self.commits[sha] = dict(value, sha=sha)
        if self.on_commit:
            self.on_commit(sha)
        self.attempt("commit", "after")
        return sha

    def advance_branch(self, branch, sha):
        self.writes.append("advance")
        self.attempt("advance", "before")
        if self.on_advance:
            self.on_advance()
        if self.head != self.commits[sha]["parents"][0]["sha"]:
            raise RemoteFailure("synthetic non-fast-forward or missing existing branch")
        self.head = sha
        self.attempt("advance", "after")
