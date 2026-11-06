"""Opt-in installed archive upstream/cancellation faults, partial AC21 only.

No DB outage, quota/disk fault, loss-window policy or external provider is tested.
This helper never starts at import time and never qualifies a full case.
"""
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import time

from common import sha256
from journey import check, create_config, new_result, verify_binding
from journey_fixture import TEXT
from journey_http import validate_detail
from journey_persistent import graceful_stop, migration_ledger, owned_cluster, verified_migration_inventory
from journey_process import JourneyError, owned_child, require, reserved_ports, runtime_environment
from journey_reliability import (Budget, api_json, cleanup, isolated_config, owned_database,
                                 readiness, validate_response)
from journey_reliability_isolation import FEATURES, disable_features, verify_features
from journey_faults_fixture import MODES, MAX_BODY, MAX_WORKERS, TIMEOUT, fault_fixture, run_request
from recovery_database import EMPTY_SQL

LIMITS = [
    "Fresh owned disposable PostgreSQL 19 database, exact installed candidate and foreground processes only.",
    "Synthetic isolation overrides; all 32 feature flags disabled through supported API and verified after own restart before traffic.",
    "Initial isolation-setup workers may run against the empty database; this is not an OS network sandbox.",
    "Readiness checks disabled embedding at its compiled URL and an absent provider key; the embedding endpoint is not fixture-owned and these checks do not prove all-transport isolation.",
    "Buffered HTTP 429/500 are completed HTTP exchanges captured as Complete, with their error response_status; only connection errors are retried by the product.",
    "Client cancellation is observed while upstream stays silent; max_in_flight=1 and a following durable capture exercise capacity recovery.",
    "A cancelled stream requires durable Error state and its partial text block; it does not imply a complete assembled response or full original-body retention.",
    "DB outage, quota-limited disk faults, persistence failure/forwarding policy and supported loss windows remain NOT_RUN pending policy decisions.",
    "No normal-default reliability, external provider, browser, real client, process crash or native platform qualification.",
    "All AC01-AC27 remain required NOT_RUN; partial observations never qualify AC21 or a release.",
]

DURABLE_SQL = f"""SELECT coalesce(json_agg(rows ORDER BY id),'[]'::json)::text FROM
 (SELECT r.id,s.session_uuid::text AS session_id,st.code AS status,
 r.completed_at IS NOT NULL AS terminal,
 (SELECT count(*) FROM responses.responses p WHERE p.request_id=r.id) AS response_count,
 (SELECT p.response_status FROM responses.responses p WHERE p.request_id=r.id LIMIT 1) AS response_status,
 (SELECT CASE WHEN octet_length(p.response_body)<={MAX_BODY} THEN encode(p.response_body,'hex') END
  FROM responses.responses p WHERE p.request_id=r.id LIMIT 1) AS response_hex,
 (SELECT coalesce(json_agg(json_build_object('kind',t.code,'text',
    CASE WHEN octet_length(b.content)<={MAX_BODY} THEN b.content END,'seq',b.seq_ord) ORDER BY b.seq_ord),'[]'::json)
  FROM (SELECT * FROM responses.stream_blocks WHERE request_id=r.id ORDER BY seq_ord LIMIT 4) b
  JOIN responses.stream_block_types t ON t.id=b.block_type_id) AS blocks,
 (SELECT count(*) FROM requests.errors e WHERE e.request_id=r.id AND e.message='stream aborted') AS abort_errors
 FROM requests.requests r JOIN sessions.sessions s ON s.id=r.session_id
 JOIN requests.statuses st ON st.id=r.status_id ORDER BY r.id LIMIT {len(MODES)+1}) rows"""


def digest(value):
    return hashlib.sha256(value).hexdigest()


ABORT_MESSAGES = ("stream closed without terminal event", "stream aborted")


def cancelled_summary(status):
    # The session-request endpoint uses DB-backed RequestSummaryView, whose
    # normalize_status_code maps SQL error to this literal (unlike Display).
    return status == "Error"


def cancelled_detail(status):
    return isinstance(status, dict) and set(status) == {"Error"} and status["Error"] in ABORT_MESSAGES


def capture(inspect_port, fixture):
    budget = Budget(30)
    expected = "Error" if fixture.mode == "cancel" else "Complete"
    while True:
        page = api_json(inspect_port, f"/api/sessions/{fixture.session}/requests?offset=0&limit=2&order=asc",
                        budget, allow_missing=True)
        if page is not None:
            require(isinstance(page, dict) and page.get("session_id") == fixture.session
                    and type(page.get("total_requests")) is int and 0 <= page["total_requests"] <= 1
                    and isinstance(page.get("requests"), list) and len(page["requests"]) <= 1,
                    "FAULT_CAPTURE_PAGE")
            if page["requests"]:
                row = page["requests"][0]
                require(isinstance(row, dict), "FAULT_CAPTURE_ROW")
                if (cancelled_summary(row.get("status")) if fixture.mode == "cancel" else row.get("status") == expected):
                    break
                require(row.get("status") in ("Pending", "Streaming"), "FAULT_CAPTURE_WRONG_TERMINAL")
        time.sleep(min(0.05, budget.remaining()))
    identifier = row.get("id")
    require(type(identifier) is int and identifier > 0, "FAULT_CAPTURE_ID")
    detail = api_json(inspect_port, f"/api/requests/{identifier}", budget)
    require(isinstance(detail, dict) and detail.get("id") == identifier
            and detail.get("session_id") == fixture.session
            and (cancelled_detail(detail.get("status")) if fixture.mode == "cancel" else detail.get("status") == expected),
            "FAULT_CAPTURE_DETAIL")
    request = api_json(inspect_port, f"/api/requests/{identifier}/body/request", budget)
    require(isinstance(request, dict) and request.get("has_body") is True
            and request.get("reconstructed") is False and request.get("text", "").encode() == fixture.body,
            "FAULT_CAPTURE_REQUEST_BYTES")
    result = {"mode": fixture.mode, "request_id": identifier, "session_id": fixture.session,
              "terminal_state": expected, "live_request_body_sha256": digest(fixture.body)}
    if fixture.mode == "cancel":
        # Durable partial evidence below is SQL-backed, not an assumption that
        # the terminal snapshot/full response body was written on cancellation.
        require(detail.get("is_streaming") is True and detail.get("response_status") == 200,
                "FAULT_CANCEL_CAPTURE_METADATA")
        return result
    error = fixture.mode in ("429", "500")
    if error:
        require(detail.get("response_status") == int(fixture.mode)
                and detail.get("is_streaming") is False and detail.get("completed_at"), "FAULT_ERROR_CAPTURE_METADATA")
    else:
        validate_detail(detail, fixture, identifier)
    response = api_json(inspect_port, f"/api/requests/{identifier}/body/response", budget)
    require(isinstance(response, dict) and response.get("has_body") is True
            and isinstance(response.get("text"), str), "FAULT_CAPTURE_RESPONSE_BODY")
    raw = response["text"].encode()
    require(0 < len(raw) <= MAX_BODY and response.get("byte_size") == len(raw), "FAULT_CAPTURE_RESPONSE_SIZE")
    if error:
        require(raw == fixture.error_body, "FAULT_ERROR_CAPTURE_BODY")
    else:
        validate_response(raw, fixture)
    result["response_body_sha256"] = digest(raw)
    return result


def durable_snapshot(cluster, database, fixtures, captures):
    owned_database(cluster, database)
    budget = Budget(30)
    while True:
        value = json.loads(cluster.sql(database, "BEGIN READ ONLY; " + DURABLE_SQL + "; COMMIT;"))
        require(isinstance(value, list) and len(value) <= len(captures), "FAULT_DURABLE_COUNT")
        if len(value) == len(captures) and all(isinstance(row, dict) and row.get("terminal") is True for row in value):
            break
        time.sleep(min(0.05, budget.remaining()))
    require(isinstance(value, list) and len(value) == len(captures), "FAULT_DURABLE_COUNT")
    by_id = {item["request_id"]: item for item in captures}
    require(len(by_id) == len(captures), "FAULT_DUPLICATE_CAPTURE_ID")
    fixture_by_mode = {item.mode: item for item in fixtures}
    seen = set()
    for row in value:
        require(isinstance(row, dict) and type(row.get("id")) is int and row["id"] in by_id
                and row["id"] not in seen, "FAULT_DURABLE_ID")
        seen.add(row["id"])
        expected = by_id[row["id"]]
        fixture = fixture_by_mode[expected["mode"]]
        require(row.get("session_id") == expected["session_id"] and row.get("terminal") is True
                and row.get("status") == expected["terminal_state"].lower(), "FAULT_DURABLE_TERMINAL")
        if fixture.mode == "cancel":
            require(row.get("blocks") == [{"kind": "text", "text": TEXT, "seq": 0}]
                    and row.get("abort_errors") == 1, "FAULT_DURABLE_PARTIAL_BLOCK")
        else:
            require(row.get("response_count") == 1 and row.get("response_status") == (
                int(fixture.mode) if fixture.mode in ("429", "500") else 200)
                and isinstance(row.get("response_hex"), str), "FAULT_DURABLE_RESPONSE")
            try:
                raw = bytes.fromhex(row["response_hex"])
            except ValueError as error:
                raise JourneyError("FAULT_DURABLE_RESPONSE_HEX") from error
            require(len(raw) <= MAX_BODY and digest(raw) == expected["response_body_sha256"],
                    "FAULT_DURABLE_RESPONSE_BYTES")
    return {"exact_durable_id_session_set": True, "captures": len(value),
            "cancelled_partial_text_and_error_durable": any(item["mode"] == "cancel" for item in captures),
            "durable_snapshot_sha256": digest(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())}


def bound_run(paths, manifest, root, result, cluster, inventory):
    database = cluster.create(seeded=False)
    owned_database(cluster, database)
    require(cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0", "FAULT_DATABASE_NOT_EMPTY")
    stack = ExitStack()
    try:
        faults, upstream_port = stack.enter_context(fault_fixture())
        ports, reservations = stack.enter_context(reserved_ports())
        config, home, cwd = create_config(root, ports, upstream_port, database_url=cluster.config_url(database))
        isolated_config(config, upstream_port, {"max_requests": 32, "concurrent_sessions": 1})
        protected = {path: sha256(path) for path in (config, home / ".env", cwd / ".env")}
        for reservation in reservations:
            reservation.close()
        generations, captures = [], []
        before = snapshot = None
        for phase in ("isolation", "faults", "durability_restart"):
            current_paths, binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
            require(current_paths == paths and binding == result["binding"]
                    and verified_migration_inventory(paths, manifest) == inventory, "FAULT_BINDING_CHANGED")
            child_stack = ExitStack()
            try:
                child = child_stack.enter_context(owned_child([str(paths["mike"]), "--config", str(config), "serve"],
                                                             cwd, runtime_environment(home)))
                check(result, "archive_fault_" + phase + "_readiness", lambda: readiness(
                    child, config, ports, manifest, generations))
                ledger = check(result, "archive_fault_" + phase + "_ledger", lambda: migration_ledger(cluster, database, inventory))
                if before is None:
                    before = ledger
                require(ledger == before, "FAULT_LEDGER_CHANGED")
                if phase == "isolation":
                    check(result, "archive_fault_disable_features", lambda: disable_features(ports[1], Budget(180)))
                else:
                    flags = check(result, "archive_fault_" + phase + "_effective_features", lambda: verify_features(ports[1], Budget(10)))
                    result["faults"]["effective_feature_flags"] = flags["effective_feature_flags"]
                    if phase == "faults":
                        for fixture in faults.fixtures:
                            check(result, "archive_fault_" + fixture.mode + "_wire", lambda: run_request(ports[0], fixture, faults))
                            captured = check(result, "archive_fault_" + fixture.mode + "_capture", lambda: capture(ports[1], fixture))
                            captures.append(captured)
                            check(result, "archive_fault_" + fixture.mode + "_durable", lambda: durable_snapshot(cluster, database, faults.fixtures, captures))
                        require(faults.attempts == {mode: 1 for mode in MODES} and faults.failures == 0,
                                "FAULT_FINAL_UPSTREAM_ACCOUNTING")
                        snapshot = durable_snapshot(cluster, database, faults.fixtures, captures)
                    else:
                        observed = check(result, "archive_fault_restart_durable", lambda: durable_snapshot(cluster, database, faults.fixtures, captures))
                        require(snapshot == observed, "FAULT_DURABILITY_CHANGED_AFTER_RESTART")
                        result["faults"].update(workload_state="PASSED", durable_capture_count=len(captures),
                                               capacity_recovered_with_max_in_flight_one=True, durable_restart_continuity=True)
                require(all(sha256(path) == value for path, value in protected.items()), "FAULT_PRIVATE_FILES_CHANGED")
                check(result, "archive_fault_" + phase + "_stop", lambda: graceful_stop(child))
            finally:
                check(result, "archive_fault_" + phase + "_process_cleanup", lambda: cleanup(child_stack, "owned_process_group_only"))
    finally:
        check(result, "archive_fault_fixture_cleanup", lambda: cleanup(stack, "owned_loopback_fixture_only"))
    current_paths, binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
    require(current_paths == paths and binding == result["binding"], "FAULT_BINDING_CHANGED")
    return {"fresh_database_only": True, "upstream_attempts": len(MODES), "independent_fixture": True}


def run_fault_journey(binary_dir, manifest, sandbox, manifest_sha256):
    result = new_result(None)
    result.update(runner="archive-upstream-fault-helper", limits=list(LIMITS), faults={
        "workload_state": "NOT_RUN", "profile": "synthetic_isolation_overrides", "effective_feature_flags": None,
        "requested_disabled_feature_keys": list(FEATURES), "max_in_flight": 1,
        "max_requests": len(MODES), "max_upstream_workers": MAX_WORKERS,
        "max_deadline_timer_workers": MAX_WORKERS, "wire_deadline_seconds": TIMEOUT,
        "first_event_observation_allowance_seconds": 1, "completion_observation_allowance_seconds": 1,
        "server_connection_deadline_seconds": TIMEOUT,
        "maximum_body_bytes": MAX_BODY, "unmeasured": {key: "NOT_RUN" for key in
            ("database_outage", "quota_disk_fault", "persistence_failure_policy", "supported_loss_window", "normal_defaults", "full_AC21")}})
    try:
        paths, result["binding"] = verify_binding(binary_dir, manifest, manifest_sha256)
        inventory = verified_migration_inventory(paths, manifest)
        cluster = owned_cluster()
        try:
            check(result, "archive_fault_bounded_run", lambda: bound_run(paths, manifest, Path(sandbox) / "faults", result, cluster, inventory))
        finally:
            def cleanup_database():
                cluster.cleanup()
                return {"only_databases_created_by_this_run": True, "cluster_left_running": True}
            check(result, "archive_fault_database_cleanup", cleanup_database)
    except Exception as error:
        result.update(state="FAILED", failure_type=type(error).__name__)
        result["faults"]["workload_state"] = "FAILED"
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    return result
