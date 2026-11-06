"""Synthetic offline notice claims; fixtures never establish real distribution rights."""

import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from common import ReleaseError, spa_inventory
from notice_inputs import (BASE, MAX_NOTICE_BYTES, MAX_TEXT_BYTES, candidate_binding, encoded,
                           prepare_notice, template, verify_notice)
from notice_inventory import (canonical, digest, npm_subjects, read_bounded, read_json,
                              source_inventory, subject, validate_inventory)


class NoticeInputTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-notice-test-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / "source"
        self.root.mkdir()
        (self.root / "frontend/build").mkdir(parents=True)
        (self.root / ".gitignore").write_text("frontend/build/\n")
        (self.root / "Cargo.lock").write_text('''version = 4
[[package]]
name = "mike"
version = "0.1.0"
[[package]]
name = "synthetic-dependency"
version = "1.2.3"
source = "registry+https://example.invalid/index"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
''')
        (self.root / "frontend/pnpm-lock.yaml").write_text('''lockfileVersion: '9.0'
packages:
  '@fixture/font@2.0.0':
    resolution: {integrity: sha512-SYNTHETIC}
  script@1.0.0:
    resolution: {integrity: sha512-SYNTHETIC2}
snapshots:
  script@1.0.0: {}
''')
        (self.root / "frontend/build/index.html").write_text('<script src="app.js"></script>')
        (self.root / "frontend/build/app.js").write_text('console.log("fixture");')
        (self.root / "frontend/build/font.woff2").write_bytes(b"synthetic font bytes")
        self.git("init", "-q")
        self.git("config", "user.name", "Synthetic notice fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("add", ".")
        self.git("commit", "-qm", "Synthetic notice fixture")
        self.commit = self.git("rev-parse", "HEAD")
        self.assets = spa_inventory(self.root)
        self.target = "x86_64-unknown-linux-gnu"
        self.bundle = self.base / "review"
        self.bundle.mkdir()
        self.path = self.bundle / "review.json"
        self.text = "Synthetic notice, not a real license. Café.\r\n".encode()
        self.text_path = self.bundle / "license.txt"
        self.text_path.write_bytes(self.text)
        self.value = template(self.root, self.commit, "0.1.0", self.target, self.assets)
        self.value["review"] = {"status": "REVIEWED", "reviewer": "Synthetic fixture reviewer",
                                "reviewed_at": "2026-10-13T00:00:00Z", "evidence_reference": "fixture:review"}
        for item in self.value["reviews"]:
            item.update(status="REVIEWED", disposition="notice_required", binaries=["mike", "mike-mcp"],
                        notice_ids=["fixture-license"], license_expression="LicenseRef-Synthetic",
                        upstream="https://example.invalid/synthetic", basis="Synthetic test assertion only",
                        evidence_reference="fixture:subject-review")
        self.value["texts"] = [{"id": "fixture-license", "path": "license.txt", "sha256": digest(self.text),
                                 "size": len(self.text)}]
        self.save()
        self.candidate = {"source_commit": self.commit, "version": "0.1.0", "target": self.target,
                          "spa_source_assets": self.assets, "build": {
                              "cargo_lock_sha256": self.value["binding"]["cargo_lock_sha256"],
                              "pnpm_lock_sha256": self.value["binding"]["pnpm_lock_sha256"]}}

    def git(self, *args):
        return subprocess.check_output(["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false",
                                        "-C", str(self.root), *args], stderr=subprocess.DEVNULL,
                                       env=dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)).decode().strip()

    def save(self, value=None):
        self.path.write_bytes(encoded(self.value if value is None else value))

    def prepare(self):
        return prepare_notice(self.root, self.path, self.commit, "0.1.0", self.target, self.assets)

    def assert_rejected(self, mutate):
        value = copy.deepcopy(self.value)
        mutate(value)
        self.save(value)
        with self.assertRaises(ReleaseError):
            self.prepare()

    def test_predecessor_placeholder_is_unchanged_and_unreviewed(self):
        with patch("notice_inputs.source_inventory", side_effect=AssertionError("must not run")):
            data = prepare_notice(None, None, None, None, None, None)
        self.assertEqual(read_json(data), BASE)
        self.assertEqual(verify_notice(data, {}), "not_run")

    def test_package_preserves_exact_utf8_and_hashes_with_unverified_claims(self):
        data = self.prepare()
        self.assertEqual(data, self.prepare())
        self.assertEqual(verify_notice(data, self.candidate), "recorded_unverified")
        package = read_json(data)["reviewed_notices"]
        self.assertEqual(package["text_payloads"][0]["text"].encode(), self.text)
        self.assertEqual(package["manifest"], self.value)
        self.assertEqual(package["manifest_sha256"], digest(canonical(self.value)))
        self.assertEqual(package["human_review_claims"], "UNVERIFIED")
        self.assertEqual(package["rights_approval"], "NOT_RUN")
        self.assertIs(package["publication_authorized"], False)
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_template_has_every_lockfile_package_and_built_asset_but_no_claims(self):
        value = template(self.root, self.commit, "0.1.0", self.target, self.assets)
        self.assertEqual(len(value["inventory"]), 7)
        self.assertEqual(value["review"]["status"], "NOT_RUN")
        self.assertEqual(value["review"]["reviewer"], "")
        self.assertTrue(all(item["status"] == "NOT_RUN" for item in value["reviews"]))
        self.save(value)
        with self.assertRaisesRegex(ReleaseError, "unreviewed"):
            self.prepare()

    def test_missing_extra_duplicate_or_reordered_reviews_fail_closed(self):
        for mutate in (lambda v: v["reviews"].pop(), lambda v: v["reviews"].append(v["reviews"][0]),
                       lambda v: v["reviews"].__setitem__(1, v["reviews"][0]),
                       lambda v: v["reviews"].reverse()):
            with self.subTest(mutation=mutate):
                self.assert_rejected(mutate)

    def test_missing_extra_or_changed_inventory_cannot_be_rehashed_into_acceptance(self):
        for mutate in (lambda v: v["inventory"].pop(), lambda v: v["inventory"].append(v["inventory"][0]),
                       lambda v: v["inventory"][0].update(source="foreign")):
            value = copy.deepcopy(self.value)
            mutate(value)
            value["inventory_sha256"] = digest(canonical(value["inventory"]))
            self.save(value)
            with self.assertRaises(ReleaseError):
                self.prepare()

    def test_candidate_source_version_target_lock_and_spa_changes_fail(self):
        for field, replacement in (("source_commit", "a" * 40), ("version", "0.2.0"),
                                   ("target", "aarch64-apple-darwin"), ("cargo_lock_sha256", "b" * 64),
                                   ("pnpm_lock_sha256", "c" * 64), ("spa_inventory_sha256", "d" * 64)):
            with self.subTest(field=field):
                self.assert_rejected(lambda v: v["binding"].update({field: replacement}))

    def test_required_and_individual_human_claims_are_not_inferred(self):
        for mutate in (lambda v: v["review"].update(status="NOT_RUN"),
                       lambda v: v["review"].update(reviewer=""),
                       lambda v: v["review"].update(evidence_reference=""),
                       lambda v: v["review"].update(reviewed_at="2026-99-13T00:00:00Z"),
                       lambda v: v["reviews"][0].update(status="NOT_RUN"),
                       lambda v: v["reviews"][0].update(basis=""),
                       lambda v: v["reviews"][0].update(upstream=""),
                       lambda v: v["reviews"][0].update(notice_ids=[]),
                       lambda v: v["reviews"][0].update(binaries=[]),
                       lambda v: v.update(publication_authorized=True)):
            with self.subTest(mutation=mutate):
                self.assert_rejected(mutate)

    def test_excluded_and_no_notice_required_entries_need_explicit_review_basis(self):
        self.value["reviews"][0].update(disposition="not_shipped", binaries=[], notice_ids=[])
        self.value["reviews"][1].update(disposition="no_notice_required", notice_ids=[])
        self.save()
        self.assertEqual(verify_notice(self.prepare(), self.candidate), "recorded_unverified")
        self.assert_rejected(lambda v: v["reviews"][0].update(binaries=["mike"]))
        self.assert_rejected(lambda v: v["reviews"][1].update(binaries=[]))

    def test_changed_text_and_declared_text_hash_size_fail(self):
        self.text_path.write_bytes(self.text + b"changed")
        with self.assertRaisesRegex(ReleaseError, "hash or size"):
            self.prepare()
        self.text_path.write_bytes(self.text)
        self.assert_rejected(lambda v: v["texts"][0].update(sha256="a" * 64))
        self.assert_rejected(lambda v: v["texts"][0].update(size=len(self.text) - 1))

    def test_duplicate_missing_extra_and_orphaned_notice_texts_fail(self):
        for mutate in (lambda v: v.update(texts=[]),
                       lambda v: v["texts"].append(v["texts"][0]),
                       lambda v: v["reviews"][0].update(notice_ids=["absent"]),
                       lambda v: [i.update(disposition="no_notice_required", notice_ids=[]) for i in v["reviews"]]):
            with self.subTest(mutation=mutate):
                self.assert_rejected(mutate)

    def test_unsafe_relative_absolute_and_nested_text_paths_are_rejected(self):
        for path in ("../outside.txt", "/tmp/outside.txt", "dir/license.txt", ".", "..", "a//b", "a\\b", "a/../b", ".git"):
            with self.subTest(path=path):
                self.assert_rejected(lambda v: v["texts"][0].update(path=path))

    def test_symlink_manifest_text_and_ancestor_are_rejected(self):
        outside = self.base / "outside.txt"
        outside.write_bytes(self.text)
        self.text_path.unlink()
        self.text_path.symlink_to(outside)
        with self.assertRaises(ReleaseError):
            self.prepare()
        self.text_path.unlink()
        self.text_path.write_bytes(self.text)
        linked = self.base / "linked"
        linked.symlink_to(self.bundle, target_is_directory=True)
        with self.assertRaises(ReleaseError):
            prepare_notice(self.root, linked / "review.json", self.commit, "0.1.0", self.target, self.assets)
        original = self.path.read_bytes()
        self.path.unlink()
        outside.write_bytes(original)
        self.path.symlink_to(outside)
        with self.assertRaises(ReleaseError):
            self.prepare()

    def test_fifo_and_hardlinked_texts_are_rejected_without_blocking(self):
        self.text_path.unlink()
        os.mkfifo(self.text_path)
        with self.assertRaises(ReleaseError):
            self.prepare()
        self.text_path.unlink()
        self.text_path.write_bytes(self.text)
        os.link(self.text_path, self.bundle / "alias.txt")
        with self.assertRaises(ReleaseError):
            self.prepare()

    def test_invalid_utf8_nul_and_empty_texts_fail_even_with_matching_hash(self):
        for raw in (b"\xff\xfe", b"notice\0private", b"  \n"):
            self.text_path.write_bytes(raw)
            self.value["texts"][0].update(size=len(raw), sha256=digest(raw))
            self.save()
            with self.assertRaises(ReleaseError):
                self.prepare()

    def test_input_and_text_limits_fail_before_unbounded_reads(self):
        self.assert_rejected(lambda v: v["texts"][0].update(size=MAX_TEXT_BYTES + 1))
        self.text_path.write_bytes(b"x" * (MAX_TEXT_BYTES + 1))
        self.save()
        with self.assertRaisesRegex(ReleaseError, "byte limit"):
            self.prepare()
        with self.assertRaisesRegex(ReleaseError, "byte limit"):
            read_bounded(self.path, 10)
        with patch("notice_inputs.MAX_TOTAL_TEXT_BYTES", 1), self.assertRaisesRegex(ReleaseError, "total notice"):
            self.prepare()
        with patch("notice_inputs.MAX_NOTICE_BYTES", 1), self.assertRaisesRegex(ReleaseError, "packaged notice"):
            encoded(BASE)

    def test_json_duplicate_keys_nonfinite_numbers_unknown_fields_and_wrong_types_fail(self):
        for raw in (b'{"x":1,"x":2}', b'{"x":NaN}', b'[] trailing', b'\xff'):
            with self.assertRaises(ReleaseError):
                read_json(raw)
        for mutate in (lambda v: v.update(unknown="surprise"), lambda v: v.update(schema_version=True),
                       lambda v: v["reviews"][0].update(binaries=[{}]),
                       lambda v: v["texts"][0].update(size=True)):
            self.assert_rejected(mutate)

    def test_changed_incomplete_extra_or_unsafe_spa_inventory_fails(self):
        self.assets.pop()
        with self.assertRaisesRegex(ReleaseError, "SPA inventory"):
            self.prepare()
        self.assets = spa_inventory(self.root)
        (self.root / "frontend/build/new.js").write_text("unexpected")
        with self.assertRaisesRegex(ReleaseError, "SPA inventory"):
            self.prepare()
        (self.root / "frontend/build/new.js").unlink()
        (self.root / "frontend/build/linked.js").symlink_to(self.text_path)
        with self.assertRaisesRegex(ReleaseError, "symlink"):
            self.prepare()

    def test_dirty_or_foreign_source_is_rejected(self):
        with self.assertRaisesRegex(ReleaseError, "source commit changed"):
            source_inventory(self.root, "a" * 40, "0.1.0", self.target, self.assets)
        with (self.root / "Cargo.lock").open("a") as output:
            output.write("\n# changed\n")
        with self.assertRaisesRegex(ReleaseError, "not clean"):
            self.prepare()

    def test_pnpm_rejects_unsupported_empty_duplicate_or_ambiguous_package_inventory(self):
        raw = (self.root / "frontend/pnpm-lock.yaml").read_bytes()
        for data in (raw.replace(b"9.0", b"8.0"), raw.replace(b"packages:", b"packages: {}"),
                     raw + b"\npackages:\n", raw.replace(b"script@1.0.0", b"script@file:local"),
                     b"lockfileVersion: '9.0'\npackages:\n  a@1.0.0:\n  a@1.0.0:\n"):
            with self.subTest(data=data), self.assertRaises(ReleaseError):
                npm_subjects(data)

    def test_archive_verifier_rejects_tampering_escalated_claims_and_mismatched_candidate(self):
        original = read_json(self.prepare())
        for mutate in (lambda v: v["reviewed_notices"].update(rights_approval="APPROVED"),
                       lambda v: v["reviewed_notices"].update(human_review_claims="VERIFIED"),
                       lambda v: v["reviewed_notices"].update(publication_authorized=True),
                       lambda v: v["reviewed_notices"].update(manifest_sha256="a" * 64),
                       lambda v: v["reviewed_notices"]["text_payloads"][0].update(text="replaced"),
                       lambda v: v["reviewed_notices"]["text_payloads"].append({"id": "x", "text": "extra"}),
                       lambda v: v.update(third_party_notice_review="passed")):
            value = copy.deepcopy(original)
            mutate(value)
            with self.subTest(mutation=mutate), self.assertRaises(ReleaseError):
                verify_notice(encoded(value), self.candidate)
        for field, replacement in (("source_commit", "b" * 40), ("version", "2.0.0"),
                                   ("target", "aarch64-apple-darwin")):
            candidate = dict(self.candidate, **{field: replacement})
            with self.subTest(field=field), self.assertRaises(ReleaseError):
                verify_notice(encoded(original), candidate)

    def test_package_record_provenance_is_bound_without_claiming_content_hashes(self):
        items = self.value["inventory"]
        dependency = next(i for i in items if i["name"] == "synthetic-dependency")
        self.assertEqual(dependency["source"], "registry+https://example.invalid/index")
        self.assertNotEqual(dependency["content_sha256"], "a" * 64)
        npm = next(i for i in items if i["name"] == "@fixture/font")
        self.assertEqual(npm["source"], "frontend/pnpm-lock.yaml#packages/@fixture/font@2.0.0")
        self.assertEqual(candidate_binding(self.candidate), self.value["binding"])


if __name__ == "__main__":
    unittest.main()
