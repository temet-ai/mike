"""Notice packaging/verification integration with synthetic tool and binary fixtures."""

import copy
import io
import json
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import unittest
from unittest.mock import patch

import archive
from common import ReleaseError, sha256, spa_inventory
from notice_inputs import MAX_NOTICE_BYTES, encoded, template
from notice_inventory import canonical, digest
import test_prepare


class NoticeArchiveTests(unittest.TestCase):
    def setUp(self):
        self.fixture = test_prepare.PreparationTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.fixture.write("frontend/pnpm-lock.yaml", """lockfileVersion: '9.0'
packages:
  synthetic@1.2.3:
    resolution: {integrity: sha512-SYNTHETIC}
snapshots:
  synthetic@1.2.3: {}
""")
        self.fixture.git("add", "frontend/pnpm-lock.yaml")
        self.fixture.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid",
                         "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
                         "commit", "-qm", "Synthetic notice package fixture")
        self.fixture.before = self.fixture.tracked_hashes()
        self.predecessor = self.fixture.candidate()
        self.original_files = {path.name: path.read_bytes() for path in self.predecessor.iterdir()}
        legacy = json.loads(self.original_files["manifest.json"])
        self.bundle = self.fixture.base / "notice-review"
        self.bundle.mkdir()
        self.path = self.bundle / "review.json"
        self.raw_text = b"Synthetic integration notice; not an actual license.\r\n"
        (self.bundle / "fixture.txt").write_bytes(self.raw_text)
        self.review = template(self.fixture.repo, legacy["source_commit"], legacy["version"],
                               legacy["target"], spa_inventory(self.fixture.repo))
        self.review["review"].update(status="REVIEWED", reviewer="Synthetic integration reviewer",
                                     reviewed_at="2026-10-13T00:00:00Z", evidence_reference="fixture:review")
        self.review["texts"] = [{"id": "fixture", "path": "fixture.txt", "sha256": digest(self.raw_text),
                                 "size": len(self.raw_text)}]
        for item in self.review["reviews"]:
            item.update(status="REVIEWED", disposition="notice_required", binaries=["mike", "mike-mcp"],
                        notice_ids=["fixture"], license_expression="LicenseRef-Synthetic",
                        upstream="https://example.invalid/fixture", basis="Synthetic fixture only",
                        evidence_reference="fixture:subject")
        self.path.write_bytes(encoded(self.review))
        self.candidate = self.prepare_reviewed()
        self.manifest = json.loads((self.candidate / "manifest.json").read_bytes())
        host = patch("archive.native_target", return_value=self.manifest["target"])
        host.start()
        self.addCleanup(host.stop)

    def prepare_reviewed(self):
        result = self.fixture.invoke("--reviewed-notices", self.path)
        return Path(next(line.removeprefix("Candidate: ") for line in result.stdout.splitlines()
                         if line.startswith("Candidate: ")))

    def save_manifest(self):
        (self.candidate / "manifest.json").write_bytes(encoded(self.manifest))

    def notice(self, binary):
        item = next(a for a in self.manifest["artifacts"] if a["binary"] == binary)
        with tarfile.open(self.candidate / item["archive"], "r:gz") as source:
            return source.extractfile("NOTICE.json").read()

    def rewrite_notice(self, binary, change):
        artifact = next(a for a in self.manifest["artifacts"] if a["binary"] == binary)
        path = self.candidate / artifact["archive"]
        with tarfile.open(path, "r:gz") as source:
            members = [(member, source.extractfile(member).read()) for member in source.getmembers()]
        with tarfile.open(path, "w:gz", format=tarfile.USTAR_FORMAT) as destination:
            for member, data in members:
                if member.name == "NOTICE.json":
                    value = json.loads(data)
                    change(value)
                    data = encoded(value)
                    member.size = len(data)
                    expected = next(i for i in artifact["contents"] if i["path"] == member.name)
                    expected.update(size=len(data), sha256=digest(data))
                destination.addfile(member, io.BytesIO(data))
        artifact["sha256"] = sha256(path)
        (self.candidate / (path.name + ".sha256")).write_text(f'{artifact["sha256"]}  {path.name}\n')
        self.save_manifest()

    def test_reviewed_archive_pair_remains_unqualified_and_preserves_predecessor(self):
        self.assertNotEqual(self.candidate, self.predecessor)
        self.assertEqual(self.prepare_reviewed(), self.candidate)
        observed = archive.verify_candidate(self.candidate)
        self.assertIs(observed["qualified"], False)
        self.assertEqual(observed["third_party_notice_inputs"], "recorded_unverified")
        self.assertEqual(observed["checks"]["third_party_notices"], "not_run")
        self.assertEqual(self.notice("mike"), self.notice("mike-mcp"))
        package = json.loads(self.notice("mike"))["reviewed_notices"]
        self.assertEqual(package["text_payloads"][0]["text"].encode(), self.raw_text)
        self.assertEqual(package["rights_approval"], "NOT_RUN")
        self.assertFalse(package["publication_authorized"])
        for name, original in self.original_files.items():
            self.assertEqual((self.predecessor / name).read_bytes(), original)
        self.assertEqual(archive.verify_candidate(self.predecessor)["third_party_notice_inputs"], "not_run")
        self.fixture.assert_unmodified()

    def test_corrupt_notice_is_rejected_before_its_binary_smoke_even_with_rehashed_archive(self):
        self.rewrite_notice("mike", lambda v: v["reviewed_notices"]["text_payloads"][0].update(text="tampered"))
        with patch("archive.smoke") as smoke, patch("archive.binary_identity") as identity:
            with self.assertRaisesRegex(ReleaseError, "text hash or size"):
                archive.verify_candidate(self.candidate)
            smoke.assert_not_called()
            identity.assert_not_called()

    def test_valid_individually_but_different_pair_notices_are_rejected(self):
        def change(value):
            package = value["reviewed_notices"]
            package["manifest"]["review"]["reviewer"] = "Another synthetic reviewer"
            package["manifest_sha256"] = digest(canonical(package["manifest"]))
        self.rewrite_notice("mike-mcp", change)
        with patch("archive.smoke", wraps=archive.smoke) as smoke:
            with self.assertRaisesRegex(ReleaseError, "different notice evidence"):
                archive.verify_candidate(self.candidate)
            self.assertTrue(all(call.args[0].name != "mike-mcp" for call in smoke.call_args_list))

    def test_unrelated_candidate_binding_rejected_before_smoke(self):
        for mutate in (lambda m: m.update(source_commit="c" * 40),
                       lambda m: m["build"].update(cargo_lock_sha256="a" * 64),
                       lambda m: m["build"].update(pnpm_lock_sha256="b" * 64),
                       lambda m: m["spa_source_assets"][0].update(sha256="d" * 64)):
            original = copy.deepcopy(self.manifest)
            mutate(self.manifest)
            self.save_manifest()
            with self.subTest(mutation=mutate), patch("archive.smoke") as smoke:
                with self.assertRaisesRegex(ReleaseError, "binding mismatch"):
                    archive.verify_candidate(self.candidate)
                smoke.assert_not_called()
            self.manifest = original
            self.save_manifest()

    def test_review_claim_cannot_change_qualification_or_manifest_status(self):
        for field, value in (("qualified", True), ("third_party_notice_inputs", "passed"),
                             ("third_party_notice_inputs", "not_run")):
            original = self.manifest[field]
            self.manifest[field] = value
            self.save_manifest()
            with self.subTest(field=field, value=value), patch("archive.smoke") as smoke:
                with self.assertRaises(ReleaseError):
                    archive.verify_candidate(self.candidate)
                smoke.assert_not_called()
            self.manifest[field] = original
        self.save_manifest()
        self.rewrite_notice("mike", lambda v: v["reviewed_notices"].update(rights_approval="APPROVED"))
        with patch("archive.smoke") as smoke, self.assertRaisesRegex(ReleaseError, "grant rights"):
            archive.verify_candidate(self.candidate)
        smoke.assert_not_called()

    def test_notice_inventory_limit_is_enforced_before_archive_extraction(self):
        for item in self.manifest["artifacts"][0]["contents"]:
            if item["path"] == "NOTICE.json":
                item["size"] = MAX_NOTICE_BYTES + 1
        self.save_manifest()
        with patch("archive.extract_members") as extract, patch("archive.smoke") as smoke:
            with self.assertRaisesRegex(ReleaseError, "invalid inventory entry: NOTICE.json"):
                archive.verify_candidate(self.candidate)
            extract.assert_not_called()
            smoke.assert_not_called()

    def test_stale_or_unreviewed_explicit_input_never_replaces_existing_candidates(self):
        for change in (lambda v: v["review"].update(status="NOT_RUN"),
                       lambda v: v["binding"].update(source_commit="a" * 40)):
            value = copy.deepcopy(self.review)
            change(value)
            self.path.write_bytes(encoded(value))
            result = self.fixture.invoke("--reviewed-notices", self.path, success=False)
            self.assertIn("third-party notices:", result.stderr)
            self.assertFalse((self.fixture.repo / "dist-out/.prepare.lock").exists())
            self.assertFalse(list((self.fixture.repo / "dist-out").glob(".prepare-*")))
            for name, original in self.original_files.items():
                self.assertEqual((self.predecessor / name).read_bytes(), original)
        self.fixture.invoke("--verify", self.candidate)

    def test_template_cli_writes_private_unreviewed_file_and_never_overwrites(self):
        output = self.bundle / "new-review.json"
        command = [sys.executable, str(self.fixture.repo / "scripts/release/notice_inputs.py"), "template",
                   "--repo", str(self.fixture.repo), "--source", self.manifest["source_commit"],
                   "--version", self.manifest["version"], "--target", self.manifest["target"], "--output", str(output)]
        completed = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        before = output.read_bytes()
        self.assertEqual(stat.S_IMODE(output.stat().st_mode), 0o600)
        value = json.loads(before)
        self.assertEqual(value["review"]["status"], "NOT_RUN")
        self.assertEqual(value["inventory"], self.review["inventory"])
        self.assertEqual(subprocess.run(command, capture_output=True).returncode, 1)
        self.assertEqual(output.read_bytes(), before)

    def test_verify_refuses_external_review_input_without_building(self):
        self.fixture.log.unlink()
        result = self.fixture.invoke("--verify", self.candidate, "--reviewed-notices", self.path, success=False)
        self.assertIn("not a version, target or review input", result.stderr)
        self.assertEqual(self.fixture.calls(), [])


if __name__ == "__main__":
    unittest.main()
