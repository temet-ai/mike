"""Offline socket-free fault-helper contracts, never installed-candidate evidence."""
from contextlib import contextmanager
import copy
import io
import json
from pathlib import Path
import socket
import tempfile
import unittest
from unittest.mock import Mock, patch

import journey_faults as faults
import journey_faults_fixture as fixture
import journey_reliability as reliability
from journey import new_result
from journey_fixture import TEXT
from journey_process import JourneyError


class Offline(unittest.TestCase):
    def setUp(self):
        for name in ("socket.socket", "socket.create_connection", "subprocess.Popen", "subprocess.run"):
            self.enterContext(patch(name, side_effect=AssertionError("Offline contract attempted runtime access")))


class ServicePrerequisiteTests(Offline):
    def test_fault_readiness_uses_shared_compiled_disabled_service_checks(self):
        self.assertIs(faults.readiness, reliability.readiness)
        health = {"instance": {"boot_generation": "fault-owned"},
                  "embedding": {"url": "http://127.0.0.1:7997", "reachable": False,
                                "reachable_state": "not_configured"},
                  "llm": {"key_configured": False}}
        with patch.object(reliability, "wait_owned_ready", return_value=health):
            observation = faults.readiness(object(), Path("/offline/config"), [10001, 10002, 10003],
                                          {"source_commit": "a" * 40}, [])
            self.assertTrue(observation["embedding_disabled"])
            self.assertFalse(observation["embedding_endpoint_owned"])
            self.assertFalse(observation["all_transport_isolation_verified"])
            health["llm"]["key_configured"] = True
            with self.assertRaisesRegex(JourneyError, "SERVICE_PREREQUISITES"):
                faults.readiness(object(), Path("/offline/config"), [10001, 10002, 10003],
                                 {"source_commit": "a" * 40}, [])


class FixtureTests(Offline):
    def test_unique_sessions_messages_tools_and_bounded_bodies(self):
        items = fixture.Faults().fixtures
        self.assertEqual(len({x.session for x in items}), 7)
        self.assertEqual(len({x.message_id for x in items}), 7)
        self.assertEqual(len({x.tool_use_id for x in items}), 7)
        self.assertEqual(len({x.body for x in items}), 7)
        self.assertTrue(all(len(x.body) < fixture.MAX_BODY for x in items))
        for item in items:
            self.assertEqual(json.loads(item.body)["stream"], item.mode not in ("429", "500"))
            self.assertIn(item.message_id.encode(), b"".join(item.events))

    def test_unexpected_or_retried_body_rejected(self):
        server = fixture.Faults()
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_UPSTREAM_BODY"):
            server.claim(b"foreign")
        item = server.fixtures[1]
        self.assertIs(server.claim(item.body), item)
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_UPSTREAM_RETRY"):
            server.claim(item.body)
        self.assertEqual(server.attempts["429"], 2)

    def test_buffered_error_exact_status_body_and_headers(self):
        for item in fixture.Faults().fixtures[1:4:2]:
            headers = {key: [value] for key, value in item.headers.items()}
            result = fixture.validate_wire(int(item.mode), headers, item.error_body, item)
            self.assertEqual(result["status"], int(item.mode))
            for status, hdrs, body in ((200, headers, item.error_body),
                                        (int(item.mode), {}, item.error_body),
                                        (int(item.mode), headers, item.error_body + b"!")):
                with self.assertRaises(JourneyError):
                    fixture.validate_wire(status, hdrs, body, item)

    def test_request_transport_is_loopback_and_unique_identity(self):
        item = fixture.Faults().fixtures[0]
        wire = fixture.request_bytes(1234, item)
        self.assertIn(b"Host: 127.0.0.1:1234\r\n", wire)
        self.assertIn(item.session.encode(), wire)
        self.assertTrue(wire.endswith(item.body))
        for port in (0, 65536, True, "1234"):
            with self.assertRaises(JourneyError):
                fixture.request_bytes(port, item)

    def test_peer_eof_or_reset_is_required_without_release(self):
        for values in ([socket.timeout(), b""], [ConnectionResetError()]):
            server = fixture.Faults()
            connection = Mock()
            connection.recv.side_effect = values
            with patch.object(fixture.time, "monotonic", return_value=1):
                server.wait_peer_close(connection, 2)
            self.assertTrue(server.peer_closed.is_set())
            connection.sendall.assert_not_called()

    def test_watchdog_or_cleanup_cannot_count_as_peer_cancellation(self):
        server = fixture.Faults()
        with patch.object(fixture.time, "monotonic", return_value=2), self.assertRaises(JourneyError):
            server.wait_peer_close(Mock(), 2)
        self.assertFalse(server.peer_closed.is_set())
        server.cancelled.set()
        with patch.object(fixture.time, "monotonic", return_value=1), self.assertRaises(JourneyError):
            server.wait_peer_close(Mock(), 2)
        self.assertFalse(server.peer_closed.is_set())

    def test_unexpected_peer_bytes_fail(self):
        server = fixture.Faults()
        connection = Mock()
        connection.recv.return_value = b"x"
        with patch.object(fixture.time, "monotonic", return_value=1), self.assertRaisesRegex(JourneyError, "UNEXPECTED_UPSTREAM_BYTES"):
            server.wait_peer_close(connection, 2)

    def test_request_has_no_harness_retry(self):
        server = fixture.Faults()
        with patch.object(fixture, "exchange", side_effect=JourneyError("FAULT_TEST")) as exchange:
            with self.assertRaises(JourneyError):
                fixture.run_request(1234, server.fixtures[1], server)
        self.assertEqual(exchange.call_count, 1)

    def test_cancel_reads_text_then_closes_owned_socket(self):
        server = fixture.Faults()
        item = server.fixtures[5]
        connection = Mock()
        connection.__enter__ = Mock(return_value=connection)
        connection.__exit__ = Mock(return_value=False)
        first = b"".join(item.events[:3])
        wire = Mock()
        wire.until.side_effect = [b"head", f"{len(first):x}".encode()]
        wire.exact.side_effect = [first, b"\r\n"]
        item.first_sent.set()
        connection.shutdown.side_effect = lambda *_: server.peer_closed.set()
        with patch.object(fixture.socket, "create_connection", return_value=connection), patch.object(fixture, "Wire", return_value=wire), patch.object(fixture, "response_head", return_value=(200, {
            **{key: [value] for key, value in item.headers.items()}, "transfer-encoding": ["chunked"]})):
            evidence = fixture.cancel_stream(1234, item, server)
        self.assertTrue(evidence["upstream_stalled_until_peer_close"])
        connection.shutdown.assert_called_once_with(socket.SHUT_RDWR)
        self.assertFalse(item.release.is_set())
        self.assertFalse(item.completed.is_set())

    def test_fixture_cleanup_only_owned_connections_after_failure(self):
        server, worker = Mock(), Mock()
        server.server_port = 1234
        worker.is_alive.return_value = False
        with patch.object(fixture, "BoundedServer", return_value=server), patch.object(fixture.threading, "Thread", return_value=worker):
            with self.assertRaisesRegex(RuntimeError, "planned"):
                with fixture.fault_fixture():
                    raise RuntimeError("planned")
        server.shutdown.assert_called_once()
        server.server_close.assert_called_once()
        server.close_owned_connections.assert_called_once()
        worker.join.assert_called_once()

    def test_thread_constructor_failure_still_closes_listener(self):
        server = Mock()
        with patch.object(fixture, "BoundedServer", return_value=server), patch.object(fixture.threading, "Thread", side_effect=RuntimeError("constructor failed")):
            with self.assertRaisesRegex(RuntimeError, "constructor failed"):
                with fixture.fault_fixture():
                    self.fail("Constructor failure must prevent fixture use")
        server.shutdown.assert_not_called()
        server.server_close.assert_called_once()
        server.close_owned_connections.assert_called_once()


class CaptureTests(Offline):
    def test_buffered_error_capture_is_complete_with_error_http_status(self):
        item = fixture.Faults().fixtures[1]
        replies = [
            {"session_id": item.session, "requests": [{"id": 1, "status": "Complete"}], "total_requests": 1},
            {"id": 1, "session_id": item.session, "status": "Complete", "response_status": 429,
             "is_streaming": False, "completed_at": "now"},
            {"has_body": True, "reconstructed": False, "text": item.body.decode()},
            {"has_body": True, "text": item.error_body.decode(), "byte_size": len(item.error_body)}]
        with patch.object(faults, "api_json", side_effect=replies):
            observed = faults.capture(1234, item)
        self.assertEqual(observed["terminal_state"], "Complete")
        self.assertEqual(observed["response_body_sha256"], faults.digest(item.error_body))

    def test_cancel_cannot_be_reported_complete(self):
        item = fixture.Faults().fixtures[5]
        with patch.object(faults, "api_json", return_value={"session_id": item.session, "total_requests": 1,
                "requests": [{"id": 1, "status": "Complete"}]}), self.assertRaisesRegex(JourneyError, "WRONG_TERMINAL"):
            faults.capture(1234, item)

    def test_cancel_does_not_invent_a_complete_response_body(self):
        item = fixture.Faults().fixtures[5]
        replies = [{"session_id": item.session, "total_requests": 1, "requests": [{"id": 1, "status": "Error"}]},
                   {"id": 1, "session_id": item.session, "status": {"Error": "stream closed without terminal event"}, "is_streaming": True, "response_status": 200},
                   {"has_body": True, "reconstructed": False, "text": item.body.decode()}]
        with patch.object(faults, "api_json", side_effect=replies) as api:
            observed = faults.capture(1234, item)
        self.assertEqual(api.call_count, 3)
        self.assertNotIn("response_body_sha256", observed)

    def test_error_api_shapes_and_unrelated_errors_fail_closed(self):
        for message in faults.ABORT_MESSAGES:
            self.assertFalse(faults.cancelled_summary("Error: " + message))
            self.assertTrue(faults.cancelled_detail({"Error": message}))
        self.assertTrue(faults.cancelled_summary("Error"))
        self.assertFalse(faults.cancelled_detail("Error"))
        for value in ("Complete", "Error: unrelated", {"Error": "unrelated"}, None, {"Error": "stream aborted", "extra": 1}):
            self.assertFalse(faults.cancelled_summary(value))
            self.assertFalse(faults.cancelled_detail(value))

    def durable_fixture(self):
        server = fixture.Faults()
        items = [server.fixtures[1], server.fixtures[5]]
        captures = [{"request_id": n, "session_id": item.session, "mode": item.mode,
                     "terminal_state": "Error" if item.mode == "cancel" else "Complete",
                     "response_body_sha256": faults.digest(getattr(item, "error_body", b""))}
                    for n, item in enumerate(items, 1)]
        rows = [{"id": 1, "session_id": items[0].session, "status": "complete", "terminal": True,
                 "response_count": 1, "response_status": 429, "response_hex": items[0].error_body.hex(),
                 "blocks": [], "abort_errors": 0},
                {"id": 2, "session_id": items[1].session, "status": "error", "terminal": True,
                 "response_count": 0, "response_status": None, "response_hex": None,
                 "blocks": [{"kind": "text", "text": TEXT, "seq": 0}], "abort_errors": 1}]
        return items, captures, rows

    def test_durable_error_status_and_partial_block_are_independent_of_response_snapshot(self):
        items, captures, rows = self.durable_fixture()
        cluster = Mock(created=["owned"])
        cluster.sql.return_value = json.dumps(rows)
        result = faults.durable_snapshot(cluster, "owned", items, captures)
        self.assertTrue(result["cancelled_partial_text_and_error_durable"])
        self.assertNotIn(TEXT, json.dumps(result))
        self.assertNotIn("response_hex", result)
        self.assertIn("BEGIN READ ONLY", cluster.sql.call_args.args[1])

    def test_durable_missing_partial_wrong_status_duplicates_or_response_fail(self):
        items, captures, base = self.durable_fixture()
        changes = [(1, "blocks", []), (1, "status", "complete"), (1, "abort_errors", 0),
                   (0, "response_status", 200), (0, "response_hex", "00"), (1, "id", 1)]
        for index, key, value in changes:
            rows = copy.deepcopy(base)
            rows[index][key] = value
            cluster = Mock(created=["owned"])
            cluster.sql.return_value = json.dumps(rows)
            with self.subTest(key=key), self.assertRaises(JourneyError):
                faults.durable_snapshot(cluster, "owned", items, captures)

    def test_no_unowned_database_read(self):
        cluster = Mock(created=[])
        with self.assertRaisesRegex(JourneyError, "DATABASE_NOT_OWNED"):
            faults.durable_snapshot(cluster, "foreign", [], [])
        cluster.sql.assert_not_called()


class OrchestrationTests(Offline):
    def test_bad_binding_stops_before_any_database_or_process(self):
        with patch.object(faults, "verify_binding", side_effect=JourneyError("FAULT_BINDING_TEST")), patch.object(faults, "owned_cluster") as cluster:
            result = faults.run_fault_journey(None, {}, None, "")
        cluster.assert_not_called()
        self.assertEqual(result["failure_code"], "FAULT_BINDING_TEST")
        self.assertFalse(result["qualified"])
        self.assertTrue(all(row["state"] == "NOT_RUN" for row in result["full_cases"]))
        self.assertTrue(all(value == "NOT_RUN" for value in result["faults"]["unmeasured"].values()))

    def test_cleanup_on_bound_failure_and_sanitized_exception(self):
        cluster = Mock()
        with patch.object(faults, "verify_binding", return_value=({}, {})), patch.object(faults, "verified_migration_inventory"), patch.object(faults, "owned_cluster", return_value=cluster), patch.object(faults, "bound_run", side_effect=RuntimeError("postgres://private:secret@host/")):
            result = faults.run_fault_journey(None, {}, "/tmp/unused", "")
        cluster.cleanup.assert_called_once()
        self.assertNotIn("secret", json.dumps(result))
        self.assertEqual(result["state"], "FAILED")

    def test_success_remains_incomplete_and_unqualified(self):
        cluster = Mock()
        with patch.object(faults, "verify_binding", return_value=({}, {})), patch.object(faults, "verified_migration_inventory"), patch.object(faults, "owned_cluster", return_value=cluster), patch.object(faults, "bound_run", return_value={"synthetic": True}):
            result = faults.run_fault_journey(None, {}, "/tmp/unused", "")
        self.assertEqual(result["state"], "INCOMPLETE")
        self.assertFalse(result["qualified"])
        self.assertEqual(len(result["full_cases"]), 27)

    def test_max_in_flight_one_and_disable_restart_verify_before_traffic(self):
        events = []
        synthetic = fixture.Faults()
        synthetic.attempts = {mode: 1 for mode in fixture.MODES}
        @contextmanager
        def context(value):
            yield value
        cluster = Mock(created=["owned"])
        cluster.create.return_value = "owned"
        cluster.config_url.return_value = "postgres://postgres@127.0.0.1:1234/owned"
        cluster.sql.return_value = "0"
        cluster.cleanup.return_value = None
        with tempfile.TemporaryDirectory() as temporary:
            result = new_result({"manifest_sha256": "a" * 64})
            result["faults"] = {}
            paths = {"mike": Path(temporary) / "bin/mike"}
            self.enterContext(patch.object(faults, "fault_fixture", side_effect=lambda: context((synthetic, 12001))))
            self.enterContext(patch.object(faults, "reserved_ports", side_effect=lambda: context(([12002, 12003, 12004], [Mock()]))))
            self.enterContext(patch.object(faults, "verify_binding", return_value=(paths, result["binding"])))
            self.enterContext(patch.object(faults, "verified_migration_inventory", return_value={}))
            self.enterContext(patch.object(faults, "owned_child", side_effect=lambda *args: context(Mock())))
            self.enterContext(patch.object(faults, "readiness", side_effect=lambda *args: events.append("ready")))
            self.enterContext(patch.object(faults, "migration_ledger", return_value={}))
            self.enterContext(patch.object(faults, "disable_features", side_effect=lambda *args: events.append("disable")))
            self.enterContext(patch.object(faults, "verify_features", side_effect=lambda *args: (events.append("verify") or {"effective_feature_flags": {key: False for key in faults.FEATURES}})))
            self.enterContext(patch.object(faults, "graceful_stop", side_effect=lambda *args: events.append("stop")))
            self.enterContext(patch.object(faults, "run_request", side_effect=lambda *args: events.append(args[1].mode)))
            self.enterContext(patch.object(faults, "capture", side_effect=lambda port, item: {"mode": item.mode}))
            self.enterContext(patch.object(faults, "durable_snapshot", return_value={"hash": "same"}))
            faults.bound_run(paths, {"source_commit": "a" * 40}, Path(temporary) / "faults", result, cluster, {})
            import tomllib
            config = tomllib.loads((Path(temporary) / "faults/config.toml").read_text())
            self.assertEqual(config["max_in_flight"], 1)
        self.assertEqual(events[:5], ["ready", "disable", "stop", "ready", "verify"])
        self.assertEqual(events[5:12], list(fixture.MODES))
        self.assertEqual(events[12:], ["stop", "ready", "verify", "stop"])
        self.assertTrue(result["faults"]["durable_restart_continuity"])


if __name__ == "__main__":
    unittest.main()
