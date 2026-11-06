"""Budgeted live daemon/API/MCP observations, distinct from browser execution."""
from contextlib import contextmanager
import json
import math
import time

from journey_mcp import Wire, content_json
from journey_process import JourneyError, owned_child, require, runtime_environment
from journey_reliability_resources import ResourceSampler
from journey_wire import exchange

METRICS = {"proxy_memory_broadcast_backlog": "broadcast_backlog",
           'proxy_memory_ticker_runs_total{outcome="ok"}': "ticker_runs"}


def remaining(deadline):
    value = deadline - time.monotonic()
    require(value > 0, "RELIABILITY_OBSERVATION_DEADLINE")
    return value


def probe_deadline(seconds, deadline=None):
    bounded = time.monotonic() + seconds
    return bounded if deadline is None else min(bounded, deadline)


def bounded_get(port, path, timeout):
    require(type(port) is int and 0 < port < 65536 and path in ("/api/metrics", "/api/stats", "/"),
            "RELIABILITY_OBSERVATION_TARGET")
    raw = (f"GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").encode()
    status, headers, body = exchange(port, raw, timeout=min(5, timeout))
    require(status == 200, "RELIABILITY_OBSERVATION_HTTP_STATUS")
    return headers, body


def product_snapshot(inspect_port, timeout, *, deadline=None):
    deadline = probe_deadline(timeout, deadline)
    _headers, raw = bounded_get(inspect_port, "/api/metrics", remaining(deadline))
    try:
        text = raw.decode("ascii")
        metrics = {}
        for line in text.splitlines():
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if parts[0] not in METRICS:
                continue
            key = METRICS[parts[0]]
            require(len(parts) == 2 and key not in metrics, "RELIABILITY_METRICS_SCHEMA")
            value = float(parts[1])
            require(math.isfinite(value) and value >= 0 and value.is_integer(), "RELIABILITY_METRICS_SCHEMA")
            metrics[key] = int(value)
        require(set(metrics) == set(METRICS.values()), "RELIABILITY_METRICS_MISSING")
        _headers, raw = bounded_get(inspect_port, "/api/stats", remaining(deadline))
        stats = json.loads(raw)
    except (ValueError, UnicodeError) as error:
        raise JourneyError("RELIABILITY_METRICS_SCHEMA") from error
    require(isinstance(stats, dict) and type(stats.get("active_streams")) is int and stats["active_streams"] >= 0,
            "RELIABILITY_STATS_SCHEMA")
    remaining(deadline)
    return dict(metrics, active_streams=stats["active_streams"])


class Observations:
    def __init__(self, resources, wire, ports, budgets, report):
        self.resources, self.wire = resources, wire
        self.ports, self.budgets, self.report = ports, budgets, report
        self.last_tick = 0
        report["product_observations"] = {"state": "RUNNING", "samples": 0, "active_streams_peak": 0,
            "broadcast_backlog_sampled_peak": 0, "drain_checks": 0,
            "queue_scope": "Only proxy_memory_broadcast_backlog; other internal queues remain NOT_RUN.",
            "concurrency_source": "/api/stats active_streams, held upstream batch barrier",
            "queue_source": "/api/metrics; drain requires a subsequent successful memory ticker sample"}
        report["responsiveness"] = {"state": "RUNNING", "samples": 0, "mcp_peak_seconds": 0,
            "spa_http_peak_seconds": 0, "baseline": None,
            "mcp_scope": "Owned installed companion, diagnostic_memory read over stdio against owned inspect endpoint",
            "ui_scope": "Embedded SPA HTTP only; no browser rendering or interaction",
            "browser_responsiveness": "NOT_RUN"}

    def product(self, timeout=5, *, deadline=None):
        started = time.monotonic()
        deadline = probe_deadline(min(timeout, self.budgets["responsiveness_seconds"]), deadline)
        remaining(deadline)
        self.resources.sample()
        snapshot = product_snapshot(self.ports[1], remaining(deadline), deadline=deadline)
        remaining(deadline)
        require(time.monotonic() - started <= self.budgets["responsiveness_seconds"],
                "RELIABILITY_API_RESPONSIVENESS_BUDGET_EXCEEDED")
        require(snapshot["ticker_runs"] >= self.last_tick, "RELIABILITY_METRIC_TICK_REGRESSION")
        self.last_tick = snapshot["ticker_runs"]
        observation = self.report["product_observations"]
        observation["samples"] += 1
        observation["final"] = snapshot
        observation["active_streams_peak"] = max(observation["active_streams_peak"], snapshot["active_streams"])
        observation["broadcast_backlog_sampled_peak"] = max(observation["broadcast_backlog_sampled_peak"], snapshot["broadcast_backlog"])
        require(snapshot["active_streams"] <= self.budgets["active_streams_peak"], "RELIABILITY_ACTIVE_STREAMS_BUDGET_EXCEEDED")
        require(snapshot["broadcast_backlog"] <= self.budgets["broadcast_backlog_peak"], "RELIABILITY_BROADCAST_BUDGET_EXCEEDED")
        return snapshot

    def responsiveness(self, *, deadline=None):
        started = time.monotonic()
        mcp_deadline = probe_deadline(self.budgets["responsiveness_seconds"], deadline)
        remaining(mcp_deadline)
        result = content_json(self.wire.call("tools/call", {"name": "diagnostic_memory", "arguments": {"top": 1}},
                                           deadline=mcp_deadline))
        mcp_elapsed = time.monotonic() - started
        memory = result.get("memory")
        require(isinstance(memory, dict) and type(memory.get("in_flight_count")) is int
                and memory["in_flight_count"] >= 0, "RELIABILITY_MCP_MEMORY_SCHEMA")
        require(mcp_elapsed <= self.budgets["responsiveness_seconds"], "RELIABILITY_MCP_RESPONSIVENESS_BUDGET_EXCEEDED")
        remaining(mcp_deadline)
        started = time.monotonic()
        spa_deadline = probe_deadline(self.budgets["responsiveness_seconds"], deadline)
        headers, body = bounded_get(self.ports[2], "/", remaining(spa_deadline))
        spa_elapsed = time.monotonic() - started
        require(any("text/html" in value for value in headers.get("content-type", [])) and body and b"/@vite" not in body,
                "RELIABILITY_SPA_HTTP_RESPONSE")
        require(spa_elapsed <= self.budgets["responsiveness_seconds"], "RELIABILITY_SPA_RESPONSIVENESS_BUDGET_EXCEEDED")
        remaining(spa_deadline)
        observation = self.report["responsiveness"]
        observation["samples"] += 1
        sample = {"mcp_seconds": mcp_elapsed, "spa_http_seconds": spa_elapsed}
        if observation["baseline"] is None:
            observation["baseline"] = sample
        observation["final"] = sample
        observation["mcp_peak_seconds"] = max(observation["mcp_peak_seconds"], mcp_elapsed)
        observation["spa_http_peak_seconds"] = max(observation["spa_http_peak_seconds"], spa_elapsed)

    def active(self, sessions, *, deadline=None):
        snapshot = self.product(deadline=deadline)
        require(snapshot["active_streams"] == sessions, "RELIABILITY_PRODUCT_CONCURRENCY_MISMATCH")
        self.responsiveness(deadline=deadline)

    def drain(self):
        start = time.monotonic()
        deadline = start + self.budgets["drain_seconds"]
        previous_tick = None
        while True:
            remaining = deadline - time.monotonic()
            require(remaining > 0, "RELIABILITY_PRODUCT_DRAIN_TIMEOUT")
            snapshot = self.product(remaining, deadline=deadline)
            require(time.monotonic() <= deadline, "RELIABILITY_PRODUCT_DRAIN_TIMEOUT")
            if previous_tick is None:
                previous_tick = snapshot["ticker_runs"]
            elif snapshot["ticker_runs"] > previous_tick and snapshot["active_streams"] == 0 and snapshot["broadcast_backlog"] == 0:
                observation = self.report["product_observations"]
                observation["drain_checks"] += 1
                observation["last_drain_seconds"] = time.monotonic() - start
                return snapshot
            time.sleep(min(0.1, max(0, deadline - time.monotonic())))

    def baseline(self):
        self.product()
        baseline = self.drain()
        self.report["product_observations"]["baseline"] = baseline
        self.responsiveness()
        self.report["baseline_comparison"] = {"kind": "same_owned_process_pretraffic_idle_vs_workload",
            "different_build_control": "NOT_RUN", "normal_defaults_control": "NOT_RUN",
            "external_provider_performance": "NOT_RUN"}

    def finish(self):
        self.drain()
        self.responsiveness()
        self.report["product_observations"].update(state="PASSED", observed_queue_and_concurrency_drained=True)
        self.report["responsiveness"].update(state="PASSED", explicit_budgets_met=True)
        # Keep the broad queue and browser NOT_RUN entries: these observations
        # cover one named queue and an HTTP endpoint, not those entire surfaces.
        self.report["unmeasured"].pop("mcp_responsiveness", None)
        self.report["unmeasured"].pop("spa_http_responsiveness", None)


@contextmanager
def observe_workload(child, paths, manifest, ports, home, cwd, report):
    budgets = report.get("resource_budgets")
    if budgets is None:
        yield None
        return
    with ResourceSampler(child, budgets, report) as resources:
        environment = runtime_environment(home)
        environment["MIKE_INSPECT_URL"] = f"http://127.0.0.1:{ports[1]}"
        with owned_child([str(paths["mike-mcp"]), "serve", "--core"], cwd, environment, protocol=True) as companion:
            wire = Wire(companion)
            try:
                initialized = wire.call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                    "clientInfo": {"name": "mike-archive-reliability", "version": "1"}})
                require(initialized.get("serverInfo", {}).get("name") == "mike-mcp"
                        and initialized["serverInfo"].get("version") == manifest["version"]
                        and initialized.get("protocolVersion") == "2024-11-05"
                        and isinstance(initialized.get("capabilities", {}).get("tools"), dict), "RELIABILITY_MCP_IDENTITY")
                wire.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
                observation = Observations(resources, wire, ports, budgets, report)
                observation.baseline()
                yield observation
                observation.finish()
            except Exception:
                for key in ("product_observations", "responsiveness"):
                    if key in report:
                        report[key]["state"] = "FAILED"
                raise
            finally:
                wire.finish()
