"""Bounded, owned synthetic upstream faults; no runtime work at import time."""
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler
import json
import socket
import threading
import time

from journey_process import JourneyError, require
from journey_reliability_fixture import BoundedServer, Cohort, MAX_BODY, CLEANUP_TIMEOUT
from journey_wire import Wire, MAX_HEADERS, exchange, response_head

TIMEOUT = 20
MAX_WORKERS = 2
MODES = ("baseline", "429", "after429", "500", "after500", "cancel", "aftercancel")


def request_bytes(port, fixture):
    require(type(port) is int and 0 < port < 65536, "FAULT_LOOPBACK_PORT")
    return (f"POST /v1/messages HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
            "Content-Type: application/json\r\nanthropic-version: 2023-06-01\r\n"
            "x-api-key: synthetic-unused-provider-key\r\n"
            f"x-claude-code-session-id: {fixture.session}\r\n"
            f"Content-Length: {len(fixture.body)}\r\nConnection: close\r\n\r\n").encode() + fixture.body


class Faults:
    def __init__(self):
        self.fixtures = Cohort(len(MODES)).prepare(0)
        self.lock = threading.Lock()
        self.failures = 0
        self.cancelled = threading.Event()
        self.peer_closed = threading.Event()
        self.attempts = {mode: 0 for mode in MODES}
        for mode, fixture in zip(MODES, self.fixtures):
            fixture.mode = mode
            fixture.headers = {"request-id": "req_" + fixture.session,
                               "anthropic-ratelimit-requests-remaining": "0", "retry-after": "1"}
            if mode in ("429", "500"):
                value = json.loads(fixture.body)
                value["stream"] = False
                fixture.body = json.dumps(value, separators=(",", ":")).encode()
                fixture.error_body = json.dumps({"type": "error", "error": {
                    "type": "rate_limit_error" if mode == "429" else "api_error",
                    "message": "Synthetic owned fault " + fixture.session}}, separators=(",", ":")).encode()
        self.by_body = {item.body: item for item in self.fixtures}

    def claim(self, body):
        with self.lock:
            require(not self.cancelled.is_set() and body in self.by_body, "FAULT_UNEXPECTED_UPSTREAM_BODY")
            fixture = self.by_body[body]
            self.attempts[fixture.mode] += 1
            require(self.attempts[fixture.mode] == 1, "FAULT_UNEXPECTED_UPSTREAM_RETRY")
            return fixture

    def wait_peer_close(self, connection, deadline):
        # Never release later events or end the stream to help the product notice
        # cancellation. A bounded read observes upstream TCP EOF/reset only.
        while time.monotonic() < deadline and not self.cancelled.is_set():
            connection.settimeout(min(0.2, max(0.001, deadline - time.monotonic())))
            try:
                data = connection.recv(1)
            except socket.timeout:
                continue
            except ConnectionResetError:
                data = b""
            require(time.monotonic() < deadline and not self.cancelled.is_set(), "FAULT_STALLED_UPSTREAM_NOT_CANCELLED")
            require(not data, "FAULT_UNEXPECTED_UPSTREAM_BYTES")
            self.peer_closed.set()
            return
        raise JourneyError("FAULT_STALLED_UPSTREAM_NOT_CANCELLED")

    def handler(self):
        faults = self
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def setup(self):
                super().setup()
                self.connection.settimeout(TIMEOUT)
                self.deadline = time.monotonic() + TIMEOUT
                self.expiry = threading.Timer(TIMEOUT, self.expire)
                self.expiry.daemon = True
                self.expiry.start()
            def expire(self):
                try:
                    self.connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            def finish(self):
                self.expiry.cancel()
                self.expiry.join(timeout=CLEANUP_TIMEOUT)
                super().finish()
            def log_message(self, *_args):
                pass
            def do_POST(self):
                try:
                    require(self.path == "/v1/messages" and self.headers.get("Transfer-Encoding") is None,
                            "FAULT_UPSTREAM_PATH_OR_FRAMING")
                    lengths = self.headers.get_all("Content-Length", [])
                    require(len(lengths) == 1 and lengths[0].isdigit(), "FAULT_UPSTREAM_LENGTH")
                    length = int(lengths[0])
                    require(0 < length <= MAX_BODY, "FAULT_UPSTREAM_BODY_LIMIT")
                    body = self.rfile.read(length)
                    require(len(body) == length, "FAULT_UPSTREAM_TRUNCATED_BODY")
                    fixture = faults.claim(body)
                    fixture.received = body
                    error = fixture.mode in ("429", "500")
                    self.send_response(int(fixture.mode) if error else 200)
                    self.send_header("Content-Type", "application/json" if error else "text/event-stream")
                    for key, value in fixture.headers.items():
                        self.send_header(key, value)
                    if error:
                        self.send_header("Content-Length", str(len(fixture.error_body)))
                    else:
                        self.send_header("Transfer-Encoding", "chunked")
                    self.end_headers()
                    if error:
                        self.wfile.write(fixture.error_body)
                        self.wfile.flush()
                    else:
                        events = fixture.events[:3] if fixture.mode == "cancel" else fixture.events
                        for event in events:
                            self.wfile.write(f"{len(event):x}\r\n".encode() + event + b"\r\n")
                            self.wfile.flush()
                        fixture.first_sent.set()
                        if fixture.mode == "cancel":
                            faults.wait_peer_close(self.connection, self.deadline)
                        else:
                            self.wfile.write(b"0\r\n\r\n")
                            self.wfile.flush()
                    fixture.completed.set()
                except Exception:
                    with faults.lock:
                        faults.failures += 1
                finally:
                    self.close_connection = True
        return Handler


@contextmanager
def fault_fixture():
    faults = Faults()
    server = BoundedServer(("127.0.0.1", 0), faults.handler())
    server.slots = threading.BoundedSemaphore(MAX_WORKERS)
    worker = None
    started = False
    try:
        worker = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
        worker.start()
        started = True
        yield faults, server.server_port
    finally:
        faults.cancelled.set()
        if started:
            server.shutdown()
        server.server_close()
        if started:
            worker.join(timeout=CLEANUP_TIMEOUT)
        server.close_owned_connections()
        require(not started or not worker.is_alive(), "FAULT_SERVER_CLEANUP")


def validate_wire(status, headers, body, fixture):
    error = fixture.mode in ("429", "500")
    require(status == (int(fixture.mode) if error else 200), "FAULT_RESPONSE_STATUS")
    require(all(headers.get(key) == [value] for key, value in fixture.headers.items()), "FAULT_RESPONSE_HEADERS")
    require(body == (fixture.error_body if error else b"".join(fixture.events)), "FAULT_RESPONSE_BYTES")
    return {"status": status, "exact_body": True, "forwarded_provider_rate_retry_headers": True,
            "body_sha256": hashlib.sha256(body).hexdigest()}


def cancel_stream(port, fixture, faults):
    deadline = time.monotonic() + TIMEOUT
    with socket.create_connection(("127.0.0.1", port), timeout=TIMEOUT) as connection:
        connection.sendall(request_bytes(port, fixture))
        wire = Wire(connection, deadline)
        status, headers = response_head(wire.until(b"\r\n\r\n", MAX_HEADERS))
        require(status == 200 and headers.get("transfer-encoding") == ["chunked"], "FAULT_CANCEL_RESPONSE")
        for key, value in fixture.headers.items():
            require(headers.get(key) == [value], "FAULT_CANCEL_HEADERS")
        # Chunk boundaries may change in the proxy. Require three complete SSE
        # events, never a buffering assumption about individual HTTP chunks.
        observed = bytearray()
        expected = b"".join(fixture.events[:3])
        while len(observed) < len(expected):
            size = wire.until(b"\r\n", 32)
            require(size and len(size) <= 8 and all(c in b"0123456789abcdefABCDEF" for c in size),
                    "FAULT_CANCEL_CHUNK")
            length = int(size, 16)
            require(0 < length <= MAX_BODY - len(observed), "FAULT_CANCEL_BODY_LIMIT")
            observed.extend(wire.exact(length))
            require(wire.exact(2) == b"\r\n", "FAULT_CANCEL_CHUNK")
        require(bytes(observed) == expected and fixture.first_sent.wait(1)
                and not faults.peer_closed.is_set() and not fixture.completed.is_set(), "FAULT_CANCEL_NOT_STALLED")
        connection.shutdown(socket.SHUT_RDWR)
    require(faults.peer_closed.wait(max(0, deadline - time.monotonic())), "FAULT_CANCEL_NOT_PROPAGATED")
    return {"client_observed_partial_text": True, "upstream_stalled_until_peer_close": True,
            "upstream_peer_close_observed": True, "partial_sse_sha256": hashlib.sha256(expected).hexdigest()}


def run_request(port, fixture, faults):
    observation = (cancel_stream(port, fixture, faults) if fixture.mode == "cancel" else
                   validate_wire(*exchange(port, request_bytes(port, fixture), timeout=TIMEOUT), fixture))
    require(fixture.completed.wait(1) and faults.failures == 0
            and faults.attempts[fixture.mode] == 1, "FAULT_UPSTREAM_ACCOUNTING")
    return dict(observation, upstream_attempts=1, mode=fixture.mode)
