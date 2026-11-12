"""Synthetic default-worker triggers; imports and fixture preparation open no sockets."""
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import threading
import uuid

from journey_fixture import TEXT, event
from journey_process import require
from journey_reliability import validate_response
from journey_reliability_fixture import BoundedServer, CLEANUP_TIMEOUT, Cohort, MAX_BODY


def synthetic_project(root):
    """Create only an owned empty directory and generated identity, never an operator repo."""
    root = Path(root).resolve(strict=True)
    project = root / "synthetic-project"
    project.mkdir(mode=0o700)
    return {"root": str(project), "project_uuid": str(uuid.uuid4()),
            "machine_uuid": str(uuid.uuid4()), "machine_name": "archive-synthetic-machine"}


class NormalCohort(Cohort):
    def __init__(self, count, project):
        super().__init__(count)
        root = project["root"]
        require(isinstance(root, str) and root.startswith("/")
                and all(char.isalnum() or char in "/_.-" for char in root),
                "NORMAL_DEFAULT_PROJECT_PATH")
        self.project = dict(project)

    def prepare(self, batch):
        fixtures = super().prepare(batch)
        with self.lock:
            for fixture in fixtures:
                value = json.loads(fixture.body)
                value["system"] = "Working directory: " + self.project["root"]
                fixture.body = json.dumps(value, separators=(",", ":")).encode()
                suffix = fixture.session.replace("-", "") + "_" + str(batch)
                fixture.lesson_topic = "archive-synthetic-" + suffix
                fixture.lesson_text = "Synthetic durable lesson for " + suffix + "."
                fixture.response_text = (TEXT + '\n<mem:update topic="' + fixture.lesson_topic
                                         + '">' + fixture.lesson_text + '</mem:update>')
                fixture.events[2] = event("content_block_delta", {
                    "type": "content_block_delta", "index": 0,
                    "delta": {"type": "text_delta", "text": fixture.response_text}})
                require(len(fixture.body) <= MAX_BODY and sum(map(len, fixture.events)) <= MAX_BODY,
                        "RELIABILITY_FIXTURE_BYTES")
            self.pending = {hashlib.sha256(item.body).digest(): item for item in fixtures}
            require(len(self.pending) == len(self.sessions), "RELIABILITY_DUPLICATE_FIXTURE")
        return fixtures


def validate_normal_response(body, fixture):
    value = json.loads(body)
    require(isinstance(value, dict) and isinstance(value.get("content"), list)
            and len(value["content"]) == 2
            and value["content"][0] == {"type": "text", "text": fixture.response_text},
            "NORMAL_DEFAULT_RESPONSE_TRIGGER_CHANGED")
    value["content"][0]["text"] = TEXT
    validate_response(json.dumps(value).encode(), fixture)


@contextmanager
def owned_server(server):
    worker = None
    started = False
    try:
        worker = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
        worker.start()
        started = True
        yield server
    finally:
        try:
            if started:
                server.shutdown()
        finally:
            try:
                server.server_close()
            finally:
                try:
                    if started:
                        worker.join(timeout=CLEANUP_TIMEOUT)
                finally:
                    server.close_owned_connections()
                    require(not started or not worker.is_alive(), "NORMAL_DEFAULT_SERVER_CLEANUP")


@contextmanager
def normal_fixture(count, project):
    cohort = NormalCohort(count, project)
    try:
        with owned_server(BoundedServer(("127.0.0.1", 0), cohort.handler())) as server:
            try:
                yield cohort, server.server_port
            finally:
                cohort.cancel()
    finally:
        cohort.cancel()


@contextmanager
def normal_services():
    """Owned services endpoint; any core-workload call is an unexpected dependency."""
    from http.server import BaseHTTPRequestHandler
    from journey_reliability_services import ServiceGuard
    contract = ServiceGuard(request_limit=4096)
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass
        def setup(self):
            super().setup()
            self.connection.settimeout(2)
        def handle_one_request(self):
            # Core traffic never needs this server. Count every parsed request,
            # including unsupported verbs, before replying with a bounded error.
            self.raw_requestline = self.rfile.readline(8193)
            if not self.raw_requestline:
                self.close_connection = True
                return
            try:
                contract.respond("UNEXPECTED", "/")
            except Exception:
                pass
            self.close_connection = True
            if len(self.raw_requestline) <= 8192 and self.parse_request():
                self.send_error(503, "Synthetic dependency is not part of this workload")
    with owned_server(BoundedServer(("127.0.0.1", 0), Handler)) as server:
        yield contract, server.server_port
