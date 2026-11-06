"""Offline raw-boundary verifier regressions; fake byte streams, no sockets/daemon."""
import json
from unittest.mock import Mock, patch
import unittest

from journey import new_result
from journey_fixture import Fixture, SECRET, TEXT, TOOL_RESULT
from journey_process import JourneyError
from journey_security import (WS_ACCEPT, assert_rejected, assert_upgrade, raw_http_boundary,
                              raw_websocket_boundary, rejected_cases, request_bytes)
from journey_wire import MAX_BODY, MAX_HEADERS, MAX_WIRE, Wire, exchange


class FakeSocket:
    def __init__(self, pieces):
        self.pieces = iter(pieces)
        self.sent = []
        self.closed = False
        self.timeouts = []
    def __enter__(self):
        return self
    def __exit__(self, *_arguments):
        self.closed = True
    def settimeout(self, value):
        self.timeouts.append(value)
    def sendall(self, value):
        self.sent.append(value)
    def recv(self, _limit):
        piece = next(self.pieces, b"")
        if isinstance(piece, Exception):
            raise piece
        return piece


class RawWireTests(unittest.TestCase):
    def raw(self, pieces, request=b"GET /api/requests HTTP/1.0\r\n\r\n"):
        connection = FakeSocket(pieces)
        with patch("journey_wire.socket.create_connection", return_value=connection) as connect:
            result = exchange(10001, request)
        connect.assert_called_once_with(("127.0.0.1", 10001), timeout=5)
        self.assertEqual(connection.sent, [request])
        self.assertTrue(connection.closed)
        self.assertTrue(all(0 < value <= 5 for value in connection.timeouts))
        return result

    def test_raw_missing_host_and_duplicate_headers_are_sent_without_normalization(self):
        for request in (b"GET /api/requests HTTP/1.0\r\n\r\n",
                        request_bytes("/api/requests", [("Host", "first"), ("Host", "second")])):
            response = self.raw([b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n"], request)
            self.assertEqual(response[0], 400)
        self.assertEqual(request.count(b"Host:"), 2)

    def test_split_content_length_chunked_and_close_delimited_responses(self):
        frames = [
            [b"HTTP/1.1 403 Forbidden\r\nContent-Len", b"gth: 3\r\n\r\n", b"abc"],
            [b"HTTP/1.1 403 Forbidden\r\nTransfer-Encoding: chunked\r\n\r\n",
             b"1\r\na\r\n2;fixture=1\r\nbc\r\n0\r\n\r\n"],
            [b"HTTP/1.0 403 Forbidden\r\n\r\nab", b"c", b""],
        ]
        for pieces in frames:
            with self.subTest(pieces=pieces):
                self.assertEqual(self.raw(pieces)[2], b"abc")

    def test_upgrade_returns_at_headers_and_never_waits_for_live_frames(self):
        response = self.raw([("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n"
                             "Connection: Upgrade\r\nSec-WebSocket-Accept: " + WS_ACCEPT + "\r\n\r\n").encode(),
                             AssertionError("must not read WebSocket frames")])
        assert_upgrade(response)

    def test_malformed_truncated_oversized_or_ambiguous_responses_fail_closed(self):
        frames = [b"not HTTP\r\n\r\n", b"HTTP/1.1 403 Forbidden\r\n folded: invalid\r\n\r\n",
                  b"HTTP/1.1 403 Forbidden\r\nContent-Length: 3\r\n\r\nx",
                  b"HTTP/1.1 403 Forbidden\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
                  b"HTTP/1.1 403 Forbidden\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\nx",
                  b"HTTP/1.1 403 Forbidden\r\nTransfer-Encoding: gzip\r\n\r\nx",
                  b"HTTP/1.1 403 Forbidden\r\nContent-Length: -1\r\n\r\n",
                  b"HTTP/1.1 403 Forbidden\r\nContent-Length: " + str(MAX_BODY + 1).encode() + b"\r\n\r\n",
                  b"HTTP/1.1 403 Forbidden\r\n" + b"x" * (MAX_HEADERS + 1),
                  b"HTTP/1.1 403 Forbidden\r\nTransfer-Encoding: chunked\r\n\r\nZ\r\nx\r\n",
                  b"HTTP/1.1 403 Forbidden\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nxZZ",
                  b"HTTP/1.1 403 Forbidden\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX-Private: hidden\r\n\r\n",
                  b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\nhidden trailing bytes",
                  b"HTTP/1.1 100 Continue\r\n\r\n",
                  b"HTTP/1.1 101 Switching Protocols\r\nContent-Length: 1\r\n\r\nx"]
        for value in frames:
            with self.subTest(value=value[:100]), self.assertRaises(JourneyError):
                self.raw([value])

    def test_timeout_and_total_wire_limits_are_bounded_and_close_owned_socket(self):
        connection = FakeSocket([TimeoutError("private timeout context")])
        with patch("journey_wire.socket.create_connection", return_value=connection):
            with self.assertRaisesRegex(JourneyError, "RESPONSE_TIMEOUT"):
                exchange(10001, b"GET / HTTP/1.0\r\n\r\n")
        self.assertTrue(connection.closed)
        wire = Wire(FakeSocket([b"x" * (MAX_WIRE + 1)]), 100)
        with patch("journey_wire.time.monotonic", return_value=0):
            with self.assertRaisesRegex(JourneyError, "WIRE_LIMIT"):
                wire.receive()
        wire = Wire(FakeSocket([]), 1)
        with patch("journey_wire.time.monotonic", return_value=2):
            with self.assertRaisesRegex(JourneyError, "RESPONSE_TIMEOUT"):
                wire.receive()

    def test_invalid_port_or_request_never_opens_a_socket(self):
        for port, request in ((True, b"x"), (0, b"x"), (65536, b"x"), (10001, "not bytes"),
                              (10001, b""), (10001, b"x" * (MAX_HEADERS + 1))):
            with self.subTest(port=port), patch("journey_wire.socket.create_connection") as connect:
                with self.assertRaises(JourneyError):
                    exchange(port, request)
                connect.assert_not_called()


class BoundaryPolicyTests(unittest.TestCase):
    def setUp(self):
        self.fixture = Fixture()

    def rejection(self):
        return 403, {}, b'{"error":"host not allowed"}'

    def upgrade(self):
        return 101, {"upgrade": ["websocket"], "connection": ["Upgrade"],
                     "sec-websocket-accept": [WS_ACCEPT]}, b""

    def test_probe_corpus_contains_wire_cases_high_level_clients_would_change(self):
        cases = {case["id"]: case for case in rejected_cases(10001, "/api/ws")}
        self.assertEqual(len(cases), 18)
        for name in ("missing_host_http11", "missing_host_http10", "absolute_authority_without_host_http10"):
            case = cases[name]
            raw = request_bytes(case["path"], case["headers"], version=case["version"], websocket=True)
            self.assertNotIn(b"Host:", raw)
        self.assertEqual(request_bytes("/api/ws", cases["duplicate_host"]["headers"]).count(b"Host:"), 2)
        self.assertEqual(cases["missing_host_http10"]["version"], "HTTP/1.0")
        self.assertTrue(cases["conflicting_absolute_authority"]["path"].startswith("http://unrelated.example/"))

    def test_denial_rejects_success_redirect_upgrade_cors_and_planted_content_leaks(self):
        assert_rejected(self.rejection(), self.fixture)
        assert_rejected((400, {}, b""), self.fixture)
        bad = [(status, {}, b"") for status in (101, 200, 302, 404, 500)]
        bad.extend([(403, {"sec-websocket-accept": [WS_ACCEPT]}, b'{"error":"denied"}'),
                    (403, {"access-control-allow-origin": ["*"]}, b'{"error":"denied"}'),
                    (403, {}, b'{"error":"denied","requests":[]}')])
        for marker in (SECRET, TEXT, TOOL_RESULT, self.fixture.session):
            bad.append((403, {}, json.dumps({"error": marker}).encode()))
            bad.append((400, {"x-private": [marker]}, b""))
        for response in bad:
            with self.subTest(response=response), self.assertRaises(JourneyError):
                assert_rejected(response, self.fixture)

    def test_accepted_websocket_requires_exact_accept_and_upgrade_headers(self):
        assert_upgrade(self.upgrade())
        for field, changed in (("sec-websocket-accept", ["wrong"]), ("upgrade", ["http2"]),
                               ("connection", ["close"]), ("sec-websocket-accept", [WS_ACCEPT, WS_ACCEPT])):
            status, headers, body = self.upgrade()
            headers[field] = changed
            with self.subTest(field=field), self.assertRaises(JourneyError):
                assert_upgrade((status, headers, body))

    def test_http_observes_all_rejections_and_legitimate_capture_before_and_after(self):
        successful = (200, {}, json.dumps({"has_body": True, "text": self.fixture.body.decode()}).encode())
        responses = [successful] * 4 + [self.rejection()] * 18 + [successful]
        with patch("journey_security.exchange", side_effect=responses) as transport:
            result = raw_http_boundary(10001, 7, self.fixture)
        self.assertEqual(transport.call_count, 23)
        self.assertEqual(len(result["rejected"]), 18)
        self.assertTrue(result["legitimate_capture_read_after_rejections"])
        self.assertEqual(result["browser_execution"], "NOT_RUN")
        self.assertNotIn(TOOL_RESULT, json.dumps(result))
        self.assertTrue(all(call.args[1].startswith(b"GET ") for call in transport.call_args_list))

    def test_websocket_observes_all_rejections_and_leaves_frame_evidence_not_run(self):
        responses = [self.upgrade()] * 4 + [self.rejection()] * 18 + [self.upgrade()]
        with patch("journey_security.exchange", side_effect=responses) as transport:
            result = raw_websocket_boundary(10002, self.fixture)
        self.assertEqual(transport.call_count, 23)
        self.assertTrue(result["legitimate_upgrade_after_rejections"])
        self.assertEqual(result["websocket_message_capture"], "NOT_RUN")
        self.assertTrue(all(b"Sec-WebSocket-Key: " in call.args[1] for call in transport.call_args_list))

    def test_failure_does_not_claim_partial_or_full_pass(self):
        from journey import check
        result = new_result({})
        success = (200, {}, json.dumps({"has_body": True, "text": self.fixture.body.decode()}).encode())
        with patch("journey_security.exchange", side_effect=[success] * 5):
            with self.assertRaisesRegex(JourneyError, "NOT_REJECTED"):
                check(result, "boundary", lambda: raw_http_boundary(10001, 7, self.fixture))
        self.assertEqual(result["state"], "FAILED")
        self.assertFalse(result["qualified"])
        self.assertEqual(result["checks"][0]["state"], "FAILED")
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in result["full_cases"]))


if __name__ == "__main__":
    unittest.main()
