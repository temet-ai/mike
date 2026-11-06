"""Synthetic publication fixtures only; never evidence for a Mike release."""

import copy
import hashlib
from pathlib import Path

from archive import pack
from common import sha256
from publication_engine import RemoteFailure, make_plan
from publication_gates import CASES, POLICY, check_qualification
from publication_inputs import candidate_identity, canonical

TARGET = "x86_64-unknown-linux-gnu"


class Fixture:
    def __init__(self, root, target=TARGET):
        self.root = root
        self.candidate = root / "candidate"
        self.candidate.mkdir()
        self.manifest = {"schema_version": 1, "state": "prepared", "qualified": False,
                         "version": "0.1.0-alpha.1", "target": target, "source_commit": "a" * 40, "artifacts": []}
        license_path = root / "LICENSE"
        license_path.write_text("Synthetic fixture, not release qualification\n")
        for name in ("mike", "mike-mcp"):
            executable = root / name
            executable.write_text("NOT AN EXECUTABLE; publication must never execute candidates\n" + name)
            archive = self.candidate / f"{name}-v0.1.0-alpha.1-{target}.tar.gz"
            contents = pack(archive, executable, license_path, b'{"fixture":true}', 0)
            fingerprint = sha256(archive)
            (self.candidate / (archive.name + ".sha256")).write_text(f"{fingerprint}  {archive.name}\n")
            self.manifest["artifacts"].append({"binary": name, "archive": archive.name,
                                              "sha256": fingerprint, "contents": contents})
        self.write_manifest()
        self.identity = candidate_identity(self.candidate, self.manifest)
        self.request = {"schema_version": 1, "kind": "mike-publication-destination",
                        "repository": "fixture/binary-downloads", "source_repository": "fixture/source",
                        "visibility": "public", "tag": "v0.1.0-alpha.1", "tag_commit": "b" * 40,
                        "source_exposure": False, "channels": ["archive"], "prerelease": True}
        evidence = root / "synthetic-evidence.json"
        evidence.write_bytes(canonical({"status": "PASS", "qualified": True,
                                        "warning": "SYNTHETIC TEST ONLY; NOT MIKE QUALIFICATION"}))
        self.gate = {"status": "PASS", "reviewed_by": "synthetic test reviewer",
                     "observed": "synthetic state machine assertion, not a real acceptance observation",
                     "evidence": [{"file": evidence.name, "sha256": sha256(evidence)}]}
        cases = {name: copy.deepcopy(self.gate) for name in CASES}
        cases["AC08"]["channels"] = ["archive"]
        cases["AC15"]["manual_restart"] = True
        cases["AC20"]["duration_seconds"] = 900
        cases["AC23"]["predecessor_kind"] = "internal"
        self.qualification = {"schema_version": 1, "kind": "mike-publication-qualification",
                              "state": "APPROVED", "qualified": True, "policy": POLICY,
                              "destination": self.request, "candidate_identities": [self.identity],
                              "gates": {name: copy.deepcopy(self.gate) for name in ("SG01", "SG02", "SG03", "PUBLIC01")},
                              "target_cases": {target: cases}}
        self.qualification["gates"]["PUBLIC01"]["binary_only_destination_and_generated_source_archives_reviewed"] = True
        self.destination_file, self.qualification_file = root / "destination.json", root / "qualification.json"
        self.save()

    def write_manifest(self):
        (self.candidate / "manifest.json").write_bytes(canonical(self.manifest))

    def save(self):
        self.destination_file.write_bytes(canonical(self.request))
        self.qualification_file.write_bytes(canonical(self.qualification))

    def plan(self):
        fingerprint = check_qualification(self.qualification_file, self.request, [self.identity])
        return make_plan(self.request, [self.identity], fingerprint)


class FakeGitHub:
    def __init__(self, request):
        self.request = request
        self.repo = {"full_name": request["repository"], "visibility": request["visibility"],
                     "permissions": {"push": True}, "archived": False, "disabled": False}
        self.tag = request["tag_commit"]
        self.current = None
        self.contents = {}
        self.writes = []
        self.reads = []
        self.fail = None
        self.on_upload = None

    def repository(self):
        self.reads.append("repository")
        return copy.deepcopy(self.repo)

    def tag_commit(self, tag):
        self.reads.append("tag")
        return self.tag

    def release(self, tag):
        self.reads.append("release")
        return copy.deepcopy(self.current)

    def assets(self, release_id):
        self.reads.append("assets")
        return [{"id": i + 1, "name": name, "state": "uploaded", "size": len(data)}
                for i, (name, data) in enumerate(sorted(self.contents.items()))]

    def asset_sha256(self, asset_id, expected_size):
        self.reads.append("download")
        return hashlib.sha256(sorted(self.contents.items())[asset_id - 1][1]).hexdigest()

    def create_draft(self, spec):
        self.writes.append("create")
        if self.fail == "create_before":
            raise RemoteFailure("synthetic create failure")
        self.current = dict(spec, id=17)
        if self.fail == "create_after":
            raise RemoteFailure("synthetic unknown create outcome")

    def upload(self, release_id, name, path, expected):
        self.writes.append("upload:" + name)
        if self.fail == "upload_before":
            raise RemoteFailure("synthetic upload failure")
        if name in self.contents:
            raise AssertionError("attempted asset overwrite")
        self.contents[name] = Path(path).read_bytes()
        if self.on_upload:
            self.on_upload()
        if self.fail == "upload_after":
            raise RemoteFailure("synthetic unknown upload outcome")

    def publish(self, release_id):
        self.writes.append("publish")
        if self.fail == "publish_before":
            raise RemoteFailure("synthetic publication failure")
        self.current["draft"] = False
        if self.fail == "publish_after":
            raise RemoteFailure("synthetic unknown publication outcome")
