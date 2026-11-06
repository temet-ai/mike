"""In-memory request contracts only; sockets and candidate execution are blocked."""
import json
from pathlib import Path
import unittest

from journey_process import JourneyError
from journey_reliability_services import (FiniteContract, MAX_FINITE_RUNS,
                                          MAX_SERVICE_REQUESTS, MAX_SERVICE_RESPONSE_BYTES)
from test_journey_reliability import OfflineTests


class FiniteContractTests(OfflineTests):
    def test_empty_list_matches_actual_finite_client_schema(self):
        fixture = FiniteContract()
        status, headers, body = fixture.respond("GET", "/api/runs")
        self.assertEqual(status, 200)
        self.assertEqual(headers, {"Content-Type": "application/json", "Connection": "close",
                                   "Content-Length": str(len(body))})
        self.assertEqual(json.loads(body), {"runs": []})
        self.assertEqual(fixture.counts(), {"attempted": 1, "accepted": 1, "rejected": 0,
                                            "request_limit_exceeded": False})

    def test_generated_list_and_manifest_identity_and_fields_match_source_models(self):
        fixture = FiniteContract(run_count=MAX_FINITE_RUNS, request_limit=MAX_FINITE_RUNS + 1)
        _status, _headers, body = fixture.respond("GET", "/api/runs")
        runs = json.loads(body)["runs"]
        self.assertEqual(len(runs), MAX_FINITE_RUNS)
        self.assertLessEqual(len(body), MAX_SERVICE_RESPONSE_BYTES)
        self.assertEqual(len({run["run_id"] for run in runs}), MAX_FINITE_RUNS)
        for run in runs:
            self.assertEqual(set(run), {"run_id", "machine_id", "latest_ts", "terminal"})
            self.assertTrue(run["run_id"].startswith("archive-synthetic-run-"))
            _status, headers, body = fixture.respond("GET", f"/api/run/{run['run_id']}/manifest")
            self.assertEqual(int(headers["Content-Length"]), len(body))
            self.assertLessEqual(len(body), MAX_SERVICE_RESPONSE_BYTES)
            manifest = json.loads(body)
            self.assertEqual(set(manifest), {"run_id", "entries"})
            self.assertEqual(manifest["run_id"], run["run_id"])
            self.assertEqual(set(manifest["entries"][0]),
                             {"event_kind", "actor_id", "ts", "verdict", "payload"})
            self.assertEqual(manifest["entries"][0]["payload"], {"synthetic": True})
        self.assertEqual(fixture.counts()["accepted"], MAX_FINITE_RUNS + 1)

    def test_defaults_expect_no_service_requests_even_when_a_fixture_exists(self):
        fixture = FiniteContract()
        self.assertIsNone(fixture.require_no_default_worker_calls())
        fixture.respond("GET", "/api/runs")
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_SERVICE_CALL"):
            fixture.require_no_default_worker_calls()
        rejected = FiniteContract()
        with self.assertRaises(JourneyError):
            rejected.respond("POST", "/embeddings")
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_SERVICE_CALL"):
            rejected.require_no_default_worker_calls()

    def test_unknown_methods_bodies_and_routes_are_rejected_without_retaining_input(self):
        cases = [("POST", "/api/runs", b""), ("HEAD", "/api/runs", b""),
                 ("GET", "/api/runs", b"private-body"), ("GET", "/api/runs", ""),
                 ("GET", "/api/run/real-run/manifest", b""), ("GET", "/models", b""),
                 ("GET", "/embeddings", b""), ("POST", "/v1/messages", b""),
                 ("GET", "/api/runs?key=private-query", b""),
                 ("GET", "http://external.example/api/runs", b""),
                 ("GET", "/api/run/%2e%2e/manifest", b""), ("GET", [], b"")]
        for method, path, body in cases:
            with self.subTest(method=method, path=path):
                fixture = FiniteContract()
                with self.assertRaises(JourneyError) as raised:
                    fixture.respond(method, path, body)
                self.assertTrue(str(raised.exception).startswith("NORMAL_DEFAULT_SERVICE_"))
                self.assertEqual(fixture.counts(), {"attempted": 1, "accepted": 0, "rejected": 1,
                                                    "request_limit_exceeded": False})
                self.assertNotIn("private-", repr(vars(fixture)))

    def test_rejected_calls_consume_budget_and_counters_remain_bounded(self):
        fixture = FiniteContract(request_limit=2)
        with self.assertRaisesRegex(JourneyError, "SERVICE_PATH"):
            fixture.respond("GET", "/private")
        fixture.respond("GET", "/api/runs")
        for _ in range(20):
            with self.assertRaisesRegex(JourneyError, "REQUEST_LIMIT"):
                fixture.respond("GET", "/api/runs")
        self.assertEqual(fixture.counts(), {"attempted": 2, "accepted": 1, "rejected": 1,
                                            "request_limit_exceeded": True})

    def test_counts_are_snapshots_and_returned_headers_cannot_mutate_responses(self):
        fixture = FiniteContract(request_limit=2)
        fixture.counts()["accepted"] = 100
        _status, headers, first = fixture.respond("GET", "/api/runs")
        headers["Content-Length"] = "99999"
        _status, headers, second = fixture.respond("GET", "/api/runs")
        self.assertEqual(first, second)
        self.assertEqual(int(headers["Content-Length"]), len(second))
        self.assertEqual(fixture.counts()["accepted"], 2)

    def test_bounds_reject_bool_non_integer_negative_and_excessive_values(self):
        for count in (True, -1, MAX_FINITE_RUNS + 1, 1.0, "1", None):
            with self.subTest(count=count), self.assertRaisesRegex(JourneyError, "FINITE_RUN_LIMIT"):
                FiniteContract(run_count=count)
        for limit in (True, 0, -1, MAX_SERVICE_REQUESTS + 1, 1.0, "1", None):
            with self.subTest(limit=limit), self.assertRaisesRegex(JourneyError, "SERVICE_REQUEST_LIMIT"):
                FiniteContract(request_limit=limit)

    def test_optional_routes_match_source_without_claiming_worker_execution(self):
        root = Path(__file__).resolve().parents[2]
        source = (root / "crates/mike/src/fleet/finite_client.rs").read_text()
        self.assertIn("/api/run/{run_id}/manifest", source)
        self.assertIn("/api/runs", source)
        for field in ("run_id", "machine_id", "latest_ts", "terminal", "entries",
                      "event_kind", "actor_id", "ts", "verdict", "payload"):
            self.assertIn(f"{field}:", source)


if __name__ == "__main__":
    unittest.main()
