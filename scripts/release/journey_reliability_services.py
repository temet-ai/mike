"""Bounded in-memory Finite wire contracts, with no server or runtime adapter.

The normal-default synthetic capture workload must make zero service calls.
Finite responses are for separately exercised operator inspection routes only.
Embedding and LLM calls are rejected: their compiled-disabled or uninvoked paths
must not be silently made active just because a stub is available.
"""
import json
import threading

from journey_process import JourneyError, require

MAX_FINITE_RUNS = 32
MAX_SERVICE_REQUESTS = 4096
MAX_SERVICE_RESPONSE_BYTES = 64 * 1024


class FiniteContract:
    """Serve only generated synthetic records; never accept arbitrary fixture data."""
    def __init__(self, *, run_count=0, request_limit=1):
        require(type(run_count) is int and 0 <= run_count <= MAX_FINITE_RUNS,
                "NORMAL_DEFAULT_FINITE_RUN_LIMIT")
        require(type(request_limit) is int and 1 <= request_limit <= MAX_SERVICE_REQUESTS,
                "NORMAL_DEFAULT_SERVICE_REQUEST_LIMIT")
        self.request_limit = request_limit
        self._guard = threading.Lock()
        self._attempts = 0
        self._accepted = 0
        self._rejected = 0
        self._exhausted = False
        self._responses = self._synthetic_responses(run_count)

    @staticmethod
    def _synthetic_responses(count):
        runs, values = [], {}
        for index in range(count):
            run = f"archive-synthetic-run-{index:02d}"
            runs.append({"run_id": run, "machine_id": "archive-synthetic-machine",
                         "latest_ts": "2000-01-01T00:00:00Z", "terminal": True})
            values[f"/api/run/{run}/manifest"] = {"run_id": run, "entries": [{
                "event_kind": "turn-report", "actor_id": "archive-synthetic-actor",
                "ts": "2000-01-01T00:00:00Z", "verdict": "synthetic",
                "payload": {"synthetic": True},
            }]}
        values["/api/runs"] = {"runs": runs}
        responses = {path: json.dumps(value, separators=(",", ":")).encode("ascii")
                     for path, value in values.items()}
        require(all(len(body) <= MAX_SERVICE_RESPONSE_BYTES for body in responses.values()),
                "NORMAL_DEFAULT_SERVICE_RESPONSE_LIMIT")
        return responses

    def respond(self, method, path, body=b""):
        """Pure wire dispatch: returns status, headers and bytes; opens no socket.

        Counts are bounded and never retain caller-supplied paths or bodies.
        Rejected calls consume the same finite budget as successful calls.
        """
        with self._guard:
            if self._attempts >= self.request_limit:
                self._exhausted = True
                raise JourneyError("NORMAL_DEFAULT_SERVICE_REQUEST_LIMIT")
            self._attempts += 1
            try:
                require(type(method) is str and method == "GET",
                        "NORMAL_DEFAULT_SERVICE_METHOD")
                require(type(body) is bytes and body == b"", "NORMAL_DEFAULT_SERVICE_BODY")
                require(type(path) is str and path in self._responses,
                        "NORMAL_DEFAULT_SERVICE_PATH")
                response = self._responses[path]
            except JourneyError:
                self._rejected += 1
                raise
            self._accepted += 1
            return 200, {"Content-Type": "application/json", "Content-Length": str(len(response)),
                         "Connection": "close"}, response

    def counts(self):
        with self._guard:
            return {"attempted": self._attempts, "accepted": self._accepted,
                    "rejected": self._rejected, "request_limit_exceeded": self._exhausted}

    def require_no_default_worker_calls(self):
        """Check fixture accounting only, never all-transport egress isolation."""
        with self._guard:
            require(self._attempts == 0 and not self._exhausted,
                    "NORMAL_DEFAULT_UNEXPECTED_SERVICE_CALL")
