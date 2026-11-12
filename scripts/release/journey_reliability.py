"""Opt-in installed-archive reliability workload; partial AC20 evidence only.

Public entry points always enforce a real duration of at least 900 seconds. This deliberately uses a
separate fresh database and foreground process: the restart/maintenance journey's
single capture is never enlarged or modified. Nothing runs at import time.
"""
from contextlib import ExitStack
import hashlib
import json
import math
import os
from pathlib import Path
import time

from common import sha256
from journey import check, create_config, new_result, verify_binding
from journey_fixture import TOOL_NAME
from journey_http import validate_detail
from journey_persistent import (graceful_stop, migration_ledger, owned_cluster,
                                validate_stored_response, verified_migration_inventory)
from journey_process import (JourneyError, owned_child, require, reserved_ports,
                             runtime_environment, wait_owned_ready)
from journey_reliability_fixture import (BATCH_TIMEOUT, MAX_BODY, MAX_SESSIONS,
                                         reliability_fixture, run_batch)
from journey_reliability_isolation import FEATURES, disable_features, verify_features, verify_service_prerequisites
from journey_reliability_observations import observe_workload
from journey_reliability_profiles import (NORMAL_PROFILE, SYNTHETIC_PROFILE, reliability_options,
                                          require_normal_default_isolation)
from journey_wire import MAX_BODY as MAX_API_BYTES, exchange
from recovery_database import EMPTY_SQL

MIN_DURATION = 900
MAX_DURATION = 7200
BATCH_INTERVAL = 10
MAX_REQUESTS = 25000
MAX_OVERRUN = 120
ISOLATION_OVERRIDE_NAMES = (
    "persist_enabled", "proxy_listen_addr", "inspect_listen_addr", "spa_bind", "upstream_url", "database_url",
    "db_fallback_enabled", "db_resync_enabled", "composition_enabled", "max_requests", "max_age",
    "max_request_body_bytes", "max_response_body_bytes", "max_in_flight", "max_store_bytes", "log_level", "log_max_files",
    "analysis.enabled", "compress.enabled", "reminders.enabled",
    "weasel_intercept.enabled", "recall_injector.enabled", "harvester.enabled", "lesson_worker.enabled",
    "lesson_relations.enabled", "session_judge.enabled", "steerer.enabled", "steerer.openrouter_base_url",
    "steerer.refocus.enabled", "steerer.correction.enabled", "steerer.judge.enabled",
)
CAPTURE_TIMEOUT = 30

LIMITS = [
    "Synthetic paced archive workload in a newly owned disposable PostgreSQL 19 database only.",
    "Real elapsed monotonic time of at least 900 seconds with one to 32 concurrent synthetic sessions.",
    "Concurrency means distinct overlapping requests at the synthetic upstream barrier in every completed batch; sessions are idle between paced batches.",
    "Without explicit resource budgets only harness bounds are reported. Opt-in observations cover the owned Linux daemon, one named broadcast queue, MCP and SPA HTTP; browser execution and other queues remain NOT_RUN.",
    "The synthetic profile uses isolation overrides. The direct host helper rejects normal_defaults; use the separately configured engine-verified isolated runner. Actual normal-default evidence remains NOT_RUN until executed.",
    "No browser responsiveness, external provider, real client, fault injection, interrupted request or full AC20/AC21 qualification.",
    "A private HOME and empty runtime PATH are not an OS sandbox. Candidate executables must already be trusted.",
    "Disabled embedding at its compiled URL and an absent provider key are checked service prerequisites, not all-transport network isolation; the embedding endpoint is not fixture-owned.",
    "All AC01-AC27 remain required NOT_RUN. Passing partial subchecks never qualifies a case or release.",
]
COUNTS_SQL = """SELECT json_build_object(
 'requests',count(*),'complete',count(*) FILTER (WHERE st.code='complete'),
 'sessions',count(DISTINCT r.session_id),'orphaned',count(*) FILTER (WHERE s.id IS NULL),
 'tool_uses',(SELECT count(*) FROM tools.uses))::text
 FROM requests.requests r LEFT JOIN requests.statuses st ON st.id=r.status_id
 LEFT JOIN sessions.sessions s ON s.id=r.session_id"""
FINAL_SQL = f"""SELECT coalesce(json_agg(rows ORDER BY id),'[]'::json)::text FROM
 (SELECT r.id,s.session_uuid::text AS session_id,st.code AS status,
 tu.wire_tool_use_id AS tool_use_id,tu.tool_name
 FROM requests.requests r LEFT JOIN requests.statuses st ON st.id=r.status_id
 LEFT JOIN sessions.sessions s ON s.id=r.session_id
 LEFT JOIN tools.uses tu ON tu.request_id=r.id ORDER BY r.id LIMIT {MAX_REQUESTS + 1}) rows"""


def reliability_policy(duration_seconds=None, concurrent_sessions=None):
    minimum = MIN_DURATION
    sessions = 1 if concurrent_sessions is None else concurrent_sessions
    duration = minimum if duration_seconds is None else duration_seconds
    require(type(duration) in (int, float) and minimum <= duration <= MAX_DURATION
            and math.isfinite(duration), "RELIABILITY_DURATION_POLICY")
    require(type(sessions) is int and 1 <= sessions <= MAX_SESSIONS,
            "RELIABILITY_CONCURRENCY_POLICY")
    max_batches = math.ceil(duration / BATCH_INTERVAL) + 1
    require(max_batches * sessions <= MAX_REQUESTS, "RELIABILITY_REQUEST_LIMIT")
    return {"duration_seconds": duration, "minimum_duration_seconds": minimum,
            "concurrent_sessions": sessions, "interval_seconds": BATCH_INTERVAL,
            "max_batches": max_batches, "max_requests": max_batches * sessions}


class Budget:
    """Absolute monotonic workload deadline; wall clock never proves duration."""
    def __init__(self, seconds):
        self.start = time.monotonic()
        self.last = self.start
        self.deadline = self.start + seconds

    def now(self):
        current = time.monotonic()
        require(math.isfinite(current) and current >= self.last, "RELIABILITY_MONOTONIC_CLOCK")
        self.last = current
        require(current < self.deadline, "RELIABILITY_WORKLOAD_DEADLINE")
        return current

    def remaining(self):
        return self.deadline - self.now()


def api_json(port, path, budget, *, allow_missing=False):
    require(isinstance(path, str) and path.startswith("/api/") and all(32 < ord(char) < 127 for char in path),
            "RELIABILITY_API_PATH")
    request = (f"GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").encode("ascii")
    status, _, raw = exchange(port, request, timeout=min(5, budget.remaining()))
    if allow_missing and status == 404:
        budget.now()
        return None
    require(status == 200 and len(raw) <= MAX_API_BYTES, "RELIABILITY_API_RESPONSE")
    try:
        value = json.loads(raw)
    except (ValueError, UnicodeError) as error:
        raise JourneyError("RELIABILITY_API_JSON") from error
    budget.now()
    return value


def owned_database(cluster, database):
    require(isinstance(database, str) and database in cluster.created, "RELIABILITY_DATABASE_NOT_OWNED")


def database_counts(cluster, database, expected_requests, expected_sessions):
    owned_database(cluster, database)
    try:
        value = json.loads(cluster.sql(database, "BEGIN READ ONLY; " + COUNTS_SQL + "; COMMIT;"))
    except (ValueError, UnicodeError) as error:
        raise JourneyError("RELIABILITY_COUNTS_JSON") from error
    require(isinstance(value, dict) and set(value) == {"requests", "complete", "sessions", "orphaned", "tool_uses"}
            and all(type(item) is int and item >= 0 for item in value.values()), "RELIABILITY_COUNTS_SHAPE")
    require(value == {"requests": expected_requests, "complete": expected_requests,
                      "sessions": expected_sessions, "orphaned": 0, "tool_uses": expected_requests}, "RELIABILITY_CAPTURE_COUNTS")
    return value


def validate_response(body, fixture):
    try:
        value = json.loads(body)
    except (ValueError, UnicodeError) as error:
        raise JourneyError("RELIABILITY_RESPONSE_JSON") from error
    require(isinstance(value, dict) and value.get("id") == fixture.message_id
            and isinstance(value.get("content"), list) and len(value["content"]) == 2
            and isinstance(value["content"][1], dict)
            and value["content"][1].get("id") == fixture.tool_use_id, "RELIABILITY_RESPONSE_IDENTITY")
    # Reuse complete payload validation after checking the per-request IDs. The
    # canonical single-capture IDs cannot be reused in a multi-session database:
    # tools.uses deliberately deduplicates provider wire IDs globally.
    value["id"] = "msg_archive_fixture"
    value["content"][1]["id"] = "tool_archive_response"
    validate_stored_response(json.dumps(value).encode())


def capture_batch(inspect_port, fixtures, completed_batches, seen, budget, *, response_validator=validate_response):
    """No retry/re-send of traffic: poll only read observations after each batch."""
    observations = []
    capture_budget = Budget(min(budget.remaining(), CAPTURE_TIMEOUT))
    deadline = capture_budget.deadline
    budget = capture_budget
    for fixture in fixtures:
        while True:
            page = api_json(inspect_port, f"/api/sessions/{fixture.session}/requests?offset={completed_batches}&limit=2&order=asc", budget, allow_missing=True)
            if page is None:
                # Newly written sessions may not yet be visible to persistence.
                time.sleep(min(0.05, budget.remaining()))
                continue
            require(isinstance(page, dict) and set(page) == {"session_id", "requests", "total_requests", "offset", "limit"},
                    "RELIABILITY_SESSION_PAGE")
            require(page["session_id"] == fixture.session and type(page["offset"]) is int
                    and page["offset"] == completed_batches and type(page["limit"]) is int and page["limit"] == 2
                    and type(page["total_requests"]) is int and 0 <= page["total_requests"] <= completed_batches + 1
                    and isinstance(page["requests"], list) and len(page["requests"]) <= 1,
                    "RELIABILITY_SESSION_ACCOUNTING")
            if page["total_requests"] == completed_batches + 1 and page["requests"]:
                row = page["requests"][0]
                require(isinstance(row, dict), "RELIABILITY_CAPTURE_ROW")
                if row.get("status") == "Complete":
                    break
                require(row.get("status") in ("Pending", "Streaming"), "RELIABILITY_CAPTURE_TERMINAL_ERROR")
            require(budget.now() < deadline, "RELIABILITY_CAPTURE_COMPLETION_TIMEOUT")
            time.sleep(min(0.05, max(0, deadline - budget.now())))
        request_id = row.get("id")
        require(type(request_id) is int and request_id > 0 and request_id not in seen
                and row.get("session_id") == fixture.session, "RELIABILITY_CAPTURE_IDENTITY")
        detail = api_json(inspect_port, f"/api/requests/{request_id}", budget)
        require(isinstance(detail, dict), "RELIABILITY_CAPTURE_DETAIL")
        validate_detail(detail, fixture, request_id)
        observation = {"request_id": request_id, "session_id": fixture.session, "tool_use_id": fixture.tool_use_id}
        for kind in ("request", "response"):
            body = api_json(inspect_port, f"/api/requests/{request_id}/body/{kind}", budget)
            require(isinstance(body, dict) and body.get("has_body") is True and isinstance(body.get("text"), str),
                    "RELIABILITY_CAPTURE_BODY")
            raw = body["text"].encode()
            require(0 < len(raw) <= MAX_BODY and type(body.get("byte_size")) is int
                    and body["byte_size"] == len(raw), "RELIABILITY_CAPTURE_BODY_BYTES")
            if kind == "request":
                require(raw == fixture.body and body.get("reconstructed") is False, "RELIABILITY_CAPTURE_REQUEST_CHANGED")
            else:
                response_validator(raw, fixture)
            observation[kind + "_body_sha256"] = hashlib.sha256(raw).hexdigest()
        seen[request_id] = fixture.session
        observations.append(observation)
    return observations


def final_accounting(cluster, database, expected, expected_tools):
    owned_database(cluster, database)
    rows = json.loads(cluster.sql(database, "BEGIN READ ONLY; " + FINAL_SQL + "; COMMIT;"))
    require(isinstance(rows, list) and len(rows) == len(expected) <= MAX_REQUESTS, "RELIABILITY_FINAL_COUNT")
    actual, actual_tools = {}, {}
    for row in rows:
        require(isinstance(row, dict) and set(row) == {"id", "session_id", "status", "tool_use_id", "tool_name"}
                and type(row["id"]) is int and row["id"] > 0 and row["id"] not in actual
                and isinstance(row["session_id"], str) and row["status"] == "complete"
                and isinstance(row["tool_use_id"], str) and row["tool_name"] == TOOL_NAME, "RELIABILITY_FINAL_ROW")
        actual[row["id"]] = row["session_id"]
        actual_tools[row["id"]] = row["tool_use_id"]
    require(actual == expected, "RELIABILITY_FINAL_CAPTURE_SET")
    require(actual_tools == expected_tools and len(set(actual_tools.values())) == len(expected),
            "RELIABILITY_FINAL_TOOL_SET")
    return {"exact_durable_ids_and_sessions": True, "requests": len(actual),
            "exact_durable_tool_wire_ids": True, "tool_uses": len(actual_tools),
            "capture_set_sha256": hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest()}


def workload(child, config, ports, manifest, cluster, database, cohort, policy, report, *, observations=None):
    """Internal seam for offline short-clock contracts, not a qualification API.

    Only run_reliability_journey supplies runtime policies, always through the
    minimum-duration validator. Tests of this seam use sub-second fake clocks and
    must not be retained as candidate execution or duration evidence.
    """
    budget = Budget(policy["duration_seconds"] + MAX_OVERRUN)
    generation = None
    def owned_ready():
        nonlocal generation
        health = wait_owned_ready(child, config, ports[1], manifest["source_commit"],
                                  timeout=min(10, budget.remaining()), persistent=True)
        observed = health["instance"]["boot_generation"]
        require(generation is None or generation == observed, "RELIABILITY_GENERATION_CHANGED")
        generation = observed
        verify_service_prerequisites(health)
        flags = verify_features(ports[1], budget)
        report["effective_feature_flags"] = flags["effective_feature_flags"]
        report["feature_observations"] += 1
        require(health.get("persistence", {}).get("capture_stalled") is False,
                "RELIABILITY_CAPTURE_STALLED_OR_UNOBSERVED")
        budget.now()
        report["health_observations"] += 1
    seen, seen_tools = {}, {}
    digest = hashlib.sha256()
    owned_ready()
    database_counts(cluster, database, 0, 0)
    started = budget.now()
    while True:
        batch = report["completed_batches"]
        require(batch < policy["max_batches"] and len(seen) + len(cohort.sessions) <= policy["max_requests"],
                "RELIABILITY_REQUEST_LIMIT")
        owned_ready()
        fixtures = cohort.prepare(batch)
        report["attempted_requests"] += len(fixtures)
        observation = (run_batch(ports[0], cohort, fixtures,
                                observe_active=lambda deadline: observations.active(len(fixtures), deadline=deadline))
                       if observations is not None else run_batch(ports[0], cohort, fixtures))
        require(observation["completed"] == len(fixtures)
                and observation["concurrent_sessions"] == len(fixtures), "RELIABILITY_BATCH_ACCOUNTING")
        report["completed_client_requests"] += observation["completed"]
        report["peak_concurrent_sessions"] = max(report["peak_concurrent_sessions"], observation["concurrent_sessions"])
        report["minimum_batch_concurrency"] = (observation["concurrent_sessions"] if batch == 0 else
                                                min(report["minimum_batch_concurrency"], observation["concurrent_sessions"]))
        report["synthetic_request_bytes"] += observation["request_bytes"]
        report["synthetic_response_bytes"] += observation["response_bytes"]
        for captured in capture_batch(ports[1], fixtures, batch, seen, budget):
            seen_tools[captured["request_id"]] = captured["tool_use_id"]
            digest.update(json.dumps(captured, sort_keys=True, separators=(",", ":")).encode() + b"\n")
        database_counts(cluster, database, len(seen), len(cohort.sessions))
        if observations is not None:
            observations.drain()
        report["durably_accounted_requests"] = len(seen)
        report["completed_batches"] += 1
        owned_ready()
        elapsed = budget.now() - started
        report["duration_seconds"] = elapsed
        if elapsed >= policy["duration_seconds"]:
            break
        # Pace from the completed batch, not a catch-up schedule. Slow work never
        # causes queued batches or a burst trying to make up missed iterations.
        time.sleep(min(policy["interval_seconds"], policy["duration_seconds"] - elapsed))
    report.update(final_accounting(cluster, database, seen, seen_tools))
    owned_ready()
    elapsed = budget.now() - started
    require(elapsed >= policy["duration_seconds"] and report["attempted_requests"] == len(seen)
            and report["completed_client_requests"] == len(seen), "RELIABILITY_INCOMPLETE_WORKLOAD")
    report.update(duration_seconds=elapsed, accounting_sha256=digest.hexdigest(),
                  duration_policy_met=elapsed >= policy["minimum_duration_seconds"],
                  concurrency_policy_met=report["minimum_batch_concurrency"] >= 1,
                  workload_state="PASSED")
    return dict(report)


def isolated_config(config, upstream_port, policy):
    # This is our just-created private file, never an operator configuration.
    # Relocate supported optional service endpoints. The loader resets embedding
    # to its compiled disabled configuration, so a TOML override has no effect.
    # Observed disabled-service prerequisites are checked separately; these
    # settings do not establish all-transport network isolation.
    local = f"http://127.0.0.1:{upstream_port}"
    fields = {"db_fallback_enabled": False, "db_resync_enabled": False,
              "composition_enabled": False, "max_requests": policy["max_requests"],
              "max_age": "24h", "max_request_body_bytes": MAX_BODY, "max_response_body_bytes": MAX_BODY,
              "max_in_flight": policy["concurrent_sessions"], "max_store_bytes": 256 * 1024 * 1024,
              "log_level": "error", "log_max_files": 1}
    lines = [key + " = " + json.dumps(value) for key, value in fields.items()]
    for name in ("analysis", "compress", "reminders", "weasel_intercept", "recall_injector",
                 "harvester", "lesson_worker", "lesson_relations", "session_judge", "steerer",
                 "steerer.refocus", "steerer.correction", "steerer.judge"):
        lines.extend((f"[{name}]", "enabled = false"))
        if name == "steerer":
            lines.append("openrouter_base_url = " + json.dumps(local))
    descriptor = os.open(config, os.O_WRONLY | os.O_APPEND | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "w") as output:
        output.write("\n" + "\n".join(lines) + "\n")
        output.flush()
        os.fsync(output.fileno())


def run_bound(paths, manifest, root, result, cluster, inventory, policy):
    database = cluster.create(seeded=False)
    owned_database(cluster, database)
    require(cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0",
            "RELIABILITY_DATABASE_NOT_EMPTY")
    fixture_stack = ExitStack()
    try:
        cohort, upstream_port = fixture_stack.enter_context(reliability_fixture(policy["concurrent_sessions"]))
        ports, reservations = fixture_stack.enter_context(reserved_ports())
        config, home, cwd = create_config(root, ports, upstream_port, database_url=cluster.config_url(database))
        isolated_config(config, upstream_port, policy)
        protected = {path: sha256(path) for path in (config, home / ".env", cwd / ".env")}
        current_paths, binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
        require(current_paths == paths and binding == result["binding"], "RELIABILITY_BINDING_CHANGED")
        require(verified_migration_inventory(paths, manifest) == inventory, "RELIABILITY_INVENTORY_CHANGED")
        for reservation in reservations:
            reservation.close()
        generations = []
        before = None
        for phase in ("isolation", "workload"):
            child_stack = ExitStack()
            try:
                child = child_stack.enter_context(owned_child([str(paths["mike"]), "--config", str(config), "serve"],
                                                             cwd, runtime_environment(home)))
                check(result, "archive_reliability_" + phase + "_owned_readiness", lambda: readiness(
                    child, config, ports, manifest, generations))
                if phase == "isolation":
                    before = check(result, "archive_reliability_initial_ledger", lambda: migration_ledger(cluster, database, inventory))
                    check(result, "archive_reliability_supported_feature_isolation",
                          lambda: disable_features(ports[1], Budget(180)))
                else:
                    check(result, "archive_reliability_effective_isolation_after_restart",
                          lambda: verify_features(ports[1], Budget(10)))
                    def measured_workload():
                        with observe_workload(child, paths, manifest, ports, home, cwd, result["reliability"]) as observations:
                            return workload(child, config, ports, manifest, cluster, database, cohort, policy,
                                            result["reliability"], observations=observations)
                    check(result, "archive_reliability_paced_workload", measured_workload)
                    def unchanged():
                        require(migration_ledger(cluster, database, inventory) == before, "RELIABILITY_LEDGER_CHANGED")
                        require(all(sha256(path) == digest for path, digest in protected.items()), "RELIABILITY_PRIVATE_FILES_CHANGED")
                        new_paths, new_binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
                        require(new_paths == paths and new_binding == binding, "RELIABILITY_BINDING_CHANGED")
                        return {"ledger_unchanged": True, "private_files_unchanged": True, "exact_installed_hashes": True}
                    check(result, "archive_reliability_unchanged_source", unchanged)
                check(result, "archive_reliability_" + phase + "_owned_stop", lambda: graceful_stop(child))
            finally:
                check(result, "archive_reliability_" + phase + "_process_cleanup",
                      lambda: cleanup(child_stack, "owned_process_group_only"))
    finally:
        check(result, "archive_reliability_fixture_cleanup", lambda: cleanup(fixture_stack, "owned_loopback_fixture_only"))
    return {"independent_owned_database": True, "maintenance_capture_untouched": True}


def readiness(child, config, ports, manifest, generations):
    health = wait_owned_ready(child, config, ports[1], manifest["source_commit"], timeout=240, persistent=True)
    services = verify_service_prerequisites(health)
    generation = health["instance"]["boot_generation"]
    require(generation not in generations, "RELIABILITY_BOOT_GENERATION_REUSED")
    generations.append(generation)
    return {"owned_pid_and_generation": True, "persistent_enabled_and_reachable": True,
            **services,
            "distinct_owned_generations": len(generations)}


def cleanup(stack, name):
    stack.close()
    return {name: True, "bounded_cleanup": True}


def new_reliability_report(policy, *, profile=SYNTHETIC_PROFILE, resource_budgets=None):
    options = reliability_options(profile, resource_budgets)
    report = {"workload_state": "NOT_RUN", "profile": "synthetic_isolation_overrides",
            "override_names": list(ISOLATION_OVERRIDE_NAMES), "requested_disabled_feature_keys": list(FEATURES),
            "effective_feature_flags": None, "feature_observations": 0,
            "feature_isolation_scope": "verified_after_supported_api_setup_and_owned_restart_before_measured_traffic",
            "setup_workers_may_start_against_empty_database": True, "requested_duration_seconds": policy["duration_seconds"],
            "minimum_duration_seconds": policy["minimum_duration_seconds"], "duration_seconds": 0,
            "duration_clock": "time.monotonic", "requested_concurrent_sessions": policy["concurrent_sessions"],
            "peak_concurrent_sessions": 0, "minimum_batch_concurrency": 0,
            "duration_policy_met": False, "concurrency_policy_met": False,
            "attempted_requests": 0, "completed_client_requests": 0, "durably_accounted_requests": 0,
            "completed_batches": 0, "health_observations": 0, "synthetic_request_bytes": 0, "synthetic_response_bytes": 0, "failed_run_count": 0,
            "limits": {"maximum_requests": policy["max_requests"], "maximum_pending_batches": 1,
                       "maximum_client_workers": policy["concurrent_sessions"], "maximum_upstream_workers": MAX_SESSIONS,
                       "maximum_body_bytes": MAX_BODY, "maximum_api_response_bytes": MAX_API_BYTES,
                       "maximum_request_traffic_bytes": MAX_BODY * policy["max_requests"],
                       "maximum_response_traffic_bytes": MAX_BODY * policy["max_requests"],
                       "capture_accounting_retained_as_hash_only": True,
                       "interbatch_delay_seconds": policy["interval_seconds"], "final_delay_may_be_shorter": True,
                       "batch_timeout_seconds": BATCH_TIMEOUT, "workload_overrun_budget_seconds": MAX_OVERRUN},
            "unmeasured": {name: "NOT_RUN" for name in ("normal_default_reliability", "os_rss", "os_file_descriptors", "product_queue_depth",
                                                       "browser_responsiveness", "mcp_responsiveness", "spa_http_responsiveness", "fault_injection", "external_provider")}}
    report.update(options)
    if profile == NORMAL_PROFILE:
        report.update(override_names=[], requested_disabled_feature_keys=[],
                      feature_isolation_scope="NOT_RUN_before_normal_default_isolation_admission",
                      setup_workers_may_start_against_empty_database=False)
    return report


def run_reliability_journey(binary_dir, manifest, sandbox, manifest_sha256, *,
                            duration_seconds=None, concurrent_sessions=None, profile=SYNTHETIC_PROFILE, resource_budgets=None):
    """Explicit optional helper; caller owns sandbox. No short runtime override."""
    result = new_result(None)
    result.update(runner="archive-reliability-helper", limits=list(LIMITS))
    cluster = None
    try:
        policy = check(result, "archive_reliability_duration_policy", lambda: reliability_policy(
            duration_seconds, concurrent_sessions))
        options = check(result, "archive_reliability_profile_policy", lambda: reliability_options(profile, resource_budgets))
        result["reliability"] = new_reliability_report(policy, **options)
        if resource_budgets is not None:
            require(resource_budgets["active_streams_peak"] >= policy["concurrent_sessions"],
                    "RELIABILITY_ACTIVE_STREAMS_BUDGET_BELOW_WORKLOAD")
        if profile == NORMAL_PROFILE:
            check(result, "archive_reliability_normal_default_isolation",
                  lambda: require_normal_default_isolation(result["reliability"]))
        paths, result["binding"] = verify_binding(binary_dir, manifest, manifest_sha256)
        check(result, "archive_reliability_hash_binding", lambda: {"exact_installed_hashes": True})
        inventory = verified_migration_inventory(paths, manifest)
        cluster = owned_cluster()
        check(result, "archive_reliability_owned_fixture", lambda: {"postgres_major": 19,
              "owned_private_pgdata": True, "password_free_loopback_admin": True, "tcp_only": True})
        try:
            check(result, "archive_reliability_bounded_run", lambda: run_bound(
                paths, manifest, Path(sandbox) / "reliability", result, cluster, inventory, policy))
        finally:
            def database_cleanup():
                cluster.cleanup()
                return {"only_databases_created_by_this_run": True, "cluster_left_running": True}
            check(result, "archive_reliability_database_cleanup", database_cleanup)
    except Exception as error:
        result.update(state="FAILED", failure_type=type(error).__name__)
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
        if "reliability" in result:
            if result["reliability"].get("normal_default_contract") is None:
                result["reliability"]["workload_state"] = "FAILED"
            result["reliability"]["failed_run_count"] = 1
    return result
