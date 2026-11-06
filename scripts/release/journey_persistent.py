"""Opt-in, owned PG19 archive restart helpers. Full acceptance stays NOT_RUN.

The recovery fixture supplies ownership checks and fresh database allocation only.
Its synthetic seed/ledger is never used: only the hash-bound installed binary may
migrate the empty database. No existing database, cluster restart or daemon is adopted.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

from archive import binary_identity
from common import ReleaseError, sha256
from integration_recovery_fixture import Cluster
from journey import check, create_config, doctor, new_result, verify_binding
from journey_fixture import MODEL, TEXT, TOOL_NAME, upstream_fixture
from journey_http import validate_detail, stream_capture
from journey_mcp import run_mcp
from journey_process import (JourneyError, http_json, installation_identity, owned_child, private_write, require,
                             reserved_ports, runtime_environment, wait_owned_ready)
from migration_inventory import validate_migration_inventory
from recovery_database import EMPTY_SQL

LIMITS = [
    "Opt-in disposable PostgreSQL 19 helper only; no Docker provisioning or native account qualification.",
    "Only the exact installed binary applies embedded migrations; fixture SQL never seeds an application schema or ledger.",
    "Owned foreground SIGTERM and serve restart are not installed CLI/TUI stop/start, logout or autostart qualification.",
    "No browser execution, search, real Claude provider/client, security-boundary or published-channel qualification.",
    "A private HOME and empty runtime PATH are not an OS sandbox. Candidate executables must already be trusted.",
    "All AC01-AC27 remain required NOT_RUN. Partial subchecks never qualify a case or release.",
]
LEDGER_SQL = """SELECT coalesce(json_agg(json_build_object(
 'version',version,'description',description,'installed_on',installed_on,
 'success',success,'checksum',encode(checksum,'hex'),'execution_time',execution_time)
 ORDER BY version),'[]'::json)::text FROM public._sqlx_migrations"""


def owned_cluster():
    require(bool(os.environ.get("MIKE_TEST_ADMIN_URL", "").strip())
            and bool(os.environ.get("MIKE_RECOVERY_TEST_PGDATA", "").strip()),
            "PERSISTENT_FIXTURE_REQUIRED")
    try:
        return Cluster()
    except Exception as error:
        raise JourneyError("PERSISTENT_FIXTURE_INVALID_OR_UNAVAILABLE") from error


def inventory_digest(inventory):
    return hashlib.sha256(json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def verified_migration_inventory(paths, manifest):
    # Execute the exact installed bytes before trusting their source inventory.
    # Manifest labels or a doctor's broad DATABASE_CURRENT are not this evidence.
    try:
        artifacts = manifest["artifacts"]
        require(isinstance(artifacts, list) and len(artifacts) == 2
                and {item["binary"] for item in artifacts} == {"mike", "mike-mcp"}
                and set(paths) == {"mike", "mike-mcp"}, "PERSISTENT_CANDIDATE_SOURCE_IDENTITY")
        with tempfile.TemporaryDirectory(prefix="mike-persistent-identity-") as temporary:
            root = Path(temporary)
            home, cwd = root / "home", root / "unrelated-cwd"
            home.mkdir(mode=0o700)
            cwd.mkdir(mode=0o700)
            for artifact in artifacts:
                identity = binary_identity(paths[artifact["binary"]], manifest["version"],
                                           manifest["source_commit"], manifest["source_date_epoch"], cwd, home)
                require(identity == artifact.get("build_info"), "PERSISTENT_CANDIDATE_SOURCE_IDENTITY")
    except (ReleaseError, KeyError, TypeError, ValueError) as error:
        raise JourneyError("PERSISTENT_CANDIDATE_SOURCE_IDENTITY") from error
    return checked_migration_inventory(manifest.get("migration_inventory"), manifest.get("source_commit"))


def checked_migration_inventory(inventory, source_commit):
    try:
        return validate_migration_inventory(inventory, source_commit)
    except ReleaseError as error:
        raise JourneyError("PERSISTENT_CANDIDATE_INVENTORY_INVALID") from error


def migration_ledger(cluster, database, inventory):
    checked_migration_inventory(inventory, inventory.get("source_commit") if isinstance(inventory, dict) else None)
    # Independently compare the complete candidate set and every success flag;
    # hash every ledger field so restart cannot conceal a rewritten history.
    rows = json.loads(cluster.sql(database, "BEGIN READ ONLY; " + LEDGER_SQL + "; COMMIT;"))
    require(isinstance(rows, list) and bool(rows), "PERSISTENT_LEDGER_EMPTY")
    versions = []
    for row in rows:
        require(isinstance(row, dict) and set(row) == {
            "version", "description", "installed_on", "success", "checksum", "execution_time"},
            "PERSISTENT_LEDGER_SCHEMA")
        require(type(row["version"]) is int and row["version"] > 0
                and row["success"] is True and isinstance(row["checksum"], str)
                and re.fullmatch(r"[0-9a-f]{96}", row["checksum"])
                and isinstance(row["description"], str) and bool(row["description"])
                and isinstance(row["installed_on"], str) and bool(row["installed_on"])
                and type(row["execution_time"]) is int and row["execution_time"] >= 0,
                "PERSISTENT_LEDGER_INVALID_OR_FAILED")
        versions.append(row["version"])
    require(versions == sorted(set(versions)), "PERSISTENT_LEDGER_ORDER")
    expected = [(item["version"], item["checksum"]) for item in inventory["migrations"]]
    require([(row["version"], row["checksum"]) for row in rows] == expected,
            "PERSISTENT_LEDGER_CANDIDATE_MISMATCH")
    digest = hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"migration_count": len(rows), "full_ledger_sha256": digest, "failed_rows": 0,
            "candidate_inventory_sha256": inventory_digest(inventory), "exact_candidate_set": True}


def validate_stored_response(body):
    # Streaming persistence stores an assembled message JSON, not SSE framing.
    # Verify its fixture content here; continuity compares the exact stored bytes.
    try:
        value = json.loads(body)
    except (ValueError, UnicodeError) as error:
        raise JourneyError("PERSISTENT_RESPONSE_NOT_MESSAGE_JSON") from error
    require(isinstance(value, dict) and value.get("id") == "msg_archive_fixture"
            and value.get("type") == "message" and value.get("role") == "assistant"
            and value.get("model") == MODEL and value.get("stop_reason") == "tool_use"
            and value.get("stop_sequence") is None, "PERSISTENT_RESPONSE_METADATA")
    require(value.get("content") == [
        {"type": "text", "text": TEXT},
        {"type": "tool_use", "id": "tool_archive_response", "name": TOOL_NAME, "input": {"echo": "synthetic"}}],
        "PERSISTENT_RESPONSE_CONTENT")
    usage = value.get("usage")
    require(isinstance(usage, dict) and usage.get("input_tokens") == 25
            and usage.get("output_tokens") == 12, "PERSISTENT_RESPONSE_USAGE")


def stored_capture(inspect_port, fixture, *, expected_id=None, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        rows = http_json(inspect_port, "/api/requests")
        require(isinstance(rows, list) and all(isinstance(row, dict) for row in rows), "CAPTURE_LIST_SHAPE")
        require(len(rows) <= 1, "PERSISTENT_CAPTURE_DUPLICATED_OR_FOREIGN")
        if rows and rows[0].get("status") == "Complete":
            request_id = rows[0].get("id")
            require(type(request_id) is int and request_id > 0, "PERSISTENT_CAPTURE_ID")
            require(expected_id is None or request_id == expected_id, "PERSISTENT_CAPTURE_ID_CHANGED")
            detail = http_json(inspect_port, f"/api/requests/{request_id}")
            validate_detail(detail, fixture, request_id)
            evidence = {"request_id": request_id, "session_id": fixture.session,
                        "request_count": 1, "terminal_state": "Complete"}
            for kind in ("request", "response"):
                value = http_json(inspect_port, f"/api/requests/{request_id}/body/{kind}")
                require(isinstance(value, dict) and value.get("has_body") is True
                        and isinstance(value.get("text"), str), "PERSISTENT_BODY_MISSING")
                actual = value["text"].encode()
                require(actual and type(value.get("byte_size")) is int
                        and value["byte_size"] == len(actual), "PERSISTENT_BODY_CHANGED")
                if kind == "request":
                    require(actual == fixture.body, "PERSISTENT_BODY_CHANGED")
                else:
                    validate_stored_response(actual)
                evidence[kind + "_body_sha256"] = hashlib.sha256(actual).hexdigest()
            evidence["response_body_kind"] = "stored_assembled_message_json"
            return evidence
        time.sleep(0.05)
    raise JourneyError("PERSISTENT_CAPTURE_COMPLETION_TIMEOUT")


def stored_search(inspect_port, fixture, request_id, timeout=15):
    """Unscoped response search must find the one stored capture; the match names its request and session."""
    deadline = time.monotonic() + timeout
    path = "/api/body/find?source=response&needle=" + TEXT
    while time.monotonic() < deadline:
        found = http_json(inspect_port, path)
        matches = found.get("matches") if isinstance(found, dict) else None
        if matches:
            require(all(isinstance(item, dict) and item.get("request_id") == request_id
                        and item.get("session_id") == fixture.session and item.get("matched") == TEXT
                        for item in matches), "PERSISTENT_SEARCH_WRONG_MATCH")
            return {"request_id": request_id, "session_id": fixture.session, "match_count": len(matches),
                    "source": "response"}
        time.sleep(0.2)
    raise JourneyError("PERSISTENT_SEARCH_NO_MATCH")


def graceful_stop(child, timeout=70):
    require(child.poll() is None, "PERSISTENT_DAEMON_EXITED_BEFORE_STOP")
    # The supervisor keeps the exact owned group reserved until context cleanup.
    # Never signal a pid read from disk, or adopt another listener.
    child.signal_group(signal.SIGTERM)
    try:
        child.wait(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        raise JourneyError("PERSISTENT_GRACEFUL_STOP_TIMEOUT") from error
    require(child.returncode == 0, "PERSISTENT_GRACEFUL_STOP_NONZERO")
    return {"owned_group_only": True, "signal": "SIGTERM", "exit_status": 0,
            "installed_cli_tui_stop": "NOT_RUN"}


def run_bound(paths, manifest, root, result, cluster, inventory, maintenance_prefix=None):
    databases = []
    def fresh_database():
        database = cluster.create(seeded=False)
        databases.append(database)
        require(cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0",
                "PERSISTENT_FIXTURE_NOT_EMPTY")
        return {"new_owned_database": True, "empty_before_candidate": True, "synthetic_seed_used": False}
    check(result, "archive_persistent_empty_database", fresh_database)
    database = databases[0]
    with upstream_fixture() as (fixture, upstream_port), reserved_ports() as (ports, reservations):
        config, home, first_cwd = create_config(root, ports, upstream_port,
                                               database_url=cluster.config_url(database))
        if maintenance_prefix is not None:
            _, record = installation_identity(config)
            record.parent.parent.mkdir(mode=0o700)
            data = record.parent.parent / "data"
            data.mkdir(mode=0o700)
            private_write(data / "synthetic-maintenance-fixture.txt", b"Explicitly synthetic backup fixture, not captured application data.\n")
        second_cwd = root / "second unrelated cwd"
        second_cwd.mkdir(mode=0o700)
        private_write(second_cwd / ".env", (first_cwd / ".env").read_bytes())
        protected = [config, home / ".env", first_cwd / ".env", second_cwd / ".env"]
        before_files = {path: sha256(path) for path in protected}
        environment = runtime_environment(home)
        for reservation in reservations:
            reservation.close()
        generations = []
        before = None
        ledger = None
        mcp = None
        for index, cwd in enumerate((first_cwd, second_cwd)):
            phase = "initial" if index == 0 else "restarted"
            def recheck_binding():
                current_paths, current_binding = verify_binding(paths["mike"].parent, manifest,
                                                                result["binding"]["manifest_sha256"])
                require(current_paths == paths and current_binding == result["binding"],
                        "PERSISTENT_INSTALLED_BINDING_CHANGED")
                require(verified_migration_inventory(paths, manifest) == inventory,
                        "PERSISTENT_CANDIDATE_INVENTORY_CHANGED")
                return {"exact_installed_hashes": True, "executable_source_identity": True}
            check(result, "archive_persistent_" + phase + "_hash_binding", recheck_binding)
            with owned_child([str(paths["mike"]), "--config", str(config), "serve"], cwd, environment) as child:
                def readiness():
                    health = wait_owned_ready(child, config, ports[1], manifest["source_commit"],
                                              timeout=240, persistent=True)
                    generation = health["instance"]["boot_generation"]
                    require(generation not in generations, "PERSISTENT_BOOT_GENERATION_REUSED")
                    generations.append(generation)
                    return {"owned_pid_and_generation": True, "persistent_enabled_and_reachable": True,
                            "runtime_path": "empty", "different_cwd": index == 1}
                check(result, "archive_persistent_" + phase + "_readiness", readiness)
                check(result, "archive_persistent_" + phase + "_candidate_doctor",
                      lambda: doctor(paths["mike"], config, home, cwd, manifest, persistent=True))
                observed_ledger = check(result, "archive_persistent_" + phase + "_successful_ledger",
                                        lambda: migration_ledger(cluster, database, inventory))
                if index == 0:
                    ledger = observed_ledger
                    check(result, "archive_persistent_synchronized_streaming", lambda: stream_capture(ports[0], fixture))
                observed = check(result, "archive_persistent_" + phase + "_stored_bodies",
                                 lambda: stored_capture(ports[1], fixture,
                                                        expected_id=before["request_id"] if before else None))
                check(result, "archive_persistent_" + phase + "_search",
                      lambda: stored_search(ports[1], fixture, observed["request_id"]))
                if index == 0:
                    before = observed
                else:
                    def continuity():
                        require(observed == before, "PERSISTENT_CAPTURE_CHANGED_AFTER_RESTART")
                        require(observed_ledger == ledger, "PERSISTENT_LEDGER_CHANGED_AFTER_RESTART")
                        require(all(sha256(path) == digest for path, digest in before_files.items()),
                                "PERSISTENT_CONFIG_OR_CWD_FILES_CHANGED")
                        return {"same_request_session_and_body_hashes": True, "no_duplicate_capture": True,
                                "full_ledger_unchanged": True, "same_config_and_data_root": True,
                                "config_and_poison_files_unchanged": True}
                    check(result, "archive_persistent_restart_continuity", continuity)
                mcp = check(result, "archive_persistent_" + phase + "_mcp_read",
                      lambda: run_mcp(paths["mike-mcp"], ports[1], observed["request_id"], fixture.session,
                                      manifest["version"], home, cwd))
                check(result, "archive_persistent_" + phase + "_owned_stop", lambda: graceful_stop(child))
        if maintenance_prefix is not None:
            from journey_maintenance import LIMITS as MAINTENANCE_LIMITS, run_maintenance_roundtrip
            result["limits"].extend(MAINTENANCE_LIMITS)
            source = {"prefix": maintenance_prefix, "config": config, "home": home, "cwd": second_cwd,
                      "database": database, "capture": before, "ledger": ledger, "mcp": mcp,
                      "fixture": fixture, "ports": ports, "generations": generations}
            run_maintenance_roundtrip(paths, manifest, root.parent / "maintenance", result,
                                      cluster, inventory, source)
    return {"both_foreground_generations_observed": True, "different_restart_cwd": True}


def run_persistent_journey(binary_dir, manifest, sandbox, manifest_sha256, maintenance=False):
    """Explicit opt-in only; a missing/unusable fixture is a failure, never a skip."""
    result = new_result(None)
    result.update(runner="archive-persistent-restart-helper", limits=list(LIMITS))
    cluster = None
    try:
        paths, result["binding"] = verify_binding(binary_dir, manifest, manifest_sha256)
        check(result, "archive_persistent_hash_binding", lambda: {"exact_installed_hashes": True})
        inventory = None
        def candidate_inventory():
            nonlocal inventory
            inventory = verified_migration_inventory(paths, manifest)
            return {"executable_source_identity": True, "source_commit": inventory["source_commit"],
                    "migration_count": len(inventory["migrations"]),
                    "candidate_inventory_sha256": inventory_digest(inventory)}
        check(result, "archive_persistent_candidate_inventory", candidate_inventory)
        def fixture_check():
            nonlocal cluster
            cluster = owned_cluster()
            return {"postgres_major": 19, "owned_private_pgdata": True,
                    "password_free_loopback_admin": True, "tcp_only": True}
        check(result, "archive_persistent_owned_fixture", fixture_check)
        try:
            check(result, "archive_persistent_capture_and_restart",
                  lambda: run_bound(paths, manifest, Path(sandbox) / "persistent", result, cluster, inventory,
                                    **({"maintenance_prefix": Path(binary_dir).parent} if maintenance else {})))
        finally:
            def cleanup():
                cluster.cleanup()
                return {"only_databases_created_by_this_run": True, "cluster_left_running": True}
            check(result, "archive_persistent_database_cleanup", cleanup)
    except Exception as error:
        result.update(state="FAILED", failure_type=type(error).__name__)
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    return result
