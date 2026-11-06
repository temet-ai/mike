"""Synthetic streaming and HTTP metadata observations, never browser evidence."""
import hashlib
from html.parser import HTMLParser
import http.client
import json
import re
import time
from urllib.parse import urlsplit

from journey_fixture import MODEL, TEXT, TOOL_NAME
from journey_process import JourneyError, http_bytes, http_json, require


def stream_capture(proxy_port, fixture):
    connection = http.client.HTTPConnection("127.0.0.1", proxy_port, timeout=6)
    try:
        connection.request("POST", "/v1/messages", body=fixture.body, headers={
            "Content-Type": "application/json", "anthropic-version": "2023-06-01",
            "x-api-key": "synthetic-unused-provider-key", "x-claude-code-session-id": fixture.session})
        response = connection.getresponse()
        require(response.status == 200, "STREAM_STATUS")
        require(response.getheader("Content-Type", "").startswith("text/event-stream"), "STREAM_CONTENT_TYPE")
        require(response.getheader("request-id") == "req_archive_fixture"
                and response.getheader("anthropic-ratelimit-requests-remaining") == "42", "STREAM_HEADERS")
        first = bytearray()
        while not first.endswith(b"\n\n"):
            line = response.readline(1024 * 1024)
            require(line, "STREAM_FIRST_EVENT_EOF")
            first.extend(line)
            require(len(first) <= 1024 * 1024, "STREAM_EVENT_LIMIT")
        require(bytes(first) == fixture.events[0], "STREAM_FIRST_EVENT_MISMATCH")
        require(fixture.first_sent.wait(1) and not fixture.release.is_set()
                and not fixture.completed.is_set(), "STREAM_WAS_BUFFERED")
        fixture.release.set()
        rest = response.read(4 * 1024 * 1024 + 1)
        require(len(rest) <= 4 * 1024 * 1024, "STREAM_RESPONSE_LIMIT")
        require(bytes(first) + rest == b"".join(fixture.events), "STREAM_BYTES_OR_ORDER_CHANGED")
        require(fixture.completed.wait(1) and fixture.failure is None, "UPSTREAM_FIXTURE_FAILED")
        return dict(fixture.evidence(), first_event_before_release=True,
                    status_and_documented_headers_preserved=True, response_order_identical=True)
    finally:
        fixture.release.set()
        connection.close()


def validate_detail(detail, fixture, request_id):
    require(detail.get("id") == request_id and detail.get("session_id") == fixture.session, "CAPTURE_IDENTITY")
    require(detail.get("status") == "Complete" and detail.get("is_streaming") is True
            and detail.get("response_status") == 200 and detail.get("completed_at"), "CAPTURE_NOT_COMPLETE")
    require(detail.get("model") == MODEL and detail.get("stop_reason") == "tool_use", "CAPTURE_METADATA")
    usage = detail.get("token_usage") or {}
    require(usage.get("input_tokens") == 25 and usage.get("output_tokens") == 12, "CAPTURE_USAGE")
    tools = detail.get("tool_calls", [])
    require(any(tool.get("name") == TOOL_NAME and tool.get("complete") is True
                and json.loads(tool.get("input_json", "null")) == {"echo": "synthetic"}
                for tool in tools), "CAPTURE_TOOL_USE")


def inspect_capture(inspect_port, fixture, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        rows = http_json(inspect_port, "/api/requests")
        require(isinstance(rows, list), "CAPTURE_LIST_SHAPE")
        matching = [row for row in rows if row.get("session_id") == fixture.session]
        require(len(matching) <= 1, "CAPTURE_DUPLICATED")
        if matching and matching[0].get("status") == "Complete":
            request_id = matching[0]["id"]
            detail = http_json(inspect_port, f"/api/requests/{request_id}")
            validate_detail(detail, fixture, request_id)
            body = http_json(inspect_port, f"/api/requests/{request_id}/body/request")
            require(body.get("has_body") is True and body.get("text", "").encode() == fixture.body,
                    "CAPTURE_REQUEST_BODY_CHANGED")
            require(body.get("reconstructed") is False, "CAPTURE_REQUEST_BODY_RECONSTRUCTED")
            return request_id, {"session_matches": True, "request_count": 1, "terminal_state": "Complete",
                                "request_body_sha256": hashlib.sha256(fixture.body).hexdigest(),
                                "tool_use_complete": True, "input_tokens": 25, "output_tokens": 12}
        time.sleep(0.05)
    raise JourneyError("CAPTURE_COMPLETION_TIMEOUT")


class AssetLinks(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = set()
    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        if tag == "script" and attributes.get("src"):
            self.links.add(attributes["src"])
        if tag == "link" and attributes.get("href") and attributes.get("rel") in ("stylesheet", "modulepreload", "preload"):
            self.links.add(attributes["href"])


def asset_paths(html):
    parser = AssetLinks()
    parser.feed(html)
    parser.links.update(match[1] for match in re.findall(r'import\((["\'])([^"\']+)\1\)', html))
    paths = []
    for link in sorted(parser.links):
        parsed = urlsplit(link)
        require(not parsed.scheme and not parsed.netloc and not link.startswith("//"), "SPA_EXTERNAL_ASSET")
        path = "/" + parsed.path.removeprefix("./").lstrip("/")
        require(".." not in path.split("/") and path.startswith("/_app/"), "SPA_UNEXPECTED_ASSET")
        if path.endswith((".js", ".css")):
            paths.append(path)
    require(any(path.endswith(".js") for path in paths), "SPA_SCRIPT_MISSING")
    require(any(path.endswith(".css") for path in paths), "SPA_STYLESHEET_MISSING")
    return paths


def spa_http(spa_port, inspect_port, proxy_port, session, request_id):
    status, headers, body = http_bytes(spa_port, "/")
    require(status == 200 and "text/html" in headers.get("content-type", headers.get("Content-Type", "")), "SPA_HTML")
    html = body.decode()
    require("/@vite" not in html and "localhost:5173" not in html, "SPA_DEV_SERVER_DEPENDENCY")
    assets = []
    for path in asset_paths(html):
        status, headers, data = http_bytes(spa_port, path)
        content_type = headers.get("content-type", headers.get("Content-Type", ""))
        expected = "css" if path.endswith(".css") else "javascript"
        require(status == 200 and expected in content_type and data, "SPA_ASSET_RESPONSE")
        assets.append({"path": path, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)})
    deep = f"/sessions/{session}/requests/{request_id}"
    status, headers, body = http_bytes(spa_port, deep)
    require(status == 200 and "text/html" in headers.get("content-type", headers.get("Content-Type", "")), "SPA_DEEP_LINK_HTTP")
    info = http_json(spa_port, "/api/connection-info")
    for key, port in (("proxy_url", proxy_port), ("inspect_url", inspect_port), ("browser_url", spa_port)):
        require(info.get(key) == f"http://127.0.0.1:{port}", "SPA_SELECTED_ENDPOINT_MISMATCH")
    captured = http_json(spa_port, f"/api/requests/{request_id}")
    require(captured.get("session_id") == session, "SPA_SAME_ORIGIN_API")
    return {"kind": "HTTP_ONLY_NOT_BROWSER", "assets": assets, "deep_link_http": True,
            "same_origin_api": True, "selected_ports_match": True, "browser_execution": "NOT_RUN"}
