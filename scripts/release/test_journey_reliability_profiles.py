"""Offline policy/admission contracts, never normal-default runtime evidence."""
import json
from pathlib import Path
import re
from unittest.mock import patch
import unittest

import journey_reliability as reliability
import journey_reliability_fixture as synthetic
import journey_reliability_profiles as profiles
from journey_process import JourneyError
from test_journey_reliability import OfflineTests

BUDGETS = {"rss_peak_bytes": 123456789, "fd_peak": 321, "broadcast_backlog_peak": 27,
           "active_streams_peak": 8, "drain_seconds": 9, "responsiveness_seconds": 3}


class ProfileTests(OfflineTests):
    def test_no_implicit_resource_limits_or_normal_default_claim(self):
        self.assertEqual(profiles.reliability_options(), {"profile": profiles.SYNTHETIC_PROFILE, "resource_budgets": None})
        self.assertEqual(profiles.reliability_options(resource_budgets=BUDGETS)["resource_budgets"], BUDGETS)
        with self.assertRaisesRegex(JourneyError, "EXPLICIT_BUDGETS_REQUIRED"):
            profiles.reliability_options(profiles.NORMAL_PROFILE)

    def test_strict_bounded_explicit_budgets(self):
        for values in ([], {}, dict(BUDGETS, unknown=1), {key: value for key, value in BUDGETS.items() if key != "fd_peak"}):
            with self.subTest(values=values), self.assertRaises(JourneyError):
                profiles.reliability_options(resource_budgets=values)
        for key in BUDGETS:
            invalid = [True, -1, "1", float("nan"), float("inf"), 10 ** 1000]
            if key != "broadcast_backlog_peak":
                invalid.append(0)
            if key in profiles.BUDGET_FIELDS[:4]:
                invalid.append(1.5)
            else:
                invalid.append(31)
            for value in invalid:
                with self.subTest(key=key, value=value), self.assertRaises(JourneyError):
                    profiles.reliability_options(resource_budgets=dict(BUDGETS, **{key: value}))
        self.assertEqual(profiles.reliability_options(resource_budgets=dict(BUDGETS, broadcast_backlog_peak=0))["resource_budgets"]["broadcast_backlog_peak"], 0)

    def test_unknown_profile_fails_before_binding_or_execution(self):
        with patch.object(reliability, "verify_binding") as binding, patch.object(reliability, "owned_cluster") as cluster:
            result = reliability.run_reliability_journey(None, {}, None, "", profile="default-ish")
        binding.assert_not_called(); cluster.assert_not_called()
        self.assertEqual(result["failure_code"], "RELIABILITY_PROFILE_INVALID")

    def test_normal_defaults_fails_closed_before_any_configuration_or_candidate_activity(self):
        with patch.object(reliability, "verify_binding") as binding, patch.object(reliability, "owned_cluster") as cluster, \
             patch.object(reliability, "disable_features") as disable, patch.object(reliability, "isolated_config") as config, \
             patch.object(reliability, "owned_child") as child, patch.object(reliability, "run_bound") as run:
            result = reliability.run_reliability_journey(None, {}, None, "", profile=profiles.NORMAL_PROFILE, resource_budgets=BUDGETS)
        for action in (binding, cluster, disable, config, child, run):
            action.assert_not_called()
        self.assertEqual(result["failure_code"], "RELIABILITY_NORMAL_DEFAULT_ISOLATED_RUNNER_REQUIRED")
        self.assertFalse(result["qualified"])
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in result["full_cases"]))
        report = result["reliability"]
        self.assertEqual(report["profile"], "normal_defaults")
        self.assertEqual(report["workload_state"], "NOT_RUN")
        self.assertEqual(report["requested_disabled_feature_keys"], [])
        self.assertEqual(report["override_names"], [])
        contract = report["normal_default_contract"]
        self.assertFalse(contract["candidate_effective_defaults_verified"])
        self.assertEqual(contract["baseline_comparison"]["state"], "NOT_RUN")
        self.assertEqual(len(contract["required_execution"]), 2)
        self.assertEqual({item["code"] for item in contract["required_execution"]}, {
            "ENGINE_VERIFIED_ISOLATED_RUNNER_REQUIRED", "NORMAL_DEFAULT_RUNTIME_EVIDENCE_REQUIRED"})
        self.assertEqual(contract["implemented_offline_contracts"]["state"], "IMPLEMENTED_OFFLINE")
        self.assertFalse(contract["implemented_offline_contracts"]["candidate_runtime_evidence"])
        self.assertEqual(contract["source_reviewed_dependency_plan"]["runtime_admission"],
                         "REQUIRES_VERIFIED_ISOLATED_RUNNER")
        self.assertNotIn("PASSED", json.dumps(report))

    def test_active_streams_budget_cannot_be_lower_than_requested_cohort(self):
        with patch.object(reliability, "verify_binding") as binding:
            result = reliability.run_reliability_journey(None, {}, None, "", concurrent_sessions=8,
                resource_budgets=dict(BUDGETS, active_streams_peak=7))
        binding.assert_not_called()
        self.assertEqual(result["failure_code"], "RELIABILITY_ACTIVE_STREAMS_BUDGET_BELOW_WORKLOAD")

    def test_normal_feature_contract_matches_current_source_without_becoming_runtime_evidence(self):
        root = Path(__file__).resolve().parents[2]
        source = (root / "crates/mike-base/src/feature_toggles/mod.rs").read_text()
        block = source.split("impl Default for FeatureToggles", 1)[1].split("#[derive", 1)[0]
        actual = {name: value == "true" for name, value in re.findall(r"(\w+): (true|false),", block)}
        contract = profiles.normal_default_contract()
        self.assertEqual(actual, contract["source_reviewed_feature_defaults"])
        keys = {key for item in contract["required_dependency_contracts"] for key in item["keys"]}
        self.assertEqual(keys, profiles.NORMAL_ENABLED)
        for item in contract["required_dependency_contracts"]:
            for path in item["source"]:
                self.assertTrue((root / path).is_file(), path)
        self.assertEqual(contract["state"], "NOT_RUN")

    def test_report_includes_untoggled_worker_source_inventory_and_requirements(self):
        root = Path(__file__).resolve().parents[2]
        plan = profiles.normal_default_contract()["source_reviewed_dependency_plan"]
        source = (root / plan["background_worker_entrypoint"]).read_text()
        self.assertEqual(set(re.findall(r"crate::([a-z_:]+)\(state\);", source)),
                         set(plan["background_worker_conditions"]))
        for worker in ("mailbox_reaper::spawn_mailbox_reaper", "work_registry_worker::spawn_work_registry_worker",
                       "launch::fleet::display::spawn_fleet_display", "launch::cli::spawn_queue_replay",
                       "wright::watch::spawn_wright_worker", "memory_keeper::worker::spawn_memory_keeper",
                       "mail_outbox::spawn_mail_outbox", "api::handlers::spawn_health_refresher"):
            self.assertIn(worker, plan["background_worker_conditions"])
        self.assertIn("empty_launch_queue", plan["required_private_state"])
        self.assertFalse(plan["embedding_service"]["compiled_enabled"])
        self.assertEqual(plan["embedding_service"]["compiled_url"], "http://127.0.0.1:7997")
        self.assertFalse(plan["embedding_service"]["toml_url_override_effective"])
        self.assertIn("system prompt", plan["workspace_registration"]["marker_placement"])
        self.assertFalse(plan["candidate_defaults_observed"])
        self.assertFalse(plan["endpoint_ownership_observed"])

    def test_current_cohort_has_no_project_marker_or_harvester_tag_as_reported(self):
        contract = profiles.normal_default_contract()
        gaps = contract["source_reviewed_dependency_plan"]["legacy_fixture_exercise_gaps"]
        self.assertEqual(gaps["workspace_capture_registration"]["state"], "NOT_RUN")
        self.assertFalse(gaps["workspace_capture_registration"]["current_request_has_project_marker"])
        self.assertEqual(gaps["harvester"]["state"], "NOT_RUN")
        self.assertFalse(gaps["harvester"]["current_response_has_update_tag"])
        cohort = synthetic.Cohort(8)
        for batch in range(2):
            for fixture in cohort.prepare(batch):
                body = json.loads(fixture.body)
                self.assertNotIn("system", body)
                for marker in (b"Working directory:", b"Primary working directory:", b"Contents of /"):
                    self.assertNotIn(marker, fixture.body)
                self.assertNotIn(b"<mem:update ", b"".join(fixture.events))
        parser = Path(__file__).resolve().parents[2] / gaps["harvester"]["source"]
        self.assertIn('find("<mem:update ")', parser.read_text())

    def test_self_reported_isolation_or_completed_offline_contracts_cannot_admit_runtime(self):
        report = {"workload_state": "PASSED", "normal_default_contract": {
            "state": "PASSED", "candidate_effective_defaults_verified": True,
            "isolation_verified": True, "worker_contracts_implemented": True}}
        with self.assertRaisesRegex(JourneyError, "^RELIABILITY_NORMAL_DEFAULT_ISOLATED_RUNNER_REQUIRED$"):
            profiles.require_normal_default_isolation(report)
        self.assertEqual(report["workload_state"], "NOT_RUN")
        self.assertEqual(report["normal_default_contract"]["state"], "NOT_RUN")
        self.assertFalse(report["normal_default_contract"]["candidate_effective_defaults_verified"])
        self.assertNotIn("PASSED", json.dumps(report))


if __name__ == "__main__":
    unittest.main()
