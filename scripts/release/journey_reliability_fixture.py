"""Owned, bounded multi-session synthetic SSE fixture for reliability helpers.

No external provider and no operator payload is accepted. Each batch has exactly
one predeclared body per session. A server-side barrier holds every response open
until all of that batch's distinct sessions have reached the synthetic upstream.
"""
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import http.client
import json
import socket
import threading
import time
import uuid

from journey_fixture import Fixture
from journey_process import JourneyError, require

MAX_SESSIONS = 32
MAX_BODY = 64 * 1024
BATCH_TIMEOUT = 20
CLEANUP_TIMEOUT = 5


class Cohort:
    def __init__(self, count):
        require(type(count) is int and 1 <= count <= MAX_SESSIONS, "RELIABILITY_SESSION_LIMIT")
        self.sessions = tuple(str(uuid.uuid4()) for _ in range(count))
        self.lock = threading.Lock()
        self.pending = {}
        self.claimed = set()
        self.active = 0
        self.peak_active = 0
        self.all_active = threading.Event()
        self.cancelled = threading.Event()
        self.observation_release = threading.Event()
        self.observation_release.set()
        self.failures = 0
        self.batch = -1

    def prepare(self, batch):
        with self.lock:
            require(type(batch) is int and batch == self.batch + 1 and self.active == 0,
                    "RELIABILITY_BATCH_OWNERSHIP")
            require(not self.cancelled.is_set(), "RELIABILITY_FIXTURE_CANCELLED")
            fixtures = []
            for session in self.sessions:
                fixture = Fixture()
                fixture.session = session
                suffix = session.replace("-", "") + "_" + str(batch)
                fixture.message_id = "msg_archive_reliability_" + suffix
                fixture.tool_use_id = "tool_archive_reliability_response_" + suffix
                prior_id = "tool_archive_reliability_prior_" + suffix
                fixture.events = [data.replace(b"msg_archive_fixture", fixture.message_id.encode())
                                  .replace(b"tool_archive_response", fixture.tool_use_id.encode())
                                  for data in fixture.events]
                value = json.loads(fixture.body.replace(b"tool_archive_prior", prior_id.encode()))
                value["messages"][0]["content"] = f"Synthetic archive reliability session {session}, batch {batch}"
                fixture.body = json.dumps(value, separators=(",", ":")).encode()
                require(len(fixture.body) <= MAX_BODY and sum(map(len, fixture.events)) <= MAX_BODY,
                        "RELIABILITY_FIXTURE_BYTES")
                fixtures.append(fixture)
            self.pending = {hashlib.sha256(item.body).digest(): item for item in fixtures}
            require(len(self.pending) == len(self.sessions), "RELIABILITY_DUPLICATE_FIXTURE")
            self.claimed.clear()
            self.all_active.clear()
            self.peak_active = 0
            self.batch = batch
            return fixtures

    def claim(self, body):
        key = hashlib.sha256(body).digest()
        with self.lock:
            require(not self.cancelled.is_set() and key in self.pending and key not in self.claimed,
                    "RELIABILITY_UNEXPECTED_UPSTREAM_REQUEST")
            fixture = self.pending[key]
            require(body == fixture.body, "RELIABILITY_UPSTREAM_BODY_MUTATED")
            self.claimed.add(key)
            self.active += 1
            self.peak_active = max(self.peak_active, self.active)
            if self.active == len(self.sessions):
                self.all_active.set()
            return fixture

    def finish(self):
        with self.lock:
            self.active -= 1

    def cancel(self):
        self.cancelled.set()
        self.all_active.set()
        self.observation_release.set()
        with self.lock:
            for fixture in self.pending.values():
                fixture.release.set()

    def handler(self):
        cohort = self
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def setup(self):
                super().setup()
                self.connection.settimeout(BATCH_TIMEOUT)
            def log_message(self, *_args):
                pass
            def do_POST(self):
                fixture = None
                try:
                    require(self.path == "/v1/messages", "RELIABILITY_UPSTREAM_PATH")
                    length = int(self.headers.get("Content-Length", "0"))
                    require(0 < length <= MAX_BODY, "RELIABILITY_UPSTREAM_BODY_LIMIT")
                    body = self.rfile.read(length)
                    require(len(body) == length, "RELIABILITY_UPSTREAM_SHORT_BODY")
                    fixture = cohort.claim(body)
                    fixture.received = body
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Transfer-Encoding", "chunked")
                    self.send_header("request-id", "req_archive_fixture")
                    self.send_header("anthropic-ratelimit-requests-remaining", "42")
                    self.end_headers()
                    self.chunk(fixture.events[0])
                    fixture.first_sent.set()
                    require(cohort.all_active.wait(BATCH_TIMEOUT), "RELIABILITY_CONCURRENCY_BARRIER_TIMEOUT")
                    require(cohort.observation_release.wait(BATCH_TIMEOUT), "RELIABILITY_ACTIVE_OBSERVATION_TIMEOUT")
                    require(fixture.release.wait(BATCH_TIMEOUT) and not cohort.cancelled.is_set(),
                            "RELIABILITY_CLIENT_OBSERVATION_TIMEOUT")
                    for data in fixture.events[1:]:
                        self.chunk(data)
                    self.wfile.write(b"0\r\n\r\n")
                    self.wfile.flush()
                    fixture.completed.set()
                except Exception:
                    # No paths, request bodies or exception messages enter reports.
                    with cohort.lock:
                        cohort.failures += 1
                    if fixture is not None:
                        fixture.failure = "RELIABILITY_UPSTREAM_FAILED"
                    self.close_connection = True
                finally:
                    if fixture is not None:
                        cohort.finish()
            def chunk(self, data):
                self.wfile.write((f"{len(data):x}\r\n").encode() + data + b"\r\n")
                self.wfile.flush()
        return Handler


class BoundedServer(ThreadingHTTPServer):
    """Bound active handler threads even if the owned candidate misbehaves."""
    daemon_threads = True
    block_on_close = False
    request_queue_size = MAX_SESSIONS

    def __init__(self, address, handler):
        self.slots = threading.BoundedSemaphore(MAX_SESSIONS)
        self.handlers = set()
        self.connections = set()
        self.guard = threading.Lock()
        super().__init__(address, handler)

    def handle_error(self, request, client_address):
        pass

    def process_request(self, request, client_address):
        if not self.slots.acquire(blocking=False):
            self.shutdown_request(request)
            return
        with self.guard:
            self.connections.add(request)
        try:
            super().process_request(request, client_address)
        except BaseException:
            with self.guard:
                self.connections.discard(request)
            self.slots.release()
            raise

    def process_request_thread(self, request, client_address):
        worker = threading.current_thread()
        with self.guard:
            self.handlers.add(worker)
        try:
            super().process_request_thread(request, client_address)
        finally:
            with self.guard:
                self.handlers.discard(worker)
                self.connections.discard(request)
            self.slots.release()

    def close_owned_connections(self):
        with self.guard:
            connections = tuple(self.connections)
        for connection in connections:
            try:
                connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            connection.close()
        deadline = time.monotonic() + CLEANUP_TIMEOUT
        while True:
            with self.guard:
                handlers = tuple(self.handlers)
                pending = len(self.connections)
            if not handlers and not pending:
                return
            require(time.monotonic() < deadline, "RELIABILITY_UPSTREAM_THREAD_CLEANUP")
            for worker in handlers:
                worker.join(timeout=max(0, deadline - time.monotonic()))
            if not handlers:
                time.sleep(0.01)


@contextmanager
def reliability_fixture(count):
    cohort = Cohort(count)
    server = BoundedServer(("127.0.0.1", 0), cohort.handler())
    worker = None
    started = False
    try:
        worker = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
        worker.start()
        started = True
        yield cohort, server.server_port
    finally:
        cohort.cancel()
        if started:
            server.shutdown()
        server.server_close()
        if started:
            worker.join(timeout=CLEANUP_TIMEOUT)
        server.close_owned_connections()
        require(not started or not worker.is_alive(), "RELIABILITY_UPSTREAM_SERVER_CLEANUP")


def stream_one(connection, fixture):
    try:
        connection.request("POST", "/v1/messages", body=fixture.body, headers={
            "Content-Type": "application/json", "anthropic-version": "2023-06-01",
            "x-api-key": "synthetic-unused-provider-key", "x-claude-code-session-id": fixture.session})
        response = connection.getresponse()
        require(response.status == 200 and response.getheader("Content-Type", "").startswith("text/event-stream"),
                "RELIABILITY_STREAM_RESPONSE")
        require(response.getheader("request-id") == "req_archive_fixture"
                and response.getheader("anthropic-ratelimit-requests-remaining") == "42",
                "RELIABILITY_STREAM_HEADERS")
        first = bytearray()
        while not first.endswith(b"\n\n"):
            data = response.readline(MAX_BODY + 1)
            require(data, "RELIABILITY_STREAM_FIRST_EOF")
            first.extend(data)
            require(len(first) <= MAX_BODY, "RELIABILITY_STREAM_BYTES")
        require(bytes(first) == fixture.events[0] and fixture.first_sent.wait(1)
                and not fixture.completed.is_set() and not fixture.release.is_set(),
                "RELIABILITY_STREAM_BUFFERED")
        fixture.release.set()
        rest = response.read(MAX_BODY + 1)
        require(len(first) + len(rest) <= MAX_BODY
                and bytes(first) + rest == b"".join(fixture.events), "RELIABILITY_STREAM_BYTES")
        return len(first) + len(rest)
    finally:
        fixture.release.set()
        connection.close()


class OwnedConnection(http.client.HTTPConnection):
    """Retain only our socket for cancellation even after HTTPResponse adopts it."""
    def __init__(self, port):
        super().__init__("127.0.0.1", port, timeout=BATCH_TIMEOUT)
        self.socket_guard = threading.Lock()
        self.owned_socket = None
        self.cancelled = False

    def connect(self):
        with self.socket_guard:
            require(not self.cancelled, "RELIABILITY_CLIENT_CANCELLED")
        super().connect()
        with self.socket_guard:
            self.owned_socket = self.sock
            cancelled = self.cancelled
        if cancelled:
            self.cancel()
            raise JourneyError("RELIABILITY_CLIENT_CANCELLED")

    def cancel(self):
        with self.socket_guard:
            self.cancelled = True
            connection = self.owned_socket
        if connection is not None:
            try:
                connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            connection.close()
        self.close()


def run_batch(proxy_port, cohort, fixtures, *, observe_active=None):
    require(type(proxy_port) is int and 0 < proxy_port < 65536, "INVALID_LOOPBACK_PORT")
    require(len(fixtures) == len(cohort.sessions) and {item.session for item in fixtures} == set(cohort.sessions),
            "RELIABILITY_BATCH_SESSIONS")
    connections, workers = [], []
    errors, sizes = [None] * len(fixtures), [0] * len(fixtures)
    deadline = time.monotonic() + BATCH_TIMEOUT
    def run(index, connection, fixture):
        try:
            sizes[index] = stream_one(connection, fixture)
        except Exception:
            errors[index] = "RELIABILITY_CLIENT_FAILED"
    try:
        if observe_active is not None:
            cohort.observation_release.clear()
        for index, fixture in enumerate(fixtures):
            connection = OwnedConnection(proxy_port)
            connections.append(connection)
            worker = threading.Thread(target=run, args=(index, connection, fixture), daemon=True)
            worker.start()
            workers.append(worker)
        if observe_active is not None:
            require(cohort.all_active.wait(max(0, deadline - time.monotonic())), "RELIABILITY_CONCURRENCY_BARRIER_TIMEOUT")
            require(time.monotonic() < deadline, "RELIABILITY_ACTIVE_OBSERVATION_TIMEOUT")
            observe_active(deadline)
            require(time.monotonic() < deadline, "RELIABILITY_ACTIVE_OBSERVATION_TIMEOUT")
            cohort.observation_release.set()
        for worker in workers:
            worker.join(timeout=max(0, deadline - time.monotonic()))
        require(not any(worker.is_alive() for worker in workers), "RELIABILITY_CLIENT_BATCH_TIMEOUT")
        require(not any(errors), "RELIABILITY_CLIENT_FAILED")
        # A client may finish just before its server thread's finally block.
        while time.monotonic() < deadline:
            with cohort.lock:
                active = cohort.active
            if active == 0:
                break
            time.sleep(0.01)
        require(cohort.active == 0 and cohort.peak_active == len(fixtures)
                and len(cohort.claimed) == len(fixtures) and cohort.failures == 0
                and all(item.completed.is_set() and item.failure is None for item in fixtures),
                "RELIABILITY_UPSTREAM_ACCOUNTING")
        return {"completed": len(fixtures), "concurrent_sessions": cohort.peak_active,
                "request_bytes": sum(len(item.body) for item in fixtures), "response_bytes": sum(sizes)}
    finally:
        cohort.observation_release.set()
        alive = any(worker.is_alive() for worker in workers)
        if alive:
            cohort.cancel()
        for connection in connections:
            connection.cancel()
        cleanup_deadline = time.monotonic() + CLEANUP_TIMEOUT
        for worker in workers:
            worker.join(timeout=max(0, cleanup_deadline - time.monotonic()))
        require(not any(worker.is_alive() for worker in workers), "RELIABILITY_CLIENT_THREAD_CLEANUP")
