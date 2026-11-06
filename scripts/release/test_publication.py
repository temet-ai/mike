#!/usr/bin/env python3
"""Synthetic offline publication contracts; no GitHub calls, builds or qualification."""

import argparse
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest

import publication
from common import ReleaseError
from publication_engine import RemoteFailure, digest, preflight, publish, release_spec
from publication_fixture import FakeGitHub, Fixture, TARGET
from publication_gates import check_qualification
from publication_inputs import destination, selected_inputs


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="mike-publication-test-")
        self.addCleanup(self.temporary.cleanup)
        self.fixture = Fixture(Path(self.temporary.name))
        self.remote = FakeGitHub(self.fixture.request)

    def publish(self, plan=None):
        plan = plan or self.fixture.plan()
        with selected_inputs([self.fixture.candidate]) as (_, paths):
            return publish(plan, paths, self.remote, digest(plan), self.fixture.request["repository"])

    def assertBlockedWithoutWrite(self, action):
        with self.assertRaises((ReleaseError, RemoteFailure)):
            action()
        self.assertEqual([], self.remote.writes)

    def test_publishes_only_exact_files_then_verifies_actual_downloads(self):
        result = self.publish()
        self.assertEqual("published_same_bytes", result["status"])
        self.assertEqual("NOT_RUN", result["AC26"])
        self.assertEqual("NOT_RUN", result["FEEDBACK01"])
        self.assertEqual(["create", *["upload:" + a["name"] for a in self.fixture.plan()["assets"]], "publish"], self.remote.writes)
        self.assertGreater(self.remote.reads.count("download"), 5)
        self.assertFalse(self.remote.current["draft"])
        self.assertEqual(self.fixture.manifest, json.loads(self.remote.contents[f"manifest-{TARGET}.json"]))

    def test_repeat_complete_publication_is_read_only(self):
        self.publish()
        before = list(self.remote.writes)
        self.assertEqual("already_published", self.publish()["status"])
        self.assertEqual(before, self.remote.writes)

    def test_read_only_preflight_never_creates_draft(self):
        result = preflight(self.fixture.plan(), self.remote)
        self.assertEqual(5, len(result["missing"]))
        self.assertEqual([], self.remote.writes)

    def test_denied_permission_blocks_before_draft(self):
        self.remote.repo["permissions"]["push"] = False
        self.assertBlockedWithoutWrite(self.publish)

    def test_missing_or_wrong_tag_never_creates_or_moves_tag(self):
        for value in (None, "c" * 40):
            with self.subTest(value=value):
                self.remote.tag = value
                self.assertBlockedWithoutWrite(self.publish)

    def test_repo_identity_visibility_archived_disabled_are_fail_closed(self):
        for key, value in (("full_name", "elsewhere/repository"), ("visibility", "private"),
                           ("archived", True), ("disabled", True), ("permissions", {})):
            with self.subTest(key=key):
                saved = copy.deepcopy(self.remote.repo)
                self.remote.repo[key] = value
                self.assertBlockedWithoutWrite(self.publish)
                self.remote.repo = saved

    def test_wrong_approval_rejected_before_any_remote_read(self):
        plan = self.fixture.plan()
        for approved, repo in (("c" * 64, self.fixture.request["repository"]), (digest(plan), "other/repo")):
            with selected_inputs([self.fixture.candidate]) as (_, paths):
                self.assertBlockedWithoutWrite(lambda: publish(plan, paths, self.remote, approved, repo))
        self.assertEqual([], self.remote.reads)

    def test_resume_unknown_create_outcome_reuses_draft(self):
        self.remote.fail = "create_after"
        with self.assertRaises(RemoteFailure):
            self.publish()
        self.remote.fail = None
        self.publish()
        self.assertEqual(1, self.remote.writes.count("create"))

    def test_failed_create_does_not_attempt_upload_or_publish(self):
        self.remote.fail = "create_before"
        with self.assertRaises(RemoteFailure):
            self.publish()
        self.assertEqual(["create"], self.remote.writes)

    def test_resume_failed_and_ambiguous_uploads_uses_same_bytes(self):
        for failure in ("upload_before", "upload_after"):
            with self.subTest(failure=failure):
                self.remote = FakeGitHub(self.fixture.request)
                self.remote.fail = failure
                with self.assertRaises(RemoteFailure):
                    self.publish()
                existing = copy.deepcopy(self.remote.contents)
                self.remote.fail = None
                self.publish()
                self.assertEqual(1, self.remote.writes.count("create"))
                for name, data in existing.items():
                    self.assertEqual(data, self.remote.contents[name])
                    self.assertEqual(1, self.remote.writes.count("upload:" + name))

    def test_resume_unknown_publish_outcome_does_not_republish(self):
        self.remote.fail = "publish_after"
        with self.assertRaises(RemoteFailure):
            self.publish()
        self.remote.fail = None
        self.assertEqual("already_published", self.publish()["status"])
        self.assertEqual(1, self.remote.writes.count("publish"))

    def test_resume_failure_before_publish_preserves_all_assets(self):
        self.remote.fail = "publish_before"
        with self.assertRaises(RemoteFailure):
            self.publish()
        existing = copy.deepcopy(self.remote.contents)
        self.remote.fail = None
        self.publish()
        self.assertEqual(existing, self.remote.contents)
        self.assertEqual(5, sum(w.startswith("upload:") for w in self.remote.writes))

    def test_conflicting_release_identity_is_never_adopted(self):
        for key, value in (("body", "unrelated release"), ("tag_name", "v9.0.0"),
                           ("prerelease", False), ("name", "someone else's release")):
            with self.subTest(key=key):
                self.remote.current = dict(release_spec(self.fixture.plan()), id=17)
                self.remote.current[key] = value
                self.assertBlockedWithoutWrite(self.publish)

    def test_unused_commitish_metadata_never_substitutes_for_existing_tag_commit(self):
        self.remote.current = dict(release_spec(self.fixture.plan()), id=17)
        self.remote.current["target_commitish"] = "main"
        self.assertEqual("preflight_passed", preflight(self.fixture.plan(), self.remote)["status"])
        self.remote.tag = "c" * 40
        self.assertBlockedWithoutWrite(self.publish)

    def test_changed_existing_asset_refuses_even_when_size_matches(self):
        plan = self.fixture.plan()
        self.remote.current = dict(release_spec(plan), id=17)
        asset = plan["assets"][0]
        self.remote.contents[asset["name"]] = b"x" * asset["size"]
        self.assertBlockedWithoutWrite(self.publish)

    def test_unexpected_asset_refuses_without_deleting(self):
        self.remote.current = dict(release_spec(self.fixture.plan()), id=17)
        self.remote.contents["unreviewed-file.txt"] = b"private"
        self.assertBlockedWithoutWrite(self.publish)
        self.assertIn("unreviewed-file.txt", self.remote.contents)

    def test_incomplete_published_release_never_gets_more_assets(self):
        self.remote.current = dict(release_spec(self.fixture.plan()), id=17)
        self.remote.current["draft"] = False
        self.assertBlockedWithoutWrite(self.publish)

    def test_permission_revocation_after_one_upload_stops_immediately(self):
        self.remote.on_upload = lambda: self.remote.repo["permissions"].update(push=False)
        with self.assertRaises(ReleaseError):
            self.publish()
        self.assertEqual(2, len(self.remote.writes))
        self.assertTrue(self.remote.current["draft"])

    def test_tag_change_after_upload_stops_without_rewriting_it(self):
        self.remote.on_upload = lambda: setattr(self.remote, "tag", "c" * 40)
        with self.assertRaises(ReleaseError):
            self.publish()
        self.assertNotIn("publish", self.remote.writes)
        self.assertEqual("c" * 40, self.remote.tag)

    def test_original_candidate_change_cannot_change_snapshot_uploads(self):
        changed = self.fixture.candidate / self.fixture.manifest["artifacts"][0]["archive"]
        self.remote.on_upload = lambda: changed.write_bytes(b"replaced after snapshot")
        self.publish()
        self.assertNotIn(b"replaced after snapshot", self.remote.contents.values())
        with self.assertRaises(ReleaseError):
            self.publish()

    def test_current_acceptance_and_security_reports_cannot_authorize_network(self):
        from acceptance import new_report
        from security_review import report
        from security_common import Scanner
        for value in (new_report(), report(Scanner())):
            with self.subTest(value=value.get("runner", value.get("tool"))):
                self.fixture.qualification_file.write_text(json.dumps(value))
                args = self.arguments("publish")
                called = []
                with self.assertRaises(ReleaseError):
                    publication.run(args, lambda repo: called.append(repo))
                self.assertEqual([], called)

    def arguments(self, command="plan"):
        return argparse.Namespace(command=command, candidate=[self.fixture.candidate],
                                  qualification=self.fixture.qualification_file, destination=self.fixture.destination_file,
                                  output=None, approve_plan="bad", confirm_repository=self.fixture.request["repository"])

    def test_offline_plan_never_initializes_transport(self):
        def forbidden(_):
            self.fail("offline plan attempted to initialize network")
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(0, publication.run(self.arguments(), forbidden))
        value = json.loads(output.getvalue())
        self.assertEqual("OFFLINE_PLAN", value["state"])
        self.assertFalse(value["published"])
        self.assertFalse(value["distribution_qualified"])
        self.assertEqual("NOT_RUN", value["permission_preflight"])

    def test_inspect_needs_no_qualification_and_never_initializes_transport(self):
        args = self.arguments("inspect")
        args.qualification = None
        args.destination = None
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            publication.run(args, lambda _: self.fail("network initialized"))
        result = json.loads(output.getvalue())
        self.assertEqual("UNQUALIFIED_INVENTORY", result["state"])
        self.assertFalse(result["qualified"])
        self.assertEqual([self.fixture.identity], result["candidate_identities"])

    def test_existing_output_refused_before_network_or_overwrite(self):
        args = self.arguments("publish")
        args.approve_plan = digest(self.fixture.plan())
        args.output = self.fixture.root / "already-exists.json"
        args.output.write_text("do not overwrite")
        with self.assertRaises(ReleaseError):
            publication.run(args, lambda _: self.fail("network initialized"))
        self.assertEqual("do not overwrite", args.output.read_text())

    def test_duplicate_json_keys_and_nonfinite_numbers_rejected(self):
        from publication_inputs import read_json
        for value in ('{"status":"PASS","status":"NOT_RUN"}', '{"duration":Infinity}', '{"schema_version":NaN}'):
            self.fixture.qualification_file.write_text(value)
            with self.subTest(value=value), self.assertRaises(ReleaseError):
                read_json(self.fixture.qualification_file)

    def test_evidence_cannot_escape_review_bundle(self):
        for filename in ("../outside.json", "/absolute/evidence.json"):
            self.fixture.qualification["gates"]["SG01"]["evidence"][0]["file"] = filename
            self.fixture.save()
            with self.subTest(filename=filename), self.assertRaises(ReleaseError):
                self.fixture.plan()

    def test_cli_rejects_wrong_approval_before_transport_initialization(self):
        with self.assertRaises(ReleaseError):
            publication.run(self.arguments("publish"), lambda _: self.fail("network initialized"))

    def test_missing_required_acceptance_case_blocks(self):
        for name in ("AC01", "AC08", "AC19", "AC25", "AC27"):
            saved = self.fixture.qualification["target_cases"][TARGET].pop(name)
            self.fixture.save()
            with self.subTest(case=name), self.assertRaises(ReleaseError):
                self.fixture.plan()
            self.fixture.qualification["target_cases"][TARGET][name] = saved

    def test_not_run_deferred_failed_cases_are_not_passes(self):
        for status in ("NOT_RUN", "DEFERRED", "FAIL", "PASSED"):
            self.fixture.qualification["target_cases"][TARGET]["AC01"]["status"] = status
            self.fixture.save()
            with self.subTest(status=status), self.assertRaises(ReleaseError):
                self.fixture.plan()

    def test_source_exposure_requires_public02_exact_mapping(self):
        self.fixture.request.update(repository="fixture/source", source_exposure=True, tag_commit="a" * 40)
        self.fixture.save()
        with self.assertRaises(ReleaseError):
            self.fixture.plan()
        self.fixture.qualification["gates"]["PUBLIC02"] = copy.deepcopy(self.fixture.gate)
        self.fixture.save()
        self.fixture.plan()

    def test_binary_only_requires_reviewed_separate_destination(self):
        self.fixture.qualification["gates"]["PUBLIC01"].pop("binary_only_destination_and_generated_source_archives_reviewed")
        self.fixture.save()
        with self.assertRaises(ReleaseError):
            self.fixture.plan()
        self.fixture.request["repository"] = self.fixture.request["source_repository"]
        with self.assertRaises(ReleaseError):
            destination(self.fixture.request, [self.fixture.identity])

    def test_alpha_requires_a_predecessor_kind_and_fifteen_minutes_of_reliability(self):
        cases = self.fixture.qualification["target_cases"][TARGET]
        self.fixture.plan()
        for key, value in (("AC20", {"duration_seconds": 899}), ("AC23", {"predecessor_kind": "none"})):
            with self.subTest(case=key):
                changed = copy.deepcopy(cases[key])
                cases[key].update(value)
                self.fixture.save()
                with self.assertRaises(ReleaseError):
                    self.fixture.plan()
                cases[key].update(changed)
                self.fixture.save()
        self.fixture.plan()
        self.assertNotIn("AC26", cases)
        self.assertNotIn("FEEDBACK01", self.fixture.qualification["gates"])

    def test_qualification_hash_or_candidate_mismatch_refuses_resume(self):
        self.fixture.qualification["candidate_identities"][0]["manifest_sha256"] = "c" * 64
        self.fixture.save()
        with selected_inputs([self.fixture.candidate]) as (identities, _), self.assertRaises(ReleaseError):
            check_qualification(self.fixture.qualification_file, self.fixture.request, identities)

    def test_modified_evidence_or_incomplete_helper_is_rejected(self):
        evidence = self.fixture.root / "synthetic-evidence.json"
        evidence.write_text('{"status":"PASS","qualified":false}')
        with self.assertRaises(ReleaseError):
            self.fixture.plan()
        from common import sha256
        for gate in [*self.fixture.qualification["gates"].values(),
                     *self.fixture.qualification["target_cases"][TARGET].values()]:
            gate["evidence"][0]["sha256"] = sha256(evidence)
        self.fixture.save()
        with self.assertRaises(ReleaseError):
            self.fixture.plan()

    def test_destination_rejects_tap_private_visibility_stable_or_stages(self):
        for key, value in (("channels", ["archive", "homebrew"]), ("visibility", "private"),
                           ("stage", "M1"), ("prerelease", False), ("repository", "../../elsewhere")):
            request = copy.deepcopy(self.fixture.request)
            request[key] = value
            with self.subTest(key=key), self.assertRaises(ReleaseError):
                destination(request, [self.fixture.identity])

    def second_target(self):
        root = self.fixture.root / "second-native"
        root.mkdir()
        return Fixture(root, target="aarch64-apple-darwin")

    def test_two_independently_reviewed_targets_share_one_immutable_plan(self):
        from publication_engine import make_plan
        second = self.second_target()
        with selected_inputs([self.fixture.candidate, second.candidate]) as (identities, paths):
            self.fixture.qualification["candidate_identities"] = identities
            self.fixture.qualification["target_cases"].update(second.qualification["target_cases"])
            self.fixture.save()
            fingerprint = check_qualification(self.fixture.qualification_file, self.fixture.request, identities)
            plan = make_plan(self.fixture.request, identities, fingerprint)
            self.assertEqual(10, len(plan["assets"]))
            result = publish(plan, paths, self.remote, digest(plan), self.fixture.request["repository"])
            self.assertEqual("published_same_bytes", result["status"])
            self.assertIn("manifest-aarch64-apple-darwin.json", self.remote.contents)
            self.assertIn("manifest-x86_64-unknown-linux-gnu.json", self.remote.contents)

    def test_mixed_source_candidates_refuse_before_network(self):
        second = self.second_target()
        second.manifest["source_commit"] = "c" * 40
        second.write_manifest()
        with self.assertRaises(ReleaseError):
            with selected_inputs([self.fixture.candidate, second.candidate]):
                pass
        self.assertEqual([], self.remote.reads)

    def test_second_target_without_independent_cases_refused(self):
        second = self.second_target()
        with selected_inputs([self.fixture.candidate, second.candidate]) as (identities, _):
            self.fixture.qualification["candidate_identities"] = identities
            self.fixture.save()
            with self.assertRaises(ReleaseError):
                check_qualification(self.fixture.qualification_file, self.fixture.request, identities)

    def test_corrupt_checksum_extra_file_and_duplicate_target_refuse(self):
        with self.assertRaises(ReleaseError):
            with selected_inputs([self.fixture.candidate, self.fixture.candidate]):
                pass
        extra = self.fixture.candidate / "unreviewed.txt"
        extra.write_text("do not distribute")
        with self.assertRaises(ReleaseError):
            with selected_inputs([self.fixture.candidate]):
                pass
        extra.unlink()
        next(self.fixture.candidate.glob("*.sha256")).write_text("wrong checksum")
        with self.assertRaises(ReleaseError):
            with selected_inputs([self.fixture.candidate]):
                pass

    def test_manifest_and_evidence_symlinks_refused(self):
        for path in (self.fixture.candidate / "manifest.json", self.fixture.root / "synthetic-evidence.json"):
            original = path.read_bytes()
            saved = path.with_suffix(".saved")
            saved.write_bytes(original)
            path.unlink()
            path.symlink_to(saved)
            with self.subTest(path=path.name), self.assertRaises((ReleaseError, OSError)):
                if path.name == "manifest.json":
                    with selected_inputs([self.fixture.candidate]):
                        pass
                else:
                    self.fixture.plan()
            path.unlink()
            path.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
