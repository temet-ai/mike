"""Raw installed-listener boundary probes. Partial helpers, never AC06 qualification."""
import base64
import hashlib
import json

from journey_fixture import SECRET, TEXT, TOOL_RESULT
from journey_process import JourneyError, require
from journey_wire import exchange

WS_KEY = "dGhlIHNhbXBsZSBub25jZQ=="
WS_ACCEPT = base64.b64encode(hashlib.sha1(
    (WS_KEY + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()


def probe(name, action):
    """Preserve the predefined probe identity on failure, never response/log text."""
    try:
        return action()
    except Exception as error:
        reason = str(error) if isinstance(error, JourneyError) else type(error).__name__
        raise JourneyError("BOUNDARY_PROBE_" + name.upper() + "_" + reason) from error


def request_bytes(path, headers, *, version="HTTP/1.1", websocket=False):
    require(version in ("HTTP/1.0", "HTTP/1.1") and "\r" not in path and "\n" not in path,
            "BOUNDARY_REQUEST_LINE")
    values = list(headers)
    if websocket:
        values.extend([("Connection", "Upgrade"), ("Upgrade", "websocket"),
                       ("Sec-WebSocket-Version", "13"), ("Sec-WebSocket-Key", WS_KEY)])
    else:
        values.append(("Connection", "close"))
    for name, value in values:
        require(all(char not in name + value for char in "\r\n"), "BOUNDARY_REQUEST_HEADER")
    return ("GET " + path + " " + version + "\r\n"
            + "".join(name + ": " + value + "\r\n" for name, value in values) + "\r\n").encode("ascii")


def rejected_cases(port, path):
    host = ("Host", f"127.0.0.1:{port}")
    local_origin = f"http://127.0.0.1:{port}"
    other_port = port + 1 if port < 65535 else port - 1
    values = [
        ("forged_host", [("Host", f"unrelated.example:{port}")]),
        ("confusable_host", [("Host", f"localhost.evil:{port}")]),
        ("wrong_listener_port", [("Host", f"127.0.0.1:{other_port}")]),
        ("duplicate_host", [host, host]),
        ("disallowed_origin", [host, ("Origin", "http://unrelated.example")]),
        ("null_origin", [host, ("Origin", "null")]),
        ("wrong_origin_scheme", [host, ("Origin", f"https://127.0.0.1:{port}")]),
        ("malformed_origin", [host, ("Origin", local_origin + "/")]),
        ("duplicate_origin", [host, ("Origin", local_origin), ("Origin", local_origin)]),
        ("originless_cross_site", [host, ("Sec-Fetch-Site", "cross-site")]),
        ("originless_same_site", [host, ("Sec-Fetch-Site", "same-site")]),
        ("invalid_fetch_metadata", [host, ("Sec-Fetch-Site", "invalid")]),
        ("duplicate_fetch_metadata", [host, ("Sec-Fetch-Site", "same-origin"), ("Sec-Fetch-Site", "none")]),
        ("forwarded_headers_do_not_authorize_origin", [host, ("Origin", f"https://127.0.0.1:{port}"),
             ("X-Forwarded-Proto", "https"), ("X-Forwarded-Host", "unrelated.example")]),
    ]
    cases = [{"id": name, "path": path, "headers": headers, "version": "HTTP/1.1"}
             for name, headers in values]
    cases.extend([
        {"id": "missing_host_http11", "path": path, "headers": [], "version": "HTTP/1.1"},
        {"id": "missing_host_http10", "path": path, "headers": [], "version": "HTTP/1.0"},
        {"id": "absolute_authority_without_host_http10", "path": local_origin + path,
         "headers": [], "version": "HTTP/1.0"},
        {"id": "conflicting_absolute_authority", "path": "http://unrelated.example" + path,
         "headers": [host], "version": "HTTP/1.1"},
    ])
    return cases


def assert_rejected(response, fixture):
    status, headers, body = response
    require(status in (400, 403), "BOUNDARY_UNTRUSTED_REQUEST_NOT_REJECTED")
    require("sec-websocket-accept" not in headers, "BOUNDARY_REJECTED_REQUEST_UPGRADED")
    require("access-control-allow-origin" not in headers, "BOUNDARY_REJECTED_REQUEST_CORS")
    private = (SECRET.encode(), TEXT.encode(), TOOL_RESULT.encode(), fixture.session.encode(), fixture.body)
    wire_content = body + json.dumps(headers, sort_keys=True).encode()
    require(all(marker not in wire_content for marker in private), "BOUNDARY_REJECTED_PRIVATE_CONTENT")
    if status == 403:
        value = json.loads(body)
        require(isinstance(value, dict) and set(value) == {"error"}
                and isinstance(value["error"], str) and bool(value["error"]), "BOUNDARY_REJECTION_SCHEMA")
    return {"status": status, "planted_content_absent": True, "no_upgrade_or_cors_allowance": True}


def allowed_headers(port):
    host = ("Host", f"127.0.0.1:{port}")
    return [
        ("native_no_origin", [host]),
        ("same_origin", [host, ("Origin", f"http://127.0.0.1:{port}")]),
        ("originless_same_origin_metadata", [host, ("Sec-Fetch-Site", "same-origin")]),
        ("originless_none_metadata", [host, ("Sec-Fetch-Site", "none")]),
    ]


def assert_local_capture(response, fixture):
    status, _, body = response
    require(status == 200, "BOUNDARY_LEGITIMATE_READ_REJECTED")
    value = json.loads(body)
    require(isinstance(value, dict) and value.get("has_body") is True
            and isinstance(value.get("text"), str) and value["text"].encode() == fixture.body,
            "BOUNDARY_LEGITIMATE_CAPTURE_MISMATCH")


def raw_http_boundary(port, request_id, fixture):
    require(type(request_id) is int and request_id > 0, "BOUNDARY_CAPTURE_ID")
    path = f"/api/requests/{request_id}/body/request"
    accepted = []
    for name, headers in allowed_headers(port):
        probe(name, lambda: assert_local_capture(exchange(port, request_bytes(path, headers)), fixture))
        accepted.append({"id": name, "status": 200, "matching_stored_request": True})
    rejected = []
    for case in rejected_cases(port, path):
        def rejected_read():
            response = exchange(port, request_bytes(case["path"], case["headers"], version=case["version"]))
            return assert_rejected(response, fixture)
        rejected.append({"id": case["id"], **probe(case["id"], rejected_read)})
    probe("native_after_rejections", lambda: assert_local_capture(
        exchange(port, request_bytes(path, [("Host", f"127.0.0.1:{port}")])), fixture))
    return {"transport": "raw_loopback_http_bytes", "accepted": accepted, "rejected": rejected,
            "legitimate_capture_read_after_rejections": True, "browser_execution": "NOT_RUN"}


def assert_upgrade(response):
    status, headers, _ = response
    require(status == 101, "BOUNDARY_LEGITIMATE_WEBSOCKET_REJECTED")
    require(headers.get("sec-websocket-accept") == [WS_ACCEPT]
            and [value.lower() for value in headers.get("upgrade", [])] == ["websocket"],
            "BOUNDARY_WEBSOCKET_HANDSHAKE_INVALID")
    connections = headers.get("connection", [])
    require(len(connections) == 1 and "upgrade" in [token.strip().lower() for token in connections[0].split(",")],
            "BOUNDARY_WEBSOCKET_CONNECTION_HEADER")


def raw_websocket_boundary(port, fixture):
    accepted = []
    for name, headers in allowed_headers(port):
        probe(name, lambda: assert_upgrade(exchange(port, request_bytes("/api/ws", headers, websocket=True))))
        accepted.append({"id": name, "status": 101, "rfc6455_accept_matches": True})
    rejected = []
    for case in rejected_cases(port, "/api/ws"):
        def rejected_upgrade():
            response = exchange(port, request_bytes(case["path"], case["headers"], version=case["version"], websocket=True))
            return assert_rejected(response, fixture)
        rejected.append({"id": case["id"], **probe(case["id"], rejected_upgrade)})
    probe("native_after_rejections", lambda: assert_upgrade(
        exchange(port, request_bytes("/api/ws", [("Host", f"127.0.0.1:{port}")], websocket=True))))
    return {"transport": "raw_loopback_websocket_upgrade", "accepted": accepted, "rejected": rejected,
            "legitimate_upgrade_after_rejections": True, "websocket_message_capture": "NOT_RUN",
            "browser_execution": "NOT_RUN"}
