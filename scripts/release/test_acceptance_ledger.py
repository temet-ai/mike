import json
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch

from acceptance import CASE_NAMES, checked, new_report, record_passes, run_acceptance
from acceptance_ledger import ALPHA_GATING, INSTALL_CHECKS, alpha_complete
from common import ReleaseError


class AcceptanceLedgerTests(unittest.TestCase):
    def test_policy_is_a_path_in_this_repository_that_lists_every_case(self):
        from acceptance_ledger import POLICY
        from publication_gates import POLICY as PUBLICATION_POLICY
        self.assertEqual(POLICY, PUBLICATION_POLICY)
        self.assertEqual(POLICY, "docs/RELEASE_ACCEPTANCE_MATRIX.md")
        matrix = (Path(__file__).resolve().parents[2] / POLICY).read_text()
        from acceptance_ledger import case_ledger
        cases = case_ledger(CASE_NAMES)
        self.assertEqual(len(cases), 33)
        for case in cases:
            self.assertIn(f"| {case['id']} |", matrix, case["id"])

    def test_maintenance_requires_persistent_opt_in_before_creating_evidence(self):
        with tempfile.TemporaryDirectory() as root:
            output = Path(root) / "evidence"
            with self.assertRaisesRegex(ReleaseError, "requires --persistent-journey"):
                run_acceptance(Path(root) / "candidate", output, maintenance=True)
            self.assertFalse(output.exists())

    def test_all_33_cases_remain_unexecuted_with_explicit_evidence_fields(self):
        report = new_report()
        expected = {f"AC{n:02}" for n in range(1, 28)} | {"SG01", "SG02", "SG03", "PUBLIC01", "PUBLIC02", "FEEDBACK01"}
        self.assertEqual(len(CASE_NAMES), 27)
        self.assertEqual({item["id"] for item in report["cases"]}, expected)
        self.assertEqual(len(report["cases"]), 33)
        for item in report["cases"]:
            self.assertEqual(item["state"], "NOT_RUN")
            self.assertIsNone(item["exit_status"])
            self.assertIsNone(item["observed"])
            self.assertEqual(item["evidence"], [])
            self.assertTrue(item["blockers"])
        self.assertFalse(report["qualified"])

    def test_post_publication_and_source_gates_are_not_circular_prerequisites(self):
        items = {item["id"]: item for item in new_report()["cases"]}
        for identifier in ("AC26", "FEEDBACK01"):
            self.assertEqual(items[identifier]["required_before"], ["alpha_after_publication"])
        self.assertEqual(items["PUBLIC02"]["required_before"], ["before_source_exposure"])
        for identifier in ("SG01", "AC20"):
            self.assertEqual(items[identifier]["required_before"], ["alpha_before_publication"])

    def test_helper_success_does_not_create_a_command_exit_or_promote_cases(self):
        report = new_report()
        with patch("acceptance.time.monotonic", side_effect=[10.0, 12.5]):
            self.assertEqual(checked(report, "example", lambda: 3), 3)
        item = report["checks"][0]
        self.assertEqual(item["state"], "PASSED")
        self.assertEqual(item["elapsed_seconds"], 2.5)
        self.assertIsNone(item["exit_status"])
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["cases"]))

    def test_helper_failure_records_timing_without_exception_content(self):
        report = new_report()
        def fail():
            raise ValueError("private sentinel")
        with patch("acceptance.time.monotonic", side_effect=[10.0, 11.0]):
            with self.assertRaises(ValueError):
                checked(report, "example", fail)
        self.assertEqual(report["checks"][0]["elapsed_seconds"], 1.0)
        self.assertEqual(report["checks"][0]["error_type"], "ValueError")
        self.assertNotIn("private sentinel", str(report))


class RecordedPassTests(unittest.TestCase):
    DIGEST = "a" * 64

    def journey(self, state="PASSED", digest=DIGEST):
        return {"state": "INCOMPLETE", "binding": {"manifest_sha256": digest},
                "checks": [{"id": "step", "state": state}]}

    def install_checks(self, report):
        report["checks"] += [{"id": name, "state": "PASSED"} for name in INSTALL_CHECKS]

    def states(self, report):
        return {c["id"]: c["state"] for c in report["cases"]}

    def test_only_the_alpha_cases_gate_and_the_rest_stay_not_run(self):
        report = new_report()
        gating = {c["id"] for c in report["cases"] if c["gates_alpha"]}
        self.assertEqual(gating, set(ALPHA_GATING))
        self.assertFalse(alpha_complete(report))
        self.assertTrue(all(not c["required"] for c in report["cases"] if c["id"] not in gating))

    def test_passing_persistent_journey_records_its_cases_only(self):
        report = new_report()
        report["archive_persistent_journey"] = self.journey()
        record_passes(report, self.DIGEST, maintenance=False)
        states = self.states(report)
        for case in ("AC02", "AC03", "AC07"):
            self.assertEqual(states[case], "PASS")
        self.assertEqual(states["AC22"], "NOT_RUN")
        self.assertEqual(states["AC04"], "NOT_RUN")
        passed = next(c for c in report["cases"] if c["id"] == "AC02")
        self.assertIsNone(passed["exit_status"])
        self.assertEqual(passed["blockers"], [])

    def test_failed_or_unbound_journey_moves_no_case(self):
        for journey in (self.journey("FAILED"), self.journey(digest="b" * 64)):
            report = new_report()
            report["archive_persistent_journey"] = journey
            record_passes(report, self.DIGEST, maintenance=True)
            self.assertTrue(all(s == "NOT_RUN" for s in self.states(report).values()))

    def run_with(self, journeys):
        def helpers(candidate, report, *args, **kwargs):
            self.install_checks(report)
            for key in journeys:
                report[key] = self.journey()
            record_passes(report, self.DIGEST, maintenance=True)
        with tempfile.TemporaryDirectory() as root, patch("acceptance.run_helpers", helpers), \
                patch("builtins.print"):
            candidate = Path(root).resolve() / "candidate"
            candidate.mkdir()
            output = Path(root).resolve() / "evidence"
            code = run_acceptance(candidate, output)
            return code, json.loads((output / "result.json").read_text())

    def test_run_acceptance_exits_0_when_every_gating_case_passes_and_2_otherwise(self):
        code, result = self.run_with(("archive_trial_journey", "archive_persistent_journey"))
        self.assertEqual(code, 0)
        self.assertEqual(result["verdict"], "PASS")
        self.assertEqual(result["required_not_run"], 0)
        code, result = self.run_with(("archive_persistent_journey",))
        self.assertEqual(code, 2)
        self.assertEqual(result["verdict"], "NOT RUN")

    def test_full_alpha_run_completes_and_one_missing_case_does_not(self):
        report = new_report()
        self.install_checks(report)
        for key in ("archive_trial_journey", "archive_persistent_journey"):
            report[key] = self.journey()
        record_passes(report, self.DIGEST, maintenance=True)
        self.assertTrue(alpha_complete(report))
        states = self.states(report)
        self.assertEqual(states["AC01"], "PASS")
        self.assertEqual((states["AC23"], states["AC24"]), ("NOT_RUN", "NOT_RUN"))
        self.assertFalse(any(c["gates_alpha"] for c in report["cases"] if c["id"] in ("AC23", "AC24")))
        missing = new_report()
        self.install_checks(missing)
        missing["archive_persistent_journey"] = self.journey()
        record_passes(missing, self.DIGEST, maintenance=True)
        self.assertFalse(alpha_complete(missing))


if __name__ == "__main__":
    unittest.main()
