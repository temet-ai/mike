"""Offline contracts only: no candidate, socket, DB process or real-duration run.

The workload clock tests intentionally exercise fractions of a second on a fake
clock. They never pretend that 900 real seconds elapsed or qualify AC20.
"""
from contextlib import ExitStack, contextmanager
import hashlib
import json
from pathlib import Path
import tempfile
import threading
import tomllib
import unittest
from unittest.mock import Mock, patch

import journey_reliability as reliability
import journey_reliability_fixture as synthetic
from journey import create_config, new_result
from journey_fixture import MODEL, TEXT, TOOL_NAME, SECRET
from journey_process import JourneyError


class OfflineTests(unittest.TestCase):
    def setUp(self):
        for name in ("socket.socket", "socket.create_connection", "subprocess.Popen", "subprocess.run"):
            self.enterContext(patch(name, side_effect=AssertionError("Offline contract attempted runtime access")))


class PolicyTests(OfflineTests):
    def test_minimum_and_bounded_upper_limits(self):
        default = reliability.reliability_policy()
        self.assertEqual((default["duration_seconds"], default["concurrent_sessions"]), (900, 1))
        self.assertNotIn("stage", default)
        eight = reliability.reliability_policy(concurrent_sessions=8)
        self.assertEqual((eight["duration_seconds"], eight["concurrent_sessions"]), (900, 8))
        self.assertEqual(eight["max_requests"], 91 * 8)
        maximum = reliability.reliability_policy(7200, 32)
        self.assertLessEqual(maximum["max_requests"], reliability.MAX_REQUESTS)

    def test_short_nonfinite_bool_and_excessive_durations_refused(self):
        for value in (899.999, 0, True, "900", float("nan"), float("inf"), 7201, 10 ** 1000):
            with self.subTest(value=value), self.assertRaisesRegex(JourneyError, "DURATION_POLICY"):
                reliability.reliability_policy(value)

    def test_bad_concurrency_refused(self):
        for count in (0, True, 33, 1.0, "1"):
            with self.subTest(count=count), self.assertRaisesRegex(JourneyError, "CONCURRENCY_POLICY"):
                reliability.reliability_policy(concurrent_sessions=count)

    def test_invalid_policy_refused_before_hash_or_fixture_access(self):
        with patch.object(reliability, "verify_binding") as binding, patch.object(reliability, "owned_cluster") as cluster:
            result = reliability.run_reliability_journey(None, {}, None, "", duration_seconds=0.01)
        binding.assert_not_called()
        cluster.assert_not_called()
        self.assertEqual(result["failure_code"], "RELIABILITY_DURATION_POLICY")
        self.assertFalse(result["qualified"])
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in result["full_cases"]))

    def test_monotonic_regression_and_absolute_deadline_fail(self):
        for clock, code in (([2, 1], "MONOTONIC_CLOCK"), ([1, 3], "WORKLOAD_DEADLINE")):
            with patch.object(reliability.time, "monotonic", side_effect=clock):
                budget = reliability.Budget(1)
                with self.assertRaisesRegex(JourneyError, code):
                    budget.now()

    def test_report_cannot_label_missing_measurements_as_passed(self):
        report = reliability.new_reliability_report(reliability.reliability_policy())
        self.assertEqual(report["workload_state"], "NOT_RUN")
        self.assertTrue(all(value == "NOT_RUN" for value in report["unmeasured"].values()))
        self.assertFalse(report["duration_policy_met"])
        self.assertLess(len(json.dumps(report)), 8192)
        self.assertEqual(report["limits"]["maximum_pending_batches"], 1)
        self.assertEqual(report["profile"], "synthetic_isolation_overrides")
        self.assertEqual(report["unmeasured"]["normal_default_reliability"], "NOT_RUN")
        self.assertIn("database_url", report["override_names"])
        self.assertNotIn("postgres://", json.dumps(report))

    def test_private_config_disables_nonfixture_consumers_and_bounds_work(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "owned"
            config, _, _ = create_config(root, [10001, 10002, 10003], 10004,
                                         database_url="postgres://postgres@127.0.0.1:6432/owned?sslmode=disable")
            reliability.isolated_config(config, 10004, reliability.reliability_policy(concurrent_sessions=8))
            value = tomllib.loads(config.read_text())
            self.assertEqual(config.stat().st_mode & 0o777, 0o600)
            self.assertEqual(value["max_in_flight"], 8)
            self.assertEqual(value["max_requests"], 91 * 8)
            self.assertFalse(value["db_fallback_enabled"])
            self.assertFalse(value["db_resync_enabled"])
            for field in ("steerer", "lesson_worker", "analysis", "session_judge", "harvester"):
                self.assertFalse(value[field]["enabled"])
            self.assertNotIn("embedding", value)
            self.assertNotIn("embedding.enabled", reliability.ISOLATION_OVERRIDE_NAMES)
            self.assertNotIn("embedding.url", reliability.ISOLATION_OVERRIDE_NAMES)
            self.assertEqual(value["steerer"]["openrouter_base_url"], value["upstream_url"])


class FixtureTests(OfflineTests):
    def test_distinct_stable_sessions_and_unique_bodies_across_batches(self):
        cohort = synthetic.Cohort(8)
        first = cohort.prepare(0)
        self.assertEqual(len({item.session for item in first}), 8)
        self.assertEqual(len({item.body for item in first}), 8)
        second = cohort.prepare(1)
        self.assertEqual([item.session for item in first], [item.session for item in second])
        self.assertFalse({item.body for item in first} & {item.body for item in second})
        self.assertTrue(all(len(item.body) < synthetic.MAX_BODY for item in second))
        self.assertEqual(len({item.tool_use_id for item in first + second}), 16)
        self.assertEqual(len({item.message_id for item in first + second}), 16)
        self.assertTrue(all(item.tool_use_id.encode() in b"".join(item.events) for item in first + second))
        self.assertTrue(all(b"tool_archive_prior\"" not in item.body for item in first + second))

    def test_server_barrier_requires_every_distinct_expected_body(self):
        cohort = synthetic.Cohort(8)
        fixtures = cohort.prepare(0)
        for item in fixtures[:-1]:
            self.assertIs(cohort.claim(item.body), item)
            self.assertFalse(cohort.all_active.is_set())
        cohort.claim(fixtures[-1].body)
        self.assertTrue(cohort.all_active.is_set())
        self.assertEqual(cohort.peak_active, 8)
        for _ in fixtures:
            cohort.finish()
        self.assertEqual(cohort.active, 0)

    def test_unknown_duplicate_or_cancelled_request_rejected(self):
        cohort = synthetic.Cohort(1)
        fixture = cohort.prepare(0)[0]
        for body in (b"real data", b""):
            with self.assertRaisesRegex(JourneyError, "UNEXPECTED_UPSTREAM_REQUEST"):
                cohort.claim(body)
        cohort.claim(fixture.body)
        with self.assertRaisesRegex(JourneyError, "UNEXPECTED_UPSTREAM_REQUEST"):
            cohort.claim(fixture.body)
        cohort.finish()
        cohort.cancel()
        self.assertTrue(fixture.release.is_set())
        with self.assertRaisesRegex(JourneyError, "CANCELLED"):
            cohort.prepare(1)

    def test_bad_session_count_or_live_batch_replacement_fails(self):
        for count in (0, 33, True, 8.0):
            with self.subTest(count=count), self.assertRaisesRegex(JourneyError, "SESSION_LIMIT"):
                synthetic.Cohort(count)
        cohort = synthetic.Cohort(1)
        fixture = cohort.prepare(0)[0]
        cohort.claim(fixture.body)
        with self.assertRaisesRegex(JourneyError, "BATCH_OWNERSHIP"):
            cohort.prepare(1)

    def response(self, fixture):
        fixture.first_sent.set()
        response = Mock(status=200)
        response.getheader.side_effect = lambda name, _default=None: {
            "Content-Type": "text/event-stream", "request-id": "req_archive_fixture",
            "anthropic-ratelimit-requests-remaining": "42"}[name]
        response.readline.side_effect = [fixture.events[0]]
        response.read.return_value = b"".join(fixture.events[1:])
        return response

    def test_exact_stream_bytes_headers_and_synthetic_session_sent(self):
        fixture = synthetic.Cohort(1).prepare(0)[0]
        connection = Mock(getresponse=Mock(return_value=self.response(fixture)))
        size = synthetic.stream_one(connection, fixture)
        self.assertEqual(size, sum(map(len, fixture.events)))
        self.assertTrue(fixture.release.is_set())
        self.assertEqual(connection.request.call_args.kwargs["body"], fixture.body)
        self.assertEqual(connection.request.call_args.kwargs["headers"]["x-claude-code-session-id"], fixture.session)
        connection.close.assert_called_once()

    def test_failed_stream_always_releases_and_closes_without_retry(self):
        for change in (lambda response: setattr(response, "status", 502),
                       lambda response: setattr(response.read, "return_value", b"wrong"),
                       lambda response: setattr(response.readline, "side_effect", [b""])):
            fixture = synthetic.Cohort(1).prepare(0)[0]
            response = self.response(fixture)
            change(response)
            connection = Mock(getresponse=Mock(return_value=response))
            with self.assertRaises(JourneyError):
                synthetic.stream_one(connection, fixture)
            connection.close.assert_called_once()
            self.assertEqual(connection.request.call_count, 1)
            self.assertTrue(fixture.release.is_set())


class WorkerBoundsTests(OfflineTests):
    def test_socket_reference_survives_http_response_adoption_for_cancellation(self):
        connection = synthetic.OwnedConnection(10001)
        owned_socket = Mock()
        def connect(instance):
            instance.sock = owned_socket
        with patch("http.client.HTTPConnection.connect", connect):
            connection.connect()
        connection.sock = None  # HTTPResponse owns a close-delimited connection.
        connection.cancel()
        owned_socket.shutdown.assert_called_once()
        owned_socket.close.assert_called_once()
        with patch("http.client.HTTPConnection.connect") as connect:
            with self.assertRaisesRegex(JourneyError, "CLIENT_CANCELLED"):
                connection.connect()
        connect.assert_not_called()

    def test_server_refuses_more_than_owned_handler_limit_without_thread_start(self):
        server = object.__new__(synthetic.BoundedServer)
        server.slots = threading.BoundedSemaphore(1)
        server.slots.acquire()
        server.shutdown_request = Mock()
        connection = Mock()
        with patch("http.server.ThreadingHTTPServer.process_request") as spawn:
            server.process_request(connection, ("127.0.0.1", 10001))
        spawn.assert_not_called()
        server.shutdown_request.assert_called_once_with(connection)

    def test_fixture_thread_start_failure_still_closes_owned_listener(self):
        server = Mock()
        worker = Mock(start=Mock(side_effect=RuntimeError("offline start failure")))
        with patch.object(synthetic, "BoundedServer", return_value=server), \
             patch.object(synthetic.threading, "Thread", return_value=worker):
            with self.assertRaisesRegex(RuntimeError, "offline start failure"):
                with synthetic.reliability_fixture(1):
                    self.fail("fixture unexpectedly started")
        server.server_close.assert_called_once()
        server.close_owned_connections.assert_called_once()
        server.shutdown.assert_not_called()
        worker.join.assert_not_called()

    def test_client_thread_start_failure_closes_socket_without_joining_unstarted_thread(self):
        cohort = synthetic.Cohort(1)
        fixtures = cohort.prepare(0)
        connection = Mock()
        worker = Mock(start=Mock(side_effect=RuntimeError("offline start failure")))
        with patch.object(synthetic, "OwnedConnection", return_value=connection), \
             patch.object(synthetic.threading, "Thread", return_value=worker):
            with self.assertRaisesRegex(RuntimeError, "offline start failure"):
                synthetic.run_batch(10001, cohort, fixtures)
        connection.cancel.assert_called_once()
        worker.join.assert_not_called()

    def test_batch_threads_sockets_and_pending_requests_are_bounded_and_closed(self):
        cohort = synthetic.Cohort(8)
        fixtures = cohort.prepare(0)
        for fixture in fixtures:
            cohort.claim(fixture.body)
        class InlineThread:
            def __init__(self, target, args, daemon):
                self.target, self.args = target, args
            def start(self):
                self.target(*self.args)
            def join(self, timeout):
                pass
            def is_alive(self):
                return False
        def completed(_connection, fixture):
            fixture.completed.set()
            cohort.finish()
            return 64
        connections = [Mock() for _ in fixtures]
        with patch.object(synthetic.threading, "Thread", InlineThread), \
             patch.object(synthetic, "OwnedConnection", side_effect=connections) as create, \
             patch.object(synthetic, "stream_one", side_effect=completed):
            report = synthetic.run_batch(10001, cohort, fixtures)
        self.assertEqual(create.call_count, 8)
        self.assertEqual(report["concurrent_sessions"], 8)
        self.assertEqual(report["completed"], 8)
        self.assertEqual(report["response_bytes"], 512)
        for connection in connections:
            connection.cancel.assert_called_once()

    def test_batch_timeout_cancels_only_owned_connections_and_joins_workers(self):
        cohort = synthetic.Cohort(1)
        fixtures = cohort.prepare(0)
        class WaitingThread:
            def __init__(self, **_args):
                pass
            def start(self):
                pass
            def join(self, timeout):
                pass
            def is_alive(self):
                return not cohort.cancelled.is_set()
        connection = Mock()
        with patch.object(synthetic.threading, "Thread", WaitingThread), \
             patch.object(synthetic, "OwnedConnection", return_value=connection), \
             patch.object(synthetic, "stream_one") as stream:
            with self.assertRaisesRegex(JourneyError, "CLIENT_BATCH_TIMEOUT"):
                synthetic.run_batch(10001, cohort, fixtures)
        stream.assert_not_called()
        connection.cancel.assert_called_once()
        self.assertTrue(cohort.cancelled.is_set())
        self.assertTrue(fixtures[0].release.is_set())


    def test_active_observation_holds_barrier_and_failure_releases_owned_workers(self):
        cohort = synthetic.Cohort(1)
        fixtures = cohort.prepare(0)
        cohort.claim(fixtures[0].body)
        class WaitingThread:
            def __init__(self, **_args):
                pass
            def start(self):
                pass
            def join(self, timeout):
                pass
            def is_alive(self):
                return not cohort.cancelled.is_set()
        def observe(deadline):
            self.assertIsInstance(deadline, (int, float))
            self.assertGreater(deadline, synthetic.time.monotonic())
            self.assertLessEqual(deadline - synthetic.time.monotonic(), synthetic.BATCH_TIMEOUT)
            self.assertFalse(cohort.observation_release.is_set())
            self.assertTrue(cohort.all_active.is_set())
            self.assertEqual(cohort.active, 1)
            raise JourneyError("RELIABILITY_OBSERVATION_FAULT")
        connection = Mock()
        with patch.object(synthetic.threading, "Thread", WaitingThread), \
             patch.object(synthetic, "OwnedConnection", return_value=connection):
            with self.assertRaisesRegex(JourneyError, "OBSERVATION_FAULT"):
                synthetic.run_batch(10001, cohort, fixtures, observe_active=observe)
        self.assertTrue(cohort.observation_release.is_set())
        self.assertTrue(cohort.cancelled.is_set())
        self.assertTrue(fixtures[0].release.is_set())
        connection.cancel.assert_called_once()

    def test_barrier_at_batch_deadline_never_starts_active_observation(self):
        cohort = synthetic.Cohort(1)
        fixtures = cohort.prepare(0)
        cohort.claim(fixtures[0].body)
        clock = ShortClock()
        class WaitingThread:
            def __init__(self, **_args):
                pass
            def start(self):
                pass
            def join(self, timeout):
                pass
            def is_alive(self):
                return not cohort.cancelled.is_set()
        def late_barrier(_timeout):
            clock.value = synthetic.BATCH_TIMEOUT
            return True
        connection, observe = Mock(), Mock()
        with patch.object(synthetic.time, "monotonic", side_effect=clock.now), \
             patch.object(cohort.all_active, "wait", side_effect=late_barrier), \
             patch.object(synthetic.threading, "Thread", WaitingThread), \
             patch.object(synthetic, "OwnedConnection", return_value=connection), \
             self.assertRaisesRegex(JourneyError, "ACTIVE_OBSERVATION_TIMEOUT"):
            synthetic.run_batch(10001, cohort, fixtures, observe_active=observe)
        observe.assert_not_called()
        connection.cancel.assert_called_once()
        self.assertTrue(cohort.cancelled.is_set())
        self.assertTrue(cohort.observation_release.is_set())


class AccountingTests(OfflineTests):
    def stored_response(self, fixture):
        return json.dumps({"id": fixture.message_id, "type": "message", "role": "assistant",
            "model": MODEL, "stop_reason": "tool_use", "stop_sequence": None,
            "content": [{"type": "text", "text": TEXT},
                        {"type": "tool_use", "id": fixture.tool_use_id, "name": TOOL_NAME,
                         "input": {"echo": "synthetic"}}], "usage": {"input_tokens": 25, "output_tokens": 12}}).encode()

    def capture_values(self, fixture, request_id=7, completed_batches=0):
        page = {"session_id": fixture.session, "requests": [{"id": request_id, "session_id": fixture.session,
                 "status": "Complete"}], "total_requests": completed_batches + 1, "offset": completed_batches, "limit": 2}
        detail = {"id": request_id, "session_id": fixture.session, "status": "Complete", "is_streaming": True,
                  "response_status": 200, "completed_at": "synthetic", "model": MODEL, "stop_reason": "tool_use",
                  "token_usage": {"input_tokens": 25, "output_tokens": 12},
                  "tool_calls": [{"name": TOOL_NAME, "complete": True, "input_json": '{"echo":"synthetic"}'}]}
        bodies = [{"has_body": True, "text": raw.decode(), "byte_size": len(raw), "reconstructed": False}
                  for raw in (fixture.body, self.stored_response(fixture))]
        return [page, detail, *bodies]

    def test_each_durable_capture_checks_exact_body_and_one_new_row(self):
        fixture = synthetic.Cohort(1).prepare(0)[0]
        seen = {}
        with patch.object(reliability, "api_json", side_effect=self.capture_values(fixture)) as api:
            observation = reliability.capture_batch(10002, [fixture], 0, seen, reliability.Budget(10))
        self.assertEqual(seen, {7: fixture.session})
        self.assertEqual(observation[0]["request_body_sha256"], hashlib.sha256(fixture.body).hexdigest())
        self.assertIn("?offset=0&limit=2&order=asc", api.call_args_list[0].args[1])
        self.assertEqual(api.call_count, 4)

    def test_duplicate_foreign_lost_reconstructed_or_changed_capture_fails(self):
        changes = [lambda values: values[0].update(total_requests=2),
                   lambda values: values[0]["requests"].append(values[0]["requests"][0]),
                   lambda values: values[0]["requests"][0].update(session_id="foreign"),
                   lambda values: values[0]["requests"][0].update(status="Error: private"),
                   lambda values: values[1].update(response_status=503),
                   lambda values: values[2].update(reconstructed=True),
                   lambda values: values[2].update(byte_size=True),
                   lambda values: values[3].update(text="not json")]
        for change in changes:
            fixture = synthetic.Cohort(1).prepare(0)[0]
            values = self.capture_values(fixture)
            change(values)
            with self.subTest(change=change), patch.object(reliability, "api_json", side_effect=values):
                with self.assertRaises(JourneyError):
                    reliability.capture_batch(10002, [fixture], 0, {}, reliability.Budget(10))
        fixture = synthetic.Cohort(1).prepare(0)[0]
        with patch.object(reliability, "api_json", side_effect=self.capture_values(fixture)):
            with self.assertRaisesRegex(JourneyError, "CAPTURE_IDENTITY"):
                reliability.capture_batch(10002, [fixture], 0, {7: fixture.session}, reliability.Budget(10))

    def test_stored_message_and_tool_ids_must_match_this_request(self):
        first, second = synthetic.Cohort(2).prepare(0)
        reliability.validate_response(self.stored_response(first), first)
        with self.assertRaisesRegex(JourneyError, "RESPONSE_IDENTITY"):
            reliability.validate_response(self.stored_response(first), second)
        wrong = json.loads(self.stored_response(first))
        wrong["content"][1]["id"] = second.tool_use_id
        with self.assertRaisesRegex(JourneyError, "RESPONSE_IDENTITY"):
            reliability.validate_response(json.dumps(wrong).encode(), first)

    def test_missing_session_is_polled_without_resending_traffic(self):
        fixture = synthetic.Cohort(1).prepare(0)[0]
        clock = ShortClock()
        with patch.object(reliability.time, "monotonic", side_effect=clock.now), \
             patch.object(reliability.time, "sleep", side_effect=clock.sleep), \
             patch.object(reliability, "api_json", side_effect=[None, *self.capture_values(fixture)]) as api:
            seen = {}
            reliability.capture_batch(10002, [fixture], 0, seen, reliability.Budget(1))
        self.assertEqual(seen, {7: fixture.session})
        self.assertEqual(clock.sleeps, [0.05])
        self.assertTrue(api.call_args_list[0].kwargs["allow_missing"])

    def test_missing_api_404_is_only_allowed_for_explicit_poll(self):
        with patch.object(reliability, "exchange", return_value=(404, {}, b"")):
            self.assertIsNone(reliability.api_json(10002, "/api/sessions/synthetic/requests", reliability.Budget(1), allow_missing=True))
            with self.assertRaisesRegex(JourneyError, "API_RESPONSE"):
                reliability.api_json(10002, "/api/requests/1", reliability.Budget(1))

    def test_aggregate_counts_are_strict_owned_and_read_only(self):
        expected = {"requests": 8, "complete": 8, "sessions": 8, "orphaned": 0, "tool_uses": 8}
        cluster = Mock(created=["owned"], sql=Mock(return_value=json.dumps(expected)))
        self.assertEqual(reliability.database_counts(cluster, "owned", 8, 8), expected)
        statement = cluster.sql.call_args.args[1]
        self.assertTrue(statement.startswith("BEGIN READ ONLY;"))
        self.assertIn("st.code='complete'", statement)
        for field, value in (("requests", 9), ("complete", 7), ("sessions", 9), ("orphaned", 1), ("requests", True), ("tool_uses", 1)):
            cluster.sql.return_value = json.dumps(expected | {field: value})
            with self.subTest(field=field, value=value), self.assertRaises(JourneyError):
                reliability.database_counts(cluster, "owned", 8, 8)
        cluster.sql.reset_mock()
        with self.assertRaisesRegex(JourneyError, "DATABASE_NOT_OWNED"):
            reliability.database_counts(cluster, "real", 0, 0)
        cluster.sql.assert_not_called()

    def test_final_set_rejects_deleted_replaced_duplicate_or_foreign_ids(self):
        expected = {7: "synthetic-a", 8: "synthetic-b"}
        expected_tools = {7: "unique-tool-a", 8: "unique-tool-b"}
        valid = [{"id": key, "session_id": value, "status": "complete", "tool_use_id": expected_tools[key],
                  "tool_name": TOOL_NAME} for key, value in expected.items()]
        cluster = Mock(created=["owned"], sql=Mock(return_value=json.dumps(valid)))
        observation = reliability.final_accounting(cluster, "owned", expected, expected_tools)
        self.assertTrue(observation["exact_durable_ids_and_sessions"])
        self.assertNotIn("synthetic-a", json.dumps(observation))
        for rows in (valid[:1], valid + [valid[0]], [valid[0], valid[0]],
                     [valid[0], dict(valid[1], id=9)], [valid[0], dict(valid[1], session_id="foreign")],
                     [valid[0], dict(valid[1], status="pending")], [valid[0], dict(valid[1], tool_use_id="unique-tool-a")],
                     [valid[0], dict(valid[1], tool_use_id=None)]):
            cluster.sql.return_value = json.dumps(rows)
            with self.subTest(rows=rows), self.assertRaises(JourneyError):
                reliability.final_accounting(cluster, "owned", expected, expected_tools)

    def test_api_transport_has_absolute_deadline_fixed_loopback_no_redirects(self):
        with patch.object(reliability, "exchange", return_value=(200, {}, b'{"synthetic":true}')) as exchange:
            self.assertEqual(reliability.api_json(10002, "/api/health", reliability.Budget(10)), {"synthetic": True})
        self.assertEqual(exchange.call_args.args[0], 10002)
        self.assertIn(b"Host: 127.0.0.1:10002\r\n", exchange.call_args.args[1])
        self.assertLessEqual(exchange.call_args.kwargs["timeout"], 5)
        for response in ((302, {}, b""), (200, {}, b"not json")):
            with patch.object(reliability, "exchange", return_value=response), self.assertRaises(JourneyError):
                reliability.api_json(10002, "/api/health", reliability.Budget(10))
        with patch.object(reliability, "exchange") as exchange, self.assertRaisesRegex(JourneyError, "API_PATH"):
            reliability.api_json(10002, "/api/health\r\nInjected: yes", reliability.Budget(10))
        exchange.assert_not_called()


class ShortClock:
    def __init__(self):
        self.value = 0
        self.sleeps = []
    def now(self):
        return self.value
    def sleep(self, seconds):
        self.sleeps.append(seconds)
        self.value += seconds


class WorkloadTests(OfflineTests):
    @contextmanager
    def observations(self, *, fault=None, generation_change=False):
        clock = ShortClock()
        policy = reliability.reliability_policy(concurrent_sessions=8) | {"duration_seconds": 0.16, "interval_seconds": 0.05,
                                                       "max_batches": 5, "max_requests": 40}
        report = reliability.new_reliability_report(policy)
        cohort = synthetic.Cohort(8)
        health = {"instance": {"boot_generation": "owned"}, "persistence": {"capture_stalled": False},
                  "embedding": {"url": "http://127.0.0.1:7997", "reachable_state": "not_configured", "reachable": False},
                  "llm": {"key_configured": False}}
        def batch(_port, _cohort, fixtures):
            clock.value += 0.01
            if fault:
                raise fault
            return {"completed": 8, "concurrent_sessions": 8, "request_bytes": 800, "response_bytes": 1600}
        def captures(_port, fixtures, completed, seen, _budget):
            for index, fixture in enumerate(fixtures):
                request_id = completed * 8 + index + 1
                seen[request_id] = fixture.session
            return [{"request_id": key, "session_id": seen[key], "tool_use_id": "synthetic-tool-" + str(key), "request_body_sha256": "a" * 64,
                     "response_body_sha256": "b" * 64} for key in sorted(seen)[-8:]]
        with ExitStack() as stack:
            stack.enter_context(patch.object(reliability.time, "monotonic", side_effect=clock.now))
            stack.enter_context(patch.object(reliability.time, "sleep", side_effect=clock.sleep))
            ready = stack.enter_context(patch.object(reliability, "wait_owned_ready", return_value=health))
            if generation_change:
                ready.side_effect = [health, health, {"instance": {"boot_generation": "foreign"},
                                                     "persistence": {"capture_stalled": False}}]
            stack.enter_context(patch.object(reliability, "verify_features", return_value={"effective_feature_flags": {key: False for key in reliability.FEATURES}}))
            counts = stack.enter_context(patch.object(reliability, "database_counts", return_value={}))
            run = stack.enter_context(patch.object(reliability, "run_batch", side_effect=batch))
            stack.enter_context(patch.object(reliability, "capture_batch", side_effect=captures))
            final = stack.enter_context(patch.object(reliability, "final_accounting", return_value={"exact_durable_ids_and_sessions": True}))
            yield clock, policy, report, cohort, ready, counts, run, final

    def invoke(self, values):
        _, policy, report, cohort, *_ = values
        return reliability.workload(Mock(), Path("/offline/config"), [10001, 10002, 10003],
                                    {"source_commit": "a" * 40}, Mock(), "owned", cohort, policy, report)

    def test_subsecond_fake_schedule_is_paced_accounted_and_not_real_duration_evidence(self):
        with self.observations() as values:
            observation = self.invoke(values)
            clock, policy, report, _, ready, counts, run, final = values
        self.assertEqual(run.call_count, 4)
        self.assertEqual(report["attempted_requests"], 32)
        self.assertEqual(report["completed_client_requests"], 32)
        self.assertEqual(report["durably_accounted_requests"], 32)
        self.assertEqual(len(clock.sleeps), 3)
        self.assertTrue(all(0 < delay <= 0.05 for delay in clock.sleeps))
        self.assertGreaterEqual(observation["duration_seconds"], 0.16)
        self.assertLess(observation["duration_seconds"], 1)
        self.assertFalse(observation["duration_policy_met"])
        self.assertEqual(observation["minimum_duration_seconds"], 900)
        self.assertEqual(observation["peak_concurrent_sessions"], 8)
        self.assertTrue(observation["concurrency_policy_met"])
        final.assert_called_once()
        self.assertEqual(len(json.dumps(report)) < 8192, True)
        self.assertTrue(all(value == "NOT_RUN" for value in report["unmeasured"].values()))

    def test_stream_failure_never_retries_or_satisfies_duration(self):
        with self.observations(fault=OSError("private host path " + SECRET)) as values:
            with self.assertRaises(OSError):
                self.invoke(values)
            self.assertEqual(values[6].call_count, 1)
            self.assertEqual(values[2]["attempted_requests"], 8)
            self.assertEqual(values[2]["durably_accounted_requests"], 0)
            self.assertEqual(values[0].sleeps, [])
            values[7].assert_not_called()

    def test_generation_change_immediately_fails_owned_runtime(self):
        with self.observations(generation_change=True) as values:
            with self.assertRaisesRegex(JourneyError, "GENERATION_CHANGED"):
                self.invoke(values)
            values[7].assert_not_called()

    def test_changed_service_prerequisites_fail_before_any_capture_traffic(self):
        for group, field, value in (("embedding", "reachable_state", "ok"),
                                    ("embedding", "url", "http://127.0.0.1:10004"),
                                    ("llm", "key_configured", True)):
            with self.subTest(group=group, field=field), self.observations() as values:
                values[4].return_value[group][field] = value
                with self.assertRaisesRegex(JourneyError, "SERVICE_PREREQUISITES"):
                    self.invoke(values)
                values[6].assert_not_called()
                values[7].assert_not_called()

    def test_request_limit_stops_even_with_frozen_clock(self):
        with self.observations() as values:
            values[1]["max_batches"] = 1
            with self.assertRaisesRegex(JourneyError, "REQUEST_LIMIT"):
                self.invoke(values)
            self.assertEqual(values[6].call_count, 1)
            values[7].assert_not_called()


class LifecycleTests(OfflineTests):
    def test_outer_missing_fixture_is_sanitized_and_unqualified(self):
        with patch.object(reliability, "verify_binding", return_value=({}, {"manifest_sha256": "a" * 64})), \
             patch.object(reliability, "verified_migration_inventory", return_value={}), \
             patch.object(reliability, "owned_cluster", side_effect=JourneyError("PERSISTENT_FIXTURE_REQUIRED")), \
             patch.object(reliability, "run_bound") as run:
            result = reliability.run_reliability_journey(Path("/offline"), {}, Path("/offline"), "a" * 64)
        run.assert_not_called()
        self.assertEqual(result["failure_code"], "PERSISTENT_FIXTURE_REQUIRED")
        self.assertFalse(result["qualified"])
        self.assertEqual(result["reliability"]["workload_state"], "FAILED")
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in result["full_cases"]))

    def test_outer_failure_always_cleans_owned_database_and_suppresses_exception_text(self):
        cluster = Mock()
        with patch.object(reliability, "verify_binding", return_value=({}, {"manifest_sha256": "a" * 64})), \
             patch.object(reliability, "verified_migration_inventory", return_value={}), \
             patch.object(reliability, "owned_cluster", return_value=cluster), \
             patch.object(reliability, "run_bound", side_effect=RuntimeError("private path " + SECRET)):
            result = reliability.run_reliability_journey(Path("/offline"), {}, Path("/offline"), "a" * 64)
        cluster.cleanup.assert_called_once()
        self.assertEqual(result["failure_type"], "RuntimeError")
        self.assertEqual(result["state"], "FAILED")
        self.assertNotIn(SECRET, json.dumps(result))
        self.assertNotIn("private path", json.dumps(result))
        self.assertEqual(result["checks"][-1]["id"], "archive_reliability_database_cleanup")
        self.assertEqual(result["checks"][-1]["state"], "PASSED")

    @contextmanager
    def run_context(self, temporary, *, fail_workload=False, fail_process_cleanup=False, fail_fixture_cleanup=False):
        root = Path(temporary) / "reliability"
        paths = {name: Path(temporary) / "bin" / name for name in ("mike", "mike-mcp")}
        binding = {"manifest_sha256": "a" * 64}
        manifest = {"source_commit": "b" * 40}
        policy = reliability.reliability_policy()
        result = new_result(binding)
        result["reliability"] = reliability.new_reliability_report(policy)
        events = []
        cluster = Mock(created=["owned"], create=Mock(return_value="owned"), sql=Mock(return_value="0"),
                       config_url=Mock(return_value="postgres://postgres@127.0.0.1:6432/owned?sslmode=disable"))
        @contextmanager
        def fixture(_count):
            events.append("fixture-open")
            try:
                yield synthetic.Cohort(1), 10004
            finally:
                events.append("fixture-cleanup")
                if fail_fixture_cleanup:
                    raise JourneyError("RELIABILITY_UPSTREAM_THREAD_CLEANUP")
        @contextmanager
        def ports():
            yield [10001, 10002, 10003], [Mock(), Mock(), Mock()]
        @contextmanager
        def process(args, cwd, environment):
            self.assertEqual(args, [str(paths["mike"]), "--config", str(root / "config.toml"), "serve"])
            self.assertEqual(environment["PATH"], "")
            events.append("process-open")
            try:
                yield Mock()
            finally:
                events.append("process-cleanup")
                if fail_process_cleanup:
                    raise JourneyError("RELIABILITY_PROCESS_CLEANUP_FAILED")
        def run_workload(*_args, **_options):
            events.append("workload")
            if fail_workload:
                raise JourneyError("RELIABILITY_SYNTHETIC_WORKLOAD_FAILED")
            return {"mock_only": True}
        with ExitStack() as stack:
            for name, replacement in (("reliability_fixture", fixture), ("reserved_ports", ports), ("owned_child", process)):
                stack.enter_context(patch.object(reliability, name, replacement))
            stack.enter_context(patch.object(reliability, "verify_binding", return_value=(paths, binding)))
            stack.enter_context(patch.object(reliability, "verified_migration_inventory", return_value={}))
            stack.enter_context(patch.object(reliability, "readiness", return_value={"mock_only": True}))
            stack.enter_context(patch.object(reliability, "migration_ledger", return_value={"mock_only": True}))
            stack.enter_context(patch.object(reliability, "disable_features", return_value={"mock_only": True}))
            stack.enter_context(patch.object(reliability, "verify_features", return_value={"mock_only": True}))
            stack.enter_context(patch.object(reliability, "workload", side_effect=run_workload))
            stop = stack.enter_context(patch.object(reliability, "graceful_stop", side_effect=lambda _child: events.append("stop")))
            yield root, paths, manifest, policy, result, cluster, events, stop

    def test_independent_source_uses_owned_contexts_and_reports_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary, self.run_context(temporary) as values:
            root, paths, manifest, policy, result, cluster, events, stop = values
            observation = reliability.run_bound(paths, manifest, root, result, cluster, {}, policy)
            self.assertTrue(observation["maintenance_capture_untouched"])
            self.assertEqual(events, ["fixture-open", "process-open", "stop", "process-cleanup", "process-open", "workload", "stop", "process-cleanup", "fixture-cleanup"])
            cluster.create.assert_called_once_with(seeded=False)
            self.assertEqual(result["state"], "INCOMPLETE")
            self.assertTrue(all(item["state"] == "NOT_RUN" for item in result["full_cases"]))
            self.assertTrue(all(item["state"] == "PASSED" for item in result["checks"]))
            self.assertFalse(result["qualified"])

    def test_workload_and_cleanup_failures_still_close_outer_owned_fixture(self):
        for options, code in (({"fail_workload": True}, "SYNTHETIC_WORKLOAD_FAILED"),
                              ({"fail_process_cleanup": True}, "PROCESS_CLEANUP_FAILED"),
                              ({"fail_fixture_cleanup": True}, "UPSTREAM_THREAD_CLEANUP")):
            with self.subTest(options=options), tempfile.TemporaryDirectory() as temporary, self.run_context(temporary, **options) as values:
                root, paths, manifest, policy, result, cluster, events, stop = values
                with self.assertRaisesRegex(JourneyError, code):
                    reliability.run_bound(paths, manifest, root, result, cluster, {}, policy)
                self.assertIn("process-cleanup", events)
                self.assertEqual(events[-1], "fixture-cleanup")
                self.assertEqual(result["state"], "FAILED")
                if options.get("fail_workload"):
                    self.assertEqual(stop.call_count, 1)  # Isolation generation stopped first.

    def test_nonempty_fresh_database_fails_before_network_or_candidate(self):
        cluster = Mock(created=["owned"], create=Mock(return_value="owned"), sql=Mock(return_value="1"))
        with patch.object(reliability, "reliability_fixture") as fixture:
            with self.assertRaisesRegex(JourneyError, "DATABASE_NOT_EMPTY"):
                reliability.run_bound({}, {}, Path("/offline"), new_result({}), cluster, {}, reliability.reliability_policy())
        fixture.assert_not_called()


if __name__ == "__main__":
    unittest.main()
