"""Synthetic SSE fixture with client-observed synchronization, never a real provider."""
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading
import uuid
from journey_process import JourneyError, require

MODEL = "claude-sonnet-4-20250514"
TEXT = "MIKE_ARCHIVE_SYNTHETIC_TEXT_275_280"
TOOL_RESULT = "MIKE_ARCHIVE_SYNTHETIC_TOOL_RESULT"
TOOL_NAME = "archive_fixture_echo"
SECRET = "MIKE_ARCHIVE_FAKE_SECRET_MUST_NOT_LEAK"

def event(name, value):
    return ("event: " + name + "\ndata: " + json.dumps(value, separators=(",", ":")) + "\n\n").encode()

def sse_events():
    values = [
        ("message_start", {"type": "message_start", "message": {"id": "msg_archive_fixture", "type": "message",
             "role": "assistant", "content": [], "model": MODEL, "stop_reason": None, "stop_sequence": None,
             "usage": {"input_tokens": 25, "output_tokens": 0}}}),
        ("content_block_start", {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        ("content_block_delta", {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": TEXT}}),
        ("content_block_stop", {"type": "content_block_stop", "index": 0}),
        ("content_block_start", {"type": "content_block_start", "index": 1,
             "content_block": {"type": "tool_use", "id": "tool_archive_response", "name": TOOL_NAME, "input": {}}}),
        ("content_block_delta", {"type": "content_block_delta", "index": 1,
             "delta": {"type": "input_json_delta", "partial_json": '{"echo":"synthetic"}'}}),
        ("content_block_stop", {"type": "content_block_stop", "index": 1}),
        ("message_delta", {"type": "message_delta", "delta": {"stop_reason": "tool_use", "stop_sequence": None},
             "usage": {"output_tokens": 12}}),
        ("message_stop", {"type": "message_stop"}),
    ]
    return [event(name, value) for name, value in values]

def request_body():
    value = {"model": MODEL, "max_tokens": 128, "stream": True,
        "tools": [{"name": TOOL_NAME, "description": "Synthetic test tool; never executed", "input_schema": {"type": "object"}}],
        "messages": [
            {"role": "user", "content": "Exercise the archive fixture"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "tool_archive_prior", "name": TOOL_NAME, "input": {}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "tool_archive_prior", "content": TOOL_RESULT}]},
        ]}
    return json.dumps(value, separators=(",", ":")).encode()

class Fixture:
    def __init__(self):
        self.session = str(uuid.uuid4())
        self.events = sse_events()
        self.body = request_body()
        self.release = threading.Event()
        self.first_sent = threading.Event()
        self.completed = threading.Event()
        self.received = None
        self.failure = None

    def handler(self):
        fixture = self
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def setup(self):
                super().setup()
                self.connection.settimeout(10)
            def log_message(self, *_args):
                pass
            def do_POST(self):
                try:
                    require(self.path == "/v1/messages", "UPSTREAM_PATH")
                    length = int(self.headers.get("Content-Length", "0"))
                    require(0 < length <= 1024 * 1024, "UPSTREAM_BODY_SIZE")
                    fixture.received = self.rfile.read(length)
                    require(fixture.received == fixture.body, "OUTBOUND_BODY_MUTATED")
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Transfer-Encoding", "chunked")
                    self.send_header("request-id", "req_archive_fixture")
                    self.send_header("anthropic-ratelimit-requests-remaining", "42")
                    self.end_headers()
                    self.chunk(fixture.events[0])
                    fixture.first_sent.set()
                    require(fixture.release.wait(10), "CLIENT_DID_NOT_OBSERVE_FIRST_EVENT")
                    for data in fixture.events[1:]:
                        self.chunk(data)
                    self.wfile.write(b"0\r\n\r\n")
                    self.wfile.flush()
                    fixture.completed.set()
                except Exception as error:
                    fixture.failure = type(error).__name__
                    self.close_connection = True
            def chunk(self, data):
                self.wfile.write((f"{len(data):x}\r\n").encode() + data + b"\r\n")
                self.wfile.flush()
        return Handler

    def evidence(self):
        return {"request_sha256": hashlib.sha256(self.body).hexdigest(),
                "response_sha256": hashlib.sha256(b"".join(self.events)).hexdigest(),
                "event_count": len(self.events), "fixture_kind": "synthetic_text_tool_use_usage"}

@contextmanager
def upstream_fixture():
    fixture = Fixture()
    server = ThreadingHTTPServer(("127.0.0.1", 0), fixture.handler())
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield fixture, server.server_port
    finally:
        fixture.release.set()
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)
        if thread.is_alive():
            raise JourneyError("FIXTURE_THREAD_CLEANUP")
