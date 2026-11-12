"""In-memory request contracts only; sockets and candidate execution are blocked."""
import unittest

from journey_process import JourneyError
from journey_reliability_services import MAX_SERVICE_REQUESTS, ServiceGuard
from test_journey_reliability import OfflineTests


class ServiceGuardTests(OfflineTests):
    def test_every_request_is_rejected_and_counted(self):
        for method, path in (("GET", "/api/runs"), ("POST", "/embeddings"), ("GET", "/models"),
                             ("POST", "/v1/messages"), ("HEAD", "/")):
            with self.subTest(method=method, path=path):
                guard = ServiceGuard()
                with self.assertRaisesRegex(JourneyError, "NORMAL_DEFAULT_SERVICE_PATH"):
                    guard.respond(method, path)
                self.assertEqual(guard.counts(), {"attempted": 1, "accepted": 0, "rejected": 1,
                                                  "request_limit_exceeded": False})

    def test_defaults_expect_no_service_requests(self):
        guard = ServiceGuard()
        self.assertIsNone(guard.require_no_default_worker_calls())
        with self.assertRaises(JourneyError):
            guard.respond("POST", "/embeddings")
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_SERVICE_CALL"):
            guard.require_no_default_worker_calls()

    def test_input_is_not_retained(self):
        guard = ServiceGuard()
        with self.assertRaises(JourneyError):
            guard.respond("GET", "/api/runs?key=private-query", b"private-body")
        self.assertNotIn("private-", repr(vars(guard)))

    def test_calls_beyond_the_limit_stay_bounded(self):
        guard = ServiceGuard(request_limit=2)
        for _ in range(2):
            with self.assertRaisesRegex(JourneyError, "SERVICE_PATH"):
                guard.respond("GET", "/private")
        for _ in range(20):
            with self.assertRaisesRegex(JourneyError, "REQUEST_LIMIT"):
                guard.respond("GET", "/private")
        self.assertEqual(guard.counts(), {"attempted": 2, "accepted": 0, "rejected": 2,
                                          "request_limit_exceeded": True})
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_SERVICE_CALL"):
            guard.require_no_default_worker_calls()

    def test_counts_are_snapshots(self):
        guard = ServiceGuard(request_limit=2)
        guard.counts()["rejected"] = 100
        self.assertEqual(guard.counts()["rejected"], 0)

    def test_bounds_reject_bool_non_integer_negative_and_excessive_values(self):
        for limit in (True, 0, -1, MAX_SERVICE_REQUESTS + 1, 1.0, "1", None):
            with self.subTest(limit=limit), self.assertRaisesRegex(JourneyError, "SERVICE_REQUEST_LIMIT"):
                ServiceGuard(request_limit=limit)


if __name__ == "__main__":
    unittest.main()
