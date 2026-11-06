#!/usr/bin/env python3
"""Offline, synthetic formula and tap contracts. No candidate or brew execution."""

import contextlib
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import homebrew
from common import ReleaseError
from homebrew_engine import checked_tree, preflight, publish
from homebrew_fixture import FakeTap, HomebrewFixture
from homebrew_proposal import (TARGET, homebrew_qualification, inspect_archives, proposal,
                               tap_contract, validate_proposal, write_proposal)
from publication_engine import RemoteFailure, make_plan as archive_plan, release_spec
from publication_fixture import FakeGitHub, Fixture
from publication_gates import check_qualification
from publication_inputs import canonical, digest, selected_inputs


class HomebrewTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-homebrew-test-")
        self.addCleanup(temporary.cleanup)
        self.fixture = HomebrewFixture(Path(temporary.name))
        self.archives, self.tap = self.fixture.archives(), FakeTap(self.fixture.tap)

    def publish(self):
        plan = self.fixture.tap_plan()
        return publish(plan, self.archives, self.tap, digest(plan), self.fixture.tap["repository"],
                       self.fixture.request["repository"])

    def assertBlockedWithoutWrite(self, callback):
        with self.assertRaises((ReleaseError, RemoteFailure)):
            callback()
        self.assertEqual([], self.tap.writes)
        self.assertEqual([], self.archives.writes)

    def run_cli(self, args, online=False):
        def forbidden(_):
            self.fail("offline action initialized a network transport")
        with contextlib.redirect_stdout(io.StringIO()) as output, patch("subprocess.run", side_effect=AssertionError("execution")):
            homebrew.run(args, (lambda _: self.archives) if online else forbidden,
                         (lambda _: self.tap) if online else forbidden)
        return json.loads(output.getvalue())

    def test_offline_propose_validate_are_deterministic_without_qualification(self):
        value = self.fixture.value
        for command in ("propose", "validate"):
            args = self.fixture.arguments(command)
            args.proposal = self.fixture.root / "another-proposal"
            args.qualification = args.homebrew_qualification = None
            output = self.run_cli(args)
            self.assertEqual("UNQUALIFIED_FORMULA_PROPOSAL", output["state"])
            self.assertFalse(output["qualified"])
            self.assertEqual("NOT_RUN", output["AC26"])
            self.assertEqual(digest(value), output["proposal_sha256"])
        for item in value["formulas"]:
            data = (args.proposal / item["path"]).read_bytes()
            self.assertEqual(item["content"].encode(), data)
            self.assertEqual(item["sha256"], hashlib.sha256(data).hexdigest())
            self.assertIn(item["archive_sha256"], item["content"])
            self.assertIn(self.fixture.identity["manifest_sha256"], item["content"])
            self.assertIn('depends_on arch: :arm64', item["content"])
            self.assertNotIn("systemctl", item["content"])
            self.assertNotIn("cargo", item["content"])
        self.assertIn('libexec.install "mike-maintenance", "mike-maintenance.pyz"', value["formulas"][0]["content"])

    def test_offline_plan_keeps_native_channel_observation_not_run(self):
        value = self.run_cli(self.fixture.arguments())
        self.assertEqual("OFFLINE_HOMEBREW_PLAN", value["state"])
        self.assertEqual("NOT_RUN", value["permission_preflight"])
        self.assertEqual({"AC26": "NOT_RUN", "native_channel_install": "NOT_RUN"}, value["plan"]["post_publication"])

    def test_missing_qualification_or_approval_stops_before_network(self):
        for key, value in (("qualification", None), ("homebrew_qualification", None),
                           ("confirm_tap", "other/homebrew-tap"), ("confirm_archives", "other/repo"),
                           ("approve_plan", "d" * 64)):
            args = self.fixture.arguments("publish")
            setattr(args, key, value)
            with self.subTest(key=key), self.assertRaises(ReleaseError):
                self.run_cli(args)

    def test_existing_output_refused_without_overwrite(self):
        args = self.fixture.arguments("publish")
        args.output = self.fixture.root / "retained.json"
        args.output.write_text("retain me")
        with self.assertRaises(ReleaseError):
            self.run_cli(args)
        self.assertEqual("retain me", args.output.read_text())

    def test_formula_or_proposal_mutation_and_extra_files_are_rejected(self):
        root = self.fixture.proposal_root
        for relative, data in (("Formula/mike.rb", b"unreviewed code"), ("proposal.json", b"{}"), ("extra", b"unknown")):
            path = root / relative
            saved = path.read_bytes() if path.exists() else None
            path.write_bytes(data)
            with self.subTest(path=relative), self.assertRaises(ReleaseError):
                validate_proposal(root, self.fixture.value)
            if saved is None:
                path.unlink()
            else:
                path.write_bytes(saved)

    def test_symlinks_fifo_and_oversized_formula_inputs_fail_closed(self):
        path = self.fixture.proposal_root / "Formula/mike.rb"
        path.unlink()
        path.symlink_to(self.fixture.proposal_root / "Formula/mike-mcp.rb")
        with self.assertRaises(ReleaseError):
            validate_proposal(self.fixture.proposal_root, self.fixture.value)
        path.unlink()
        os.mkfifo(path)
        with self.assertRaises(ReleaseError):
            validate_proposal(self.fixture.proposal_root, self.fixture.value)
        path.unlink()
        path.write_bytes(b"x" * 65537)
        with self.assertRaises(ReleaseError):
            validate_proposal(self.fixture.proposal_root, self.fixture.value)

    def test_outputs_cannot_replace_candidate_or_source_files(self):
        for root in (self.fixture.proposal_root, self.fixture.candidate / "output", Path(homebrew.__file__).parent / "output"):
            with self.subTest(path=root), self.assertRaises(ReleaseError):
                write_proposal(root, self.fixture.value, [self.fixture.candidate])

    def test_only_public_alpha_native_candidates_and_separate_tap_are_supported(self):
        for key, value in (("visibility", "private"), ("channels", ["homebrew"])):
            request = dict(self.fixture.request, **{key: value})
            with self.subTest(key=key), self.assertRaises(ReleaseError):
                proposal([self.fixture.identity], request, self.fixture.tap)
        with self.assertRaises(ReleaseError):
            proposal([dict(self.fixture.identity, target="x86_64-unknown-linux-gnu")], self.fixture.request, self.fixture.tap)
        with self.assertRaises(ReleaseError):
            proposal([self.fixture.identity], dict(self.fixture.request, repository=self.fixture.tap["repository"]), self.fixture.tap)

    def test_two_target_archive_release_binds_both_but_renders_native_pair_only(self):
        root = self.fixture.root / "linux"
        root.mkdir()
        linux = Fixture(root)
        with selected_inputs([linux.candidate, self.fixture.candidate]) as (identities, paths):
            value = proposal(identities, self.fixture.request, self.fixture.tap)
            inspect_archives(paths)
        self.assertEqual(2, len(value["archive_candidate_identities"]))
        self.assertEqual(TARGET, value["candidate_identity"]["target"])
        self.assertTrue(all(TARGET in item["archive"] for item in value["formulas"]))
        self.assertNotEqual(digest(self.fixture.value), digest(value))

    def test_two_target_release_survives_complete_cli_preflight_and_publication(self):
        root = self.fixture.root / "linux"
        root.mkdir()
        linux = Fixture(root)
        candidates = [linux.candidate, self.fixture.candidate]
        with selected_inputs(candidates) as (identities, paths):
            cases = linux.qualification["target_cases"][linux.identity["target"]]
            cases["AC23"]["predecessor_kind"] = "published"
            self.fixture.qualification["candidate_identities"] = identities
            self.fixture.qualification["target_cases"].update(linux.qualification["target_cases"])
            self.fixture.save()
            fingerprint = check_qualification(self.fixture.qualification_file, self.fixture.request, identities)
            plan = archive_plan(self.fixture.request, identities, fingerprint)
            self.archives = FakeGitHub(self.fixture.request)
            self.archives.current = dict(release_spec(plan), id=17, draft=False)
            self.archives.contents = {name: path.read_bytes() for name, path in paths.items()}
            value = proposal(identities, self.fixture.request, self.fixture.tap)
        proposal_root = self.fixture.root / "two-target-proposal"
        write_proposal(proposal_root, value, candidates)
        self.fixture.homebrew_report.update(proposal_sha256=digest(value), archive_qualification_sha256=fingerprint)
        self.fixture.save_homebrew()
        from argparse import Namespace
        args = Namespace(command="plan", candidate=candidates, destination=self.fixture.destination_file,
                         tap=self.fixture.tap_file, proposal=proposal_root, qualification=self.fixture.qualification_file,
                         homebrew_qualification=self.fixture.homebrew_file, output=None, approve_plan=None,
                         confirm_archives=self.fixture.request["repository"], confirm_tap=self.fixture.tap["repository"])
        result = self.run_cli(args)
        self.assertEqual(10, len(result["plan"]["archive_plan"]["assets"]))
        args.command, args.approve_plan = "publish", result["plan_sha256"]
        result = self.run_cli(args, online=True)
        self.assertEqual("tap_published_same_bytes", result["state"])
        self.assertTrue(result["published"])
        self.assertEqual("NOT_RUN", result["AC26"])
        self.assertEqual([], self.archives.writes)

    def test_invalid_tap_paths_baseline_branch_and_formula_pair_refused(self):
        for key, value in (("repository", "fixture/source"), ("repository", "../homebrew-other"),
                           ("branch", "../main"), ("branch", "heads/main.lock"), ("branch", "main..other"),
                           ("base_commit", "main"), ("formula_blobs", {}), ("minimum_macos", "unknown")):
            with self.subTest(key=key, value=value), self.assertRaises(ReleaseError):
                tap_contract(dict(self.fixture.tap, **{key: value}))

    def test_no_native_gate_can_be_skipped_or_forged_from_incomplete_output(self):
        original = copy.deepcopy(self.fixture.homebrew_report)
        fingerprint = check_qualification(self.fixture.qualification_file, self.fixture.request, [self.fixture.identity])
        for gate in original["gates"]:
            for status in (None, "NOT_RUN", "DEFERRED", "FAIL"):
                self.fixture.homebrew_report = copy.deepcopy(original)
                if status is None:
                    del self.fixture.homebrew_report["gates"][gate]
                else:
                    self.fixture.homebrew_report["gates"][gate]["status"] = status
                self.fixture.save_homebrew()
                with self.subTest(gate=gate, status=status), self.assertRaises(ReleaseError):
                    homebrew_qualification(self.fixture.homebrew_file, self.fixture.value, fingerprint)
        for key in ("proposal_sha256", "archive_qualification_sha256"):
            self.fixture.homebrew_report = dict(original, **{key: "e" * 64})
            self.fixture.save_homebrew()
            with self.assertRaises(ReleaseError):
                homebrew_qualification(self.fixture.homebrew_file, self.fixture.value, fingerprint)

    def test_native_policy_and_real_channel_upgrade_scope_cannot_be_omitted(self):
        original = copy.deepcopy(self.fixture.homebrew_report)
        fingerprint = original["archive_qualification_sha256"]
        fields = {"AC08": ("channels", "both_binaries"),
                  "AC23": ("channel", "predecessor_kind", "data_retained", "interrupted_retry", "incompatible_downgrade_refused"),
                  "HOMEBREW_NATIVE": ("target", "minimum_macos", "clean_account", "runtime_dependencies_verified",
                                      "signature_policy_verified", "quarantine_policy_verified")}
        for gate, keys in fields.items():
            for key in keys:
                self.fixture.homebrew_report = copy.deepcopy(original)
                del self.fixture.homebrew_report["gates"][gate][key]
                self.fixture.save_homebrew()
                with self.subTest(gate=gate, key=key), self.assertRaises(ReleaseError):
                    homebrew_qualification(self.fixture.homebrew_file, self.fixture.value, fingerprint)

    def test_mutating_local_inputs_after_validation_cannot_change_uploaded_formula_bytes(self):
        args = self.fixture.arguments("publish")
        def mutate(_):
            (self.fixture.proposal_root / "Formula/mike.rb").write_text("unreviewed replacement")
            archive = self.fixture.candidate / self.fixture.manifest["artifacts"][0]["archive"]
            archive.write_bytes(b"replacement archive")
        self.tap.on_tree = mutate
        self.assertEqual("tap_published_same_bytes", self.run_cli(args, online=True)["state"])
        for item in self.fixture.value["formulas"]:
            self.assertEqual(item["content"].encode(), self.tap.blobs[item["git_blob_sha1"]])
        before = list(self.tap.writes)
        with self.assertRaises(ReleaseError):
            self.run_cli(args)
        self.assertEqual(before, self.tap.writes)

    def test_read_only_preflight_verifies_archives_and_tap_without_writes(self):
        value = self.run_cli(self.fixture.arguments("preflight"), online=True)
        self.assertEqual("tap_preflight_passed", value["state"])
        self.assertFalse(value["published"])
        self.assertEqual([], self.tap.writes)
        self.assertEqual([], self.archives.writes)
        self.assertIn("download", self.archives.reads)

    def test_exact_formula_pair_published_and_resumed_without_rewriting(self):
        result = self.publish()
        self.assertEqual("tap_published_same_bytes", result["status"])
        self.assertEqual(["tree", "commit", "advance"], self.tap.writes)
        self.assertEqual([], self.archives.writes)
        before = list(self.tap.writes)
        self.assertEqual("tap_already_published", self.publish()["status"])
        self.assertEqual(before, self.tap.writes)
        self.assertGreaterEqual(sum(r.startswith("blob:") for r in self.tap.reads), 6)

    def test_wrong_approvals_reject_before_remote_reads(self):
        plan = self.fixture.tap_plan()
        for approved, tap, archive in (("bad", self.fixture.tap["repository"], self.fixture.request["repository"]),
                                      (digest(plan), "other/homebrew-tap", self.fixture.request["repository"]),
                                      (digest(plan), self.fixture.tap["repository"], "other/archive")):
            self.assertBlockedWithoutWrite(lambda: publish(plan, self.archives, self.tap, approved, tap, archive))
        self.assertEqual([], self.tap.reads)
        self.assertEqual([], self.archives.reads)

    def test_repository_permissions_visibility_default_branch_and_missing_head_block(self):
        for key, value in (("permissions", {"push": False}), ("archived", True), ("disabled", True),
                           ("visibility", "private"), ("full_name", "other/homebrew-tap"), ("default_branch", "other")):
            old = copy.deepcopy(self.tap.repo)
            self.tap.repo[key] = value
            with self.subTest(key=key):
                self.assertBlockedWithoutWrite(self.publish)
            self.tap.repo = old
        self.tap.head = None
        self.assertBlockedWithoutWrite(self.publish)

    def test_unpublished_missing_or_changed_archive_blocks_without_tap_writes(self):
        original = copy.deepcopy(self.archives.current)
        for release in (None, dict(original, draft=True), dict(original, body="foreign")):
            self.archives.current = release
            self.assertBlockedWithoutWrite(self.publish)
        self.archives.current = original
        name = next(iter(self.archives.contents))
        self.archives.contents[name] = b"x" * len(self.archives.contents[name])
        self.assertBlockedWithoutWrite(self.publish)

    def test_changed_predecessor_or_truncated_base_tree_never_writes(self):
        tree = self.tap.trees[self.tap.commits[self.tap.head]["tree"]["sha"]]
        tree["truncated"] = True
        self.assertBlockedWithoutWrite(self.publish)
        tree["truncated"] = False
        tree["tree"][0]["sha"] = "e" * 40
        self.assertBlockedWithoutWrite(self.publish)

    def test_formula_path_directory_is_not_treated_as_an_absent_predecessor(self):
        tree_sha = self.tap.commits[self.tap.head]["tree"]["sha"]
        self.tap.trees[tree_sha]["tree"].extend([
            {"path": "Formula/mike-mcp.rb", "type": "tree", "mode": "040000", "sha": "e" * 40},
            {"path": "Formula/mike-mcp.rb/private.txt", "type": "blob", "mode": "100644", "sha": "f" * 40}])
        self.assertBlockedWithoutWrite(self.publish)

    def test_ambiguous_mutation_outcomes_stop_then_explicit_same_plan_resumes(self):
        for operation in ("tree", "commit", "advance"):
            for when in ("before", "after"):
                self.tap = FakeTap(self.fixture.tap)
                base = self.tap.head
                self.tap.fail = operation + "_" + when
                with self.subTest(operation=operation, when=when), self.assertRaises(RemoteFailure):
                    self.publish()
                self.assertEqual(1, self.tap.writes.count(operation))
                if operation != "advance" or when == "before":
                    self.assertEqual(base, self.tap.head)
                before = list(self.tap.writes)
                self.tap.fail = None
                result = self.publish()
                if operation == "advance" and when == "after":
                    self.assertEqual("tap_already_published", result["status"])
                    self.assertEqual(before, self.tap.writes)
                else:
                    self.assertEqual("tap_published_same_bytes", result["status"])

    def test_remote_formula_metadata_cannot_substitute_for_actual_bytes(self):
        self.tap.on_tree = lambda _: self.tap.blobs.update({key: b"changed" for key in self.tap.blobs})
        with self.assertRaisesRegex(ReleaseError, "remote_formula_bytes_changed"):
            self.publish()
        self.assertEqual(["tree"], self.tap.writes)

    def test_created_tree_unapproved_files_block_before_commit(self):
        self.tap.on_tree = lambda sha: self.tap.trees[sha]["tree"].append(
            {"path": "foreign.txt", "type": "blob", "mode": "100644", "sha": "e" * 40})
        with self.assertRaisesRegex(ReleaseError, "unapproved_files"):
            self.publish()
        self.assertEqual(["tree"], self.tap.writes)

    def test_permission_revocation_before_branch_advance_stops(self):
        self.tap.on_commit = lambda _: self.tap.repo["permissions"].update(push=False)
        with self.assertRaises(ReleaseError):
            self.publish()
        self.assertEqual(["tree", "commit"], self.tap.writes)

    def test_racing_branch_advance_is_never_forced_or_recreated(self):
        for head in (None, "e" * 40):
            self.tap = FakeTap(self.fixture.tap)
            self.tap.on_advance = lambda: setattr(self.tap, "head", head)
            with self.subTest(head=head), self.assertRaises(RemoteFailure):
                self.publish()
            self.assertEqual(head, self.tap.head)
            self.assertEqual(["tree", "commit", "advance"], self.tap.writes)

    def test_changed_published_commit_or_bytes_is_never_rewritten(self):
        self.publish()
        before = list(self.tap.writes)
        for key, value in (("message", "foreign"), ("parents", [{"sha": "e" * 40}])):
            commit = self.tap.commits[self.tap.head]
            saved = commit[key]
            commit[key] = value
            with self.subTest(key=key), self.assertRaises(ReleaseError):
                self.publish()
            commit[key] = saved
        sha = next(iter(self.tap.blobs))
        self.tap.blobs[sha] = b"modified after publication"
        with self.assertRaises(ReleaseError):
            self.publish()
        self.assertEqual(before, self.tap.writes)

    def test_unsafe_duplicate_or_empty_tree_paths_rejected(self):
        base = {"sha": "d" * 40, "truncated": False, "tree": []}
        for path in ("../bad", "/bad", "a//b", "a/./b", "a\\b"):
            value = dict(base, tree=[{"path": path, "type": "blob", "mode": "100644", "sha": "e" * 40}])
            with self.subTest(path=path), self.assertRaises(ReleaseError):
                checked_tree(value, base["sha"])
        value = dict(base, tree=[{"path": "empty", "type": "tree", "mode": "040000", "sha": "e" * 40}])
        with self.assertRaises(ReleaseError):
            checked_tree(value, base["sha"])


if __name__ == "__main__":
    unittest.main()
