"""Independent persistent browser fixture, never full browser/AC qualification.

Trial mode lacks durable session and response-body APIs. Browser verification
therefore gets its own newly allocated PG19 database, private config, synthetic
capture and foreground server. It never adds requests to the baseline persistent
or maintenance fixture, adopts an existing database, or starts a database server.
"""
from pathlib import Path
import time

from common import sha256
from journey import check, create_config, doctor, new_result, verify_binding
from journey_browser import browser_tools, run_browser
from journey_browser_security import run_browser_security
from journey_browser_ipv6 import run_ipv6_boundary
from journey_fixture import upstream_fixture
from journey_http import spa_http, stream_capture
from journey_persistent import (graceful_stop, inventory_digest, migration_ledger, owned_cluster,
                                stored_capture, verified_migration_inventory)
from journey_process import (JourneyError, http_json, owned_child, require, reserved_ports,
                             runtime_environment, wait_owned_ready)
from recovery_database import EMPTY_SQL

LIMITS = [
    "Browser checks use a separate newly allocated owned PG19 database; baseline restart and maintenance captures are untouched.",
    "Installed Chromium and existing Playwright are explicitly selected; no download, remote browser attachment or sandbox workaround.",
    "Only the exact hash-bound candidate migrates the empty browser database; fixture SQL never seeds application rows or a migration ledger.",
    "Synthetic provider, installed DOM and native-browser hostile/null-origin probes only; no complete AC06, real Claude, clean-account or published-channel qualification.",
    "IPv6 has a separate opt-in V6ONLY loopback driver; an omitted or unavailable IPv6 run is never covered by IPv4 results.",
    "Private HOME, browser context and request guards are not an OS sandbox; candidate executables must already be trusted.",
    "All full acceptance cases remain NOT_RUN; passing helper observations never qualify a release.",
]


def persisted_session(port, session, request_id, count, timeout=15):
    """Require the same persistence-backed APIs used by RequestDetailColumn."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        rows = http_json(port, "/api/sessions")
        require(isinstance(rows, list) and all(isinstance(row, dict) for row in rows), "BROWSER_SESSION_LIST_SHAPE")
        require(len(rows) <= 1 and all(row.get("id") == session for row in rows), "BROWSER_FOREIGN_SESSION")
        require(all(type(row.get("request_count")) is int and 0 <= row["request_count"] <= count for row in rows),
                "BROWSER_SESSION_COUNT_INVALID")
        if rows and rows[0].get("request_count") == count:
            detail = http_json(port, f"/api/sessions/{session}")
            require(isinstance(detail, dict) and isinstance(detail.get("summary"), dict)
                    and isinstance(detail.get("requests"), list), "BROWSER_SESSION_DETAIL_SHAPE")
            require(detail["summary"].get("id") == session, "BROWSER_SESSION_DETAIL_IDENTITY")
            requests = detail["requests"]
            require(all(isinstance(row, dict) and type(row.get("id")) is int and row["id"] > 0 for row in requests),
                    "BROWSER_SESSION_REQUEST_SHAPE")
            ids = [row["id"] for row in requests]
            require(len(ids) <= count and len(set(ids)) == len(ids), "BROWSER_SESSION_CAPTURE_DUPLICATED")
            require(type(detail["summary"].get("request_count")) is int, "BROWSER_SESSION_COUNT_INVALID")
            if len(ids) == count and request_id in ids and detail["summary"].get("request_count") == count:
                return {"matching_persisted_session": True, "request_count": count,
                        "original_request_present": True, "session_list_and_detail": True}
        time.sleep(0.05)
    raise JourneyError("BROWSER_PERSISTED_SESSION_TIMEOUT")


def recheck_binding(paths, manifest, binding, inventory):
    actual_paths, actual_binding = verify_binding(paths["mike"].parent, manifest, binding["manifest_sha256"])
    require(actual_paths == paths and actual_binding == binding, "BROWSER_INSTALLED_BINDING_CHANGED")
    require(verified_migration_inventory(paths, manifest) == inventory, "BROWSER_CANDIDATE_INVENTORY_CHANGED")
    return {"exact_installed_hashes": True, "executable_source_identity": True}


def run_bound(paths, manifest, root, result, cluster, inventory, tools):
    databases = []
    def fresh_database():
        database = cluster.create(seeded=False)
        databases.append(database)
        require(cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0",
                "BROWSER_FIXTURE_NOT_EMPTY")
        return {"new_owned_database": True, "empty_before_candidate": True,
                "separate_from_restart_and_maintenance": True, "synthetic_seed_used": False}
    check(result, "archive_browser_empty_database", fresh_database)
    database = databases[0]
    with upstream_fixture() as (fixture, upstream_port), reserved_ports() as (ports, reservations):
        config, home, cwd = create_config(root, ports, upstream_port, database_url=cluster.config_url(database))
        protected = {path: sha256(path) for path in (config, home / ".env", cwd / ".env")}
        check(result, "archive_browser_runtime_hash_binding",
              lambda: recheck_binding(paths, manifest, result["binding"], inventory))
        for reservation in reservations:
            reservation.close()
        with owned_child([str(paths["mike"]), "--config", str(config), "serve"],
                         cwd, runtime_environment(home)) as child:
            def readiness():
                wait_owned_ready(child, config, ports[1], manifest["source_commit"], timeout=240, persistent=True)
                return {"owned_pid_and_generation": True, "persistent_enabled_and_reachable": True,
                        "runtime_path": "empty"}
            check(result, "archive_browser_persistent_readiness", readiness)
            check(result, "archive_browser_candidate_doctor",
                  lambda: doctor(paths["mike"], config, home, cwd, manifest, persistent=True))
            ledger = check(result, "archive_browser_candidate_ledger", lambda: migration_ledger(cluster, database, inventory))
            check(result, "archive_browser_synthetic_stream", lambda: stream_capture(ports[0], fixture))
            capture = check(result, "archive_browser_persisted_bodies", lambda: stored_capture(ports[1], fixture))
            request_id = capture["request_id"]
            check(result, "archive_browser_initial_persisted_session",
                  lambda: persisted_session(ports[1], fixture.session, request_id, 1))
            check(result, "archive_browser_packaged_http",
                  lambda: spa_http(ports[2], ports[1], ports[0], fixture.session, request_id))
            def browser_check():
                wait_owned_ready(child, config, ports[1], manifest["source_commit"], persistent=True)
                observation = run_browser(tools, ports, fixture.session, request_id, fixture, home, cwd)
                wait_owned_ready(child, config, ports[1], manifest["source_commit"], persistent=True)
                return observation
            check(result, "archive_packaged_browser_partial", browser_check)
            check(result, "archive_browser_two_persisted_captures",
                  lambda: persisted_session(ports[1], fixture.session, request_id, 2))
            def boundary_check():
                wait_owned_ready(child, config, ports[1], manifest["source_commit"], persistent=True)
                observation = run_browser_security(tools, ports, fixture.session, request_id, fixture, home, cwd)
                wait_owned_ready(child, config, ports[1], manifest["source_commit"], persistent=True)
                return observation
            check(result, "archive_browser_ipv4_native_boundary", boundary_check)
            check(result, "archive_browser_three_persisted_captures",
                  lambda: persisted_session(ports[1], fixture.session, request_id, 3))
            def continuity():
                require(migration_ledger(cluster, database, inventory) == ledger, "BROWSER_LEDGER_CHANGED")
                require(all(sha256(path) == digest for path, digest in protected.items()), "BROWSER_PRIVATE_FILES_CHANGED")
                recheck_binding(paths, manifest, result["binding"], inventory)
                return {"ledger_and_private_files_unchanged": True, "installed_binding_unchanged": True}
            check(result, "archive_browser_fixture_continuity", continuity)
            check(result, "archive_browser_owned_stop", lambda: graceful_stop(child))
        if tools.ipv6:
            recheck_binding(paths, manifest, result["binding"], inventory)
            run_ipv6_boundary(paths, manifest, root.parent / "browser ipv6", result, tools, cluster,
                              database, inventory, fixture, upstream_port, request_id)
            recheck_binding(paths, manifest, result["binding"], inventory)
        else:
            result["checks"].append({"id": "archive_browser_ipv6_native_boundary", "state": "NOT_RUN",
                "reason": "The implemented V6ONLY ::1 driver requires explicit --browser-ipv6 opt-in.",
                "binding": result["binding"]})
    return {"separate_persistent_browser_fixture": True}


def run_installed_browser_journey(binary_dir, manifest, sandbox, manifest_sha256, browser):
    result = new_result(None)
    result.update(runner="archive-installed-persistent-browser-helper", limits=list(LIMITS))
    try:
        paths, result["binding"] = verify_binding(binary_dir, manifest, manifest_sha256)
        check(result, "archive_browser_hash_binding", lambda: {"exact_installed_hashes": True})
        tools = []
        def tooling():
            tools.append(browser_tools(browser))
            return {"explicit_local_tools": True, "download_or_install": False}
        check(result, "archive_browser_explicit_tooling", tooling)
        inventories = []
        def candidate_inventory():
            inventories.append(verified_migration_inventory(paths, manifest))
            return {"executable_source_identity": True,
                    "candidate_inventory_sha256": inventory_digest(inventories[0])}
        check(result, "archive_browser_candidate_inventory", candidate_inventory)
        clusters = []
        def fixture_check():
            clusters.append(owned_cluster())
            return {"postgres_major": 19, "owned_private_pgdata": True, "tcp_only": True,
                    "independent_allocation_owner": True}
        check(result, "archive_browser_owned_fixture", fixture_check)
        cluster = clusters[0]
        try:
            check(result, "archive_browser_persistent_capture_and_dom",
                  lambda: run_bound(paths, manifest, Path(sandbox) / "browser", result,
                                    cluster, inventories[0], tools[0]))
        finally:
            def cleanup():
                cluster.cleanup()
                return {"only_browser_databases_created_by_this_run": True, "cluster_left_running": True}
            check(result, "archive_browser_database_cleanup", cleanup)
    except Exception as error:
        result.update(state="FAILED", failure_type=type(error).__name__)
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    return result
