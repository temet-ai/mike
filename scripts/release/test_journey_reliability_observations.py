"""Offline API/MCP timing/drain contracts with fake clocks and no runtime traffic."""
from contextlib import contextmanager
import json
from unittest.mock import Mock, patch
import unittest

import journey_reliability_observations as observations
from journey_process import JourneyError
from test_journey_reliability import OfflineTests, ShortClock
from test_journey_reliability_profiles import BUDGETS


def metrics(backlog=0, ticks=1):
    return (f'# TYPE proxy_memory_broadcast_backlog gauge\nproxy_memory_broadcast_backlog {backlog}\n'
            f'proxy_memory_ticker_runs_total{{outcome="ok"}} {ticks}\n').encode()


def product(active=0, backlog=0, ticks=1):
    return {"active_streams": active, "broadcast_backlog": backlog, "ticker_runs": ticks}


class SnapshotTests(OfflineTests):
    def test_only_reviewed_fixed_loopback_gets_and_live_stats_are_used(self):
        with patch.object(observations.time, "monotonic", return_value=100), \
             patch.object(observations, "exchange", side_effect=[(200, {}, metrics(2, 3)),
                                                               (200, {}, b'{"active_streams":8}')]) as transport:
            self.assertEqual(observations.product_snapshot(10002, 0.25), product(8, 2, 3))
        for call, path in zip(transport.call_args_list, ("metrics", "stats")):
            self.assertEqual(call.args[0], 10002)
            self.assertTrue(call.args[1].startswith(f"GET /api/{path} HTTP/1.1\r\n".encode()))
            self.assertEqual(call.kwargs["timeout"], 0.25)
        with patch.object(observations, "exchange") as transport, self.assertRaisesRegex(JourneyError, "TARGET"):
            observations.bounded_get(10002, "/api/other", 1)
        transport.assert_not_called()

    def test_sequential_gets_share_one_deadline_and_expiry_prevents_second_probe(self):
        clock = ShortClock()
        def get(_port, path, timeout):
            if path == "/api/metrics":
                clock.value += 0.2
                return {}, metrics()
            self.assertAlmostEqual(timeout, 0.05)
            return {}, b'{"active_streams":0}'
        with patch.object(observations.time, "monotonic", side_effect=clock.now), \
             patch.object(observations, "bounded_get", side_effect=get) as get_request:
            self.assertEqual(observations.product_snapshot(10002, 0.25), product())
            self.assertEqual(get_request.call_count, 2)
            get_request.reset_mock()
            with self.assertRaisesRegex(JourneyError, "OBSERVATION_DEADLINE"):
                observations.product_snapshot(10002, 0.1)
            self.assertEqual(get_request.call_count, 1)

    def test_missing_duplicate_fractional_nonfinite_and_negative_metrics_fail(self):
        invalid = [b"", metrics() + metrics(), metrics("nan"), metrics(-1), metrics(0.5), metrics(0, "inf"), b"\xff"]
        for raw in invalid:
            with self.subTest(raw=raw), patch.object(observations, "bounded_get", return_value=({}, raw)), self.assertRaises(JourneyError):
                observations.product_snapshot(10002, 1)
        for active in (None, True, -1, 0.5):
            with self.subTest(active=active), patch.object(observations, "bounded_get", side_effect=[({}, metrics()), ({}, json.dumps({"active_streams": active}).encode())]), self.assertRaisesRegex(JourneyError, "STATS_SCHEMA"):
                observations.product_snapshot(10002, 1)

    def test_http_redirect_or_error_never_follows_or_passes(self):
        for status in (302, 404, 503):
            with self.subTest(status=status), patch.object(observations, "exchange", return_value=(status, {}, b"private")), self.assertRaisesRegex(JourneyError, "HTTP_STATUS"):
                observations.bounded_get(10002, "/api/metrics", 1)


class ObservationTests(OfflineTests):
    def setUp(self):
        super().setUp()
        self.clock = ShortClock()
        self.enterContext(patch.object(observations.time, "monotonic", side_effect=self.clock.now))
        self.enterContext(patch.object(observations.time, "sleep", side_effect=self.clock.sleep))
        self.report = {"unmeasured": {"product_queue_depth": "NOT_RUN", "browser_responsiveness": "NOT_RUN",
                                      "mcp_responsiveness": "NOT_RUN", "spa_http_responsiveness": "NOT_RUN"}}
        self.resources, self.wire = Mock(), Mock()
        self.wire.call.return_value = {"content": [{"type": "text", "text": '{"memory":{"in_flight_count":0}}'}]}
        self.observer = observations.Observations(self.resources, self.wire, [10001, 10002, 10003], BUDGETS, self.report)
        self.http = self.enterContext(patch.object(observations, "bounded_get", return_value=({"content-type": ["text/html; charset=utf-8"]}, b"<html>embedded SPA</html>")))

    def test_product_barrier_count_and_live_resource_sample_are_required(self):
        with patch.object(observations, "product_snapshot", return_value=product(8, 2, 4)):
            self.observer.active(8)
        self.resources.sample.assert_called_once()
        self.assertEqual(self.report["product_observations"]["active_streams_peak"], 8)
        self.assertEqual(self.report["responsiveness"]["samples"], 1)
        self.wire.call.assert_called_with("tools/call", {"name": "diagnostic_memory", "arguments": {"top": 1}},
                                          deadline=BUDGETS["responsiveness_seconds"])
        with patch.object(observations, "product_snapshot", return_value=product(7, 2, 5)), self.assertRaisesRegex(JourneyError, "CONCURRENCY_MISMATCH"):
            self.observer.active(8)

    def test_drain_requires_fresh_ticker_and_zero_queue_and_concurrency(self):
        self.observer.last_tick = 5
        values = [product(0, 0, 5), product(1, 0, 6), product(0, 1, 7), product(0, 0, 8)]
        with patch.object(observations, "product_snapshot", side_effect=values):
            self.assertEqual(self.observer.drain(), values[-1])
        self.assertEqual(len(self.clock.sleeps), 3)
        self.assertEqual(self.report["product_observations"]["drain_checks"], 1)

    def test_stale_zero_gauge_and_undrained_work_time_out_instead_of_passing(self):
        for value in (product(0, 0, 5), product(1, 0, 6), product(0, 1, 6)):
            self.observer.last_tick = 5
            self.observer.budgets = dict(BUDGETS, drain_seconds=0.3)
            with self.subTest(value=value), patch.object(observations, "product_snapshot", return_value=value), self.assertRaisesRegex(JourneyError, "DRAIN_TIMEOUT"):
                self.observer.drain()
        self.assertEqual(self.report["product_observations"]["drain_checks"], 0)

    def test_tick_advanced_during_batch_is_only_drain_entry_baseline(self):
        self.observer.last_tick = 5
        values = [product(0, 0, 6), product(0, 0, 6), product(0, 0, 7)]
        with patch.object(observations, "product_snapshot", side_effect=values) as snapshot:
            self.assertEqual(self.observer.drain(), values[-1])
        self.assertEqual(snapshot.call_count, 3)
        self.assertEqual(len(self.clock.sleeps), 2)

    def test_product_caps_and_counter_reset_fail(self):
        for value, code in ((product(9), "ACTIVE_STREAMS_BUDGET"), (product(backlog=28), "BROADCAST_BUDGET"), (product(ticks=0), "TICK_REGRESSION")):
            self.observer.last_tick = 1
            with self.subTest(code=code), patch.object(observations, "product_snapshot", return_value=value), self.assertRaisesRegex(JourneyError, code):
                self.observer.product()

    def test_responsiveness_reports_http_separately_from_browser_and_keeps_budget(self):
        def mcp(*_args, **_kwargs):
            self.clock.value += 0.2
            return {"content": [{"type": "text", "text": '{"memory":{"in_flight_count":8}}'}]}
        def http(*_args):
            self.clock.value += 0.1
            return {"content-type": ["text/html"]}, b"<html></html>"
        self.wire.call.side_effect = mcp
        self.http.side_effect = http
        self.observer.responsiveness()
        result = self.report["responsiveness"]
        self.assertAlmostEqual(result["mcp_peak_seconds"], 0.2)
        self.assertAlmostEqual(result["spa_http_peak_seconds"], 0.1)
        self.assertEqual(result["browser_responsiveness"], "NOT_RUN")
        self.assertEqual(result["baseline"], result["final"])
        self.observer.budgets = dict(BUDGETS, responsiveness_seconds=0.05)
        with self.assertRaisesRegex(JourneyError, "MCP_RESPONSIVENESS_BUDGET"):
            self.observer.responsiveness()

    def test_active_probes_share_batch_deadline_with_mcp_and_remaining_spa_time(self):
        def snapshot(_port, _timeout, *, deadline):
            self.assertEqual(deadline, 0.5)
            self.clock.value += 0.2
            return product(8)
        def mcp(*_args, deadline):
            self.assertEqual(deadline, 0.5)
            self.clock.value += 0.2
            return {"content": [{"type": "text", "text": '{"memory":{"in_flight_count":8}}'}]}
        self.wire.call.side_effect = mcp
        with patch.object(observations, "product_snapshot", side_effect=snapshot):
            self.observer.active(8, deadline=0.5)
        self.assertAlmostEqual(self.http.call_args.args[2], 0.1)
        self.assertEqual(self.report["responsiveness"]["samples"], 1)

    def test_expired_batch_budget_starts_no_probe_and_mcp_expiry_prevents_spa(self):
        with patch.object(observations, "product_snapshot") as snapshot, \
             self.assertRaisesRegex(JourneyError, "OBSERVATION_DEADLINE"):
            self.observer.active(8, deadline=0)
        snapshot.assert_not_called()
        self.wire.call.assert_not_called()
        self.http.assert_not_called()
        def mcp(*_args, **_kwargs):
            self.clock.value += 0.2
            return {"content": [{"type": "text", "text": '{"memory":{"in_flight_count":8}}'}]}
        self.wire.call.side_effect = mcp
        with self.assertRaisesRegex(JourneyError, "OBSERVATION_DEADLINE"):
            self.observer.responsiveness(deadline=0.1)
        self.http.assert_not_called()
        self.assertEqual(self.report["responsiveness"]["samples"], 0)

    def test_bad_mcp_and_dev_spa_fail(self):
        self.wire.call.return_value = {"content": [{"type": "text", "text": '{"memory":{"in_flight_count":true}}'}]}
        with self.assertRaisesRegex(JourneyError, "MCP_MEMORY_SCHEMA"):
            self.observer.responsiveness()
        self.wire.call.return_value = {"content": [{"type": "text", "text": '{"memory":{"in_flight_count":0}}'}]}
        self.http.return_value = {"content-type": ["text/html"]}, b"<script src='/@vite/client'>"
        with self.assertRaisesRegex(JourneyError, "SPA_HTTP_RESPONSE"):
            self.observer.responsiveness()

    def test_success_preserves_broad_unmeasured_scopes_and_truthful_baseline(self):
        with patch.object(observations, "product_snapshot", side_effect=[product(ticks=tick) for tick in range(1, 6)]):
            self.observer.baseline()
            self.observer.finish()
        self.assertEqual(self.report["unmeasured"], {"product_queue_depth": "NOT_RUN", "browser_responsiveness": "NOT_RUN"})
        self.assertEqual(self.report["baseline_comparison"]["different_build_control"], "NOT_RUN")
        self.assertEqual(self.report["baseline_comparison"]["normal_defaults_control"], "NOT_RUN")
        self.assertEqual(self.report["product_observations"]["state"], "PASSED")
        self.assertEqual(self.report["responsiveness"]["state"], "PASSED")


class LifecycleTests(OfflineTests):
    def test_no_budget_starts_no_observer_process_or_thread(self):
        with patch.object(observations, "ResourceSampler") as sampler, patch.object(observations, "owned_child") as child:
            with observations.observe_workload(None, {}, {}, [], None, None, {}) as result:
                self.assertIsNone(result)
        sampler.assert_not_called(); child.assert_not_called()

    def test_observation_failure_finishes_mcp_and_owned_contexts(self):
        report = {"resource_budgets": BUDGETS}
        wire = Mock()
        wire.call.return_value = {"serverInfo": {"name": "mike-mcp", "version": "1"}, "protocolVersion": "2024-11-05", "capabilities": {"tools": {}}}
        events = []
        @contextmanager
        def resource(*_args):
            try:
                yield Mock()
            finally:
                events.append("resources-cleanup")
        @contextmanager
        def child(*args, **kwargs):
            self.assertEqual(args[0], ["/owned/mike-mcp", "serve", "--core"])
            self.assertEqual(args[2]["MIKE_INSPECT_URL"], "http://127.0.0.1:10002")
            self.assertEqual(args[2]["PATH"], "")
            try:
                yield Mock()
            finally:
                events.append("mcp-cleanup")
        with patch.object(observations, "ResourceSampler", side_effect=resource), \
             patch.object(observations, "owned_child", side_effect=child), patch.object(observations, "Wire", return_value=wire), \
             patch.object(observations.Observations, "baseline"), patch.object(observations.Observations, "finish") as finish:
            with self.assertRaisesRegex(JourneyError, "SYNTHETIC_FAIL"):
                with observations.observe_workload(None, {"mike-mcp": "/owned/mike-mcp"}, {"version": "1"}, [10001, 10002, 10003], "/owned/home", "/owned/cwd", report):
                    raise JourneyError("SYNTHETIC_FAIL")
        finish.assert_not_called(); wire.finish.assert_called_once()
        self.assertEqual(events, ["mcp-cleanup", "resources-cleanup"])
        self.assertEqual(report["responsiveness"]["state"], "FAILED")


if __name__ == "__main__":
    unittest.main()
