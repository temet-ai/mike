"""Offline public-route contracts; no candidate, Docker or socket is executed."""
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

import acceptance
import acceptance_isolated as isolated
from common import ReleaseError, sha256
from journey import new_result
from test_journey_faults import Offline
from test_journey_reliability_profiles import BUDGETS

IMAGE = "ghcr.io/example/mike-runner@sha256:" + "a" * 64
OPTIONS = isolated.NormalRunner(IMAGE, Path("/trusted/docker"), Path("/owned/docker.sock"), "release-native")


class OptionsTests(Offline):
    def test_explicit_complete_pinned_options_only(self):
        self.assertIsNone(isolated.runner_options())
        self.assertEqual(isolated.runner_options(IMAGE, OPTIONS.docker_binary, OPTIONS.docker_endpoint, OPTIONS.runtime_name), OPTIONS)
        for values in [(IMAGE, None, None), ("repo:latest", "/docker", "/socket"),
                       ("https://registry/repo@sha256:" + "a" * 64, "/docker", "/socket"),
                       (IMAGE, "docker", "/socket"), (IMAGE, "/docker", "tcp://remote:2375"),
                       ("repo@sha256:" + "A" * 64, "/docker", "/socket")]:
            with self.subTest(values=values), self.assertRaises(ReleaseError):
                isolated.runner_options(*values, runtime_name=OPTIONS.runtime_name)

    def test_runtime_alias_must_be_explicit_and_nonreserved(self):
        for alias in (None, "", "runc", "default", "../runtime", "has space", "a" * 65):
            with self.subTest(alias=alias), self.assertRaises(ReleaseError):
                isolated.runner_options(IMAGE, OPTIONS.docker_binary, OPTIONS.docker_endpoint, alias)

    def test_options_cannot_select_an_unrelated_profile_or_mixed_host_work(self):
        with self.assertRaisesRegex(ReleaseError, "normal_defaults"):
            isolated.validate_request("synthetic_isolation_overrides", OPTIONS)
        for options in (None, OPTIONS):
            with self.assertRaisesRegex(ReleaseError, "separately"):
                isolated.validate_request("normal_defaults", options, other_journeys=True)
        with self.assertRaisesRegex(ReleaseError, "invalid normal"):
            isolated.validate_request("normal_defaults", {"image": IMAGE})

    def test_cli_forwards_explicit_local_runner_and_rejects_partial_or_mixed_options(self):
        with tempfile.TemporaryDirectory() as temporary:
            budget = Path(temporary) / "budgets.json"
            budget.write_text(json.dumps(BUDGETS))
            base = ["release-acceptance", "--candidate", "/candidate", "--output", "/evidence",
                    "--reliability-journey", "--reliability-profile", "normal_defaults",
                    "--reliability-resource-budgets", str(budget)]
            flags = ["--normal-runner-image", IMAGE, "--normal-runner-docker", str(OPTIONS.docker_binary),
                     "--normal-runner-endpoint", str(OPTIONS.docker_endpoint),
                     "--normal-runner-runtime", OPTIONS.runtime_name]
            with patch.object(sys, "argv", base + flags), patch.object(acceptance, "run_acceptance", return_value=2) as run:
                self.assertEqual(acceptance.main(), 2)
                self.assertEqual(run.call_args.kwargs["normal_runner"], OPTIONS)
            for extra in (flags[:2], flags + ["--fault-journey"], flags + ["--persistent-journey"]):
                with self.subTest(extra=extra), patch.object(sys, "argv", base + extra), \
                     patch.object(acceptance, "run_acceptance") as run, self.assertRaises(SystemExit):
                    acceptance.main()
                run.assert_not_called()


class RoutingTests(Offline):
    def setUp(self):
        super().setUp()
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.candidate = self.root / "candidate"
        self.candidate.mkdir()
        self.manifest = {"version": "0.1.0", "target": "x86_64-unknown-linux-gnu", "source_commit": "b" * 40}
        self.write_inputs()
        for name in ("verified_inputs", "install_verified", "smoke", "run_reliability_journey"):
            self.enterContext(patch.object(acceptance, name, side_effect=AssertionError("Host candidate execution route reached")))

    def write_inputs(self):
        (self.candidate / "manifest.json").write_text(json.dumps(self.manifest))
        for binary in ("mike", "mike-mcp"):
            filename = f"{binary}-v{self.manifest['version']}-{self.manifest['target']}.tar.gz"
            (self.candidate / filename).write_bytes(b"offline selected archive " + binary.encode())
            (self.candidate / (filename + ".sha256")).write_text("offline fixture checksum")

    def run_route(self, report, runner=OPTIONS, **kwargs):
        return acceptance.run_helpers(self.candidate, report, reliability=True,
                                      reliability_profile="normal_defaults", reliability_resource_budgets=BUDGETS,
                                      normal_runner=runner, **kwargs)

    def test_missing_runner_fails_before_snapshot_and_every_host_execution_route(self):
        report = acceptance.new_report()
        with patch.object(isolated, "snapshot_inputs") as snapshot, self.assertRaisesRegex(ReleaseError, "explicitly configured"):
            self.run_route(report, runner=None)
        snapshot.assert_not_called()
        self.assertFalse(report["isolation"]["host_candidate_executed"])
        self.assertEqual(report["checks"][0]["code"], "RELIABILITY_ISOLATED_RUNNER_REQUIRED")

    def test_only_read_only_input_snapshot_reaches_runner_and_full_cases_stay_not_run(self):
        report = acceptance.new_report()
        locations = []
        def run(snapshot, manifest, digest, **kwargs):
            self.assertNotEqual(snapshot, self.candidate)
            self.assertEqual(manifest, self.manifest)
            self.assertEqual(digest, sha256(snapshot / "manifest.json"))
            self.assertEqual(kwargs["runner"], OPTIONS)
            self.assertEqual(kwargs["resource_budgets"], BUDGETS)
            locations.append(snapshot)
            return new_result({"manifest_sha256": digest})
        with patch.object(isolated, "dispatch_runner", side_effect=run):
            self.run_route(report)
        self.assertTrue(all(not path.exists() for path in locations))
        self.assertFalse(report["qualified"])
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in report["cases"]))
        self.assertFalse(report["checks"][0]["candidate_execution"])
        self.assertEqual(report["candidate"]["source_commit"], "b" * 40)

    def test_mixed_host_work_is_rejected_before_input_copy(self):
        for key in ("trial_journey", "persistent_journey", "maintenance", "fault_journey", "mode_journey"):
            with self.subTest(key=key), patch.object(isolated, "snapshot_inputs") as snapshot, self.assertRaises(ReleaseError):
                self.run_route(acceptance.new_report(), **{key: True})
            snapshot.assert_not_called()

    def test_invalid_source_identity_never_reaches_runner(self):
        self.manifest["source_commit"] = "private planted sentinel"
        (self.candidate / "manifest.json").write_text(json.dumps(self.manifest))
        report = acceptance.new_report()
        with patch.object(isolated, "dispatch_runner") as run, self.assertRaises(ReleaseError):
            self.run_route(report)
        run.assert_not_called()
        self.assertNotIn("private planted", json.dumps(report))

    def test_aggregate_candidate_limit_is_applied_during_copy_before_runner(self):
        manifest_size = (self.candidate / "manifest.json").stat().st_size
        first = self.candidate / "mike-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
        limit = manifest_size + first.stat().st_size
        report = acceptance.new_report()
        with patch.object(isolated, "MAX_NORMAL_INPUT_BYTES", limit), \
             patch.object(isolated, "dispatch_runner") as run, self.assertRaises(ReleaseError):
            self.run_route(report)
        run.assert_not_called()
        self.assertEqual(report["checks"][0]["state"], "FAILED")

    def test_snapshot_total_budget_never_writes_more_than_cap(self):
        from inputs import snapshot_inputs
        copied = self.root / "bounded-copy"
        copied.mkdir()
        manifest_size = (self.candidate / "manifest.json").stat().st_size
        for maximum in (manifest_size - 1, manifest_size + 5):
            target = copied / str(maximum)
            target.mkdir()
            with self.assertRaises(ReleaseError):
                snapshot_inputs(self.candidate, target, max_total_bytes=maximum)
            self.assertLessEqual(sum(path.stat().st_size for path in target.iterdir()), maximum)
        for maximum in (0, -1, True, 1.5):
            with self.assertRaisesRegex(ReleaseError, "aggregate"):
                snapshot_inputs(self.candidate, copied, max_total_bytes=maximum)

    def test_failed_runner_is_retained_without_promoting_cases(self):
        result = new_result({}); result["state"] = "FAILED"
        report = acceptance.new_report()
        with patch.object(isolated, "dispatch_runner", return_value=result), self.assertRaises(ReleaseError):
            self.run_route(report)
        self.assertEqual(report["archive_reliability_journey"]["state"], "FAILED")
        self.assertFalse(report["qualified"])

    def test_container_result_cannot_override_full_acceptance_qualification(self):
        for field in ("qualified", "state", "full_case", "case_count"):
            result = new_result({})
            if field == "qualified": result["qualified"] = True
            elif field == "state": result["state"] = "PASSED"
            elif field == "full_case": result["full_cases"][0]["state"] = "PASSED"
            else: result["full_cases"].pop()
            report = acceptance.new_report()
            with self.subTest(field=field), patch.object(isolated, "dispatch_runner", return_value=result), self.assertRaises(ReleaseError):
                self.run_route(report)
            self.assertNotIn("archive_reliability_journey", report)


if __name__ == "__main__":
    import unittest
    unittest.main()
