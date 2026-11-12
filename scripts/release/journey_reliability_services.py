"""Bounded in-memory guard for the owned services endpoint, with no server or runtime adapter.

The normal-default synthetic capture workload must make zero service calls.
Every call is rejected and counted: the endpoint exists only so that a relocated
service URL has a loopback owner, and any request to it is an unexpected dependency.
Embedding and LLM calls are rejected: their compiled-disabled or uninvoked paths
must not be silently made active just because a stub is available.
"""
import threading

from journey_process import JourneyError, require

MAX_SERVICE_REQUESTS = 4096


class ServiceGuard:
    """Serve nothing; count and reject every request."""
    def __init__(self, *, request_limit=1):
        require(type(request_limit) is int and 1 <= request_limit <= MAX_SERVICE_REQUESTS,
                "NORMAL_DEFAULT_SERVICE_REQUEST_LIMIT")
        self.request_limit = request_limit
        self._guard = threading.Lock()
        self._attempts = 0
        self._rejected = 0
        self._exhausted = False

    def respond(self, method, path, body=b""):
        """Pure wire dispatch: always rejects; opens no socket.

        Counts are bounded and never retain caller-supplied paths or bodies.
        Rejected calls consume the request budget.
        """
        with self._guard:
            if self._attempts >= self.request_limit:
                self._exhausted = True
                raise JourneyError("NORMAL_DEFAULT_SERVICE_REQUEST_LIMIT")
            self._attempts += 1
            self._rejected += 1
            raise JourneyError("NORMAL_DEFAULT_SERVICE_PATH")

    def counts(self):
        with self._guard:
            return {"attempted": self._attempts, "accepted": 0,
                    "rejected": self._rejected, "request_limit_exceeded": self._exhausted}

    def require_no_default_worker_calls(self):
        """Check fixture accounting only, never all-transport egress isolation."""
        with self._guard:
            require(self._attempts == 0 and not self._exhausted,
                    "NORMAL_DEFAULT_UNEXPECTED_SERVICE_CALL")
