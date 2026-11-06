"""Capture, predecessor backup and incompatible-schema refusal observations."""
import hashlib
import json
import subprocess

from journey import check
from journey_http import validate_detail
from journey_maintenance import InstalledKit, runtime_roots
from journey_mcp import run_mcp
from journey_persistent import LEDGER_SQL, graceful_stop, migration_ledger
from journey_process import JourneyError, http_json, owned_child, require, runtime_environment, wait_owned_ready
from journey_reliability import Budget, capture_batch, database_counts, validate_response
from journey_reliability_fixture import MAX_BODY, run_batch
from journey_reliability_isolation import disable_features, verify_features, verify_service_prerequisites
from journey_upgrade_state import durable_snapshot
from owned_process import OwnedProcess
from recovery import verify
from recovery_files import inventory


def read_ledger(cluster, database):
    require(database in cluster.created, "UPGRADE_DATABASE_NOT_OWNED")
    return json.loads(cluster.sql(database, "BEGIN READ ONLY; " + LEDGER_SQL + "; COMMIT;"))


def capture_evidence(port, fixture, expected):
    request_id = expected["request_id"]
    detail = http_json(port, f"/api/requests/{request_id}")
    validate_detail(detail, fixture, request_id)
    observed = {"request_id": request_id, "session_id": fixture.session, "tool_use_id": fixture.tool_use_id}
    for kind in ("request", "response"):
        value = http_json(port, f"/api/requests/{request_id}/body/{kind}")
        require(isinstance(value, dict) and value.get("has_body") is True
                and isinstance(value.get("text"), str), "UPGRADE_CAPTURE_BODY_MISSING")
        raw = value["text"].encode()
        require(0 < len(raw) <= MAX_BODY and type(value.get("byte_size")) is int
                and value["byte_size"] == len(raw), "UPGRADE_CAPTURE_BODY_SIZE")
        if kind == "request":
            require(raw == fixture.body and value.get("reconstructed") is False, "UPGRADE_REQUEST_CHANGED")
        else:
            validate_response(raw, fixture)
        observed[kind + "_body_sha256"] = hashlib.sha256(raw).hexdigest()
    require(observed == expected, "UPGRADE_CAPTURE_ID_SESSION_OR_BODY_CHANGED")
    return observed


class Runtime:
    def __init__(self, result, cluster, database, config, home, cwd, ports, upstream_port, cohort):
        self.result, self.cluster, self.database = result, cluster, database
        self.config, self.home, self.cwd = config, home, cwd
        self.ports, self.upstream_port, self.cohort = ports, upstream_port, cohort
        self.generations, self.captures = [], []

    def check(self, phase, suffix, action):
        return check(self.result, "archive_upgrade_" + phase + "_" + suffix, action)

    def ready(self, child, archive):
        health = wait_owned_ready(child, self.config, self.ports[1], archive.manifest["source_commit"],
                                  timeout=240, persistent=True)
        generation = health["instance"]["boot_generation"]
        require(generation not in self.generations, "UPGRADE_BOOT_GENERATION_REUSED")
        self.generations.append(generation)
        services = verify_service_prerequisites(health)
        return {"owned_pid_and_new_generation": True, "exact_source_commit": True,
                "persistent_enabled_and_reachable": True, "runtime_path": "empty", **services}

    def continuity(self):
        values = [capture_evidence(self.ports[1], fixture, expected) for fixture, expected in self.captures]
        database_counts(self.cluster, self.database, len(values), 1)
        return {"request_count": len(values), "same_ids_sessions_and_body_hashes": True,
                "capture_sha256": hashlib.sha256(json.dumps(values, sort_keys=True).encode()).hexdigest()}

    def capture(self):
        batch = len(self.captures)
        fixtures = self.cohort.prepare(batch)
        traffic = run_batch(self.ports[0], self.cohort, fixtures)
        require(traffic["completed"] == 1, "UPGRADE_SYNTHETIC_CAPTURE_INCOMPLETE")
        seen = {value["request_id"]: value["session_id"] for _, value in self.captures}
        observed = capture_batch(self.ports[1], fixtures, batch, seen, Budget(35))
        require(len(observed) == 1, "UPGRADE_SYNTHETIC_CAPTURE_COUNT")
        self.captures.append((fixtures[0], observed[0]))
        self.continuity()
        return {"new_durable_capture": observed[0], "real_synthetic_stream": True}

    def serve(self, archive, phase, *, capture=False, isolate=False, while_ready=None):
        self.check(phase, "binding", archive.recheck)
        with owned_child([str(archive.paths["mike"]), "--config", str(self.config), "serve"],
                         self.cwd, runtime_environment(self.home)) as child:
            self.check(phase, "owned_readiness", lambda: self.ready(child, archive))
            self.check(phase, "full_migration_ledger", lambda: migration_ledger(
                self.cluster, self.database, archive.inventory))
            if isolate:
                self.check(phase, "supported_feature_isolation", lambda: disable_features(self.ports[1], Budget(180)))
            else:
                self.check(phase, "effective_feature_isolation", lambda: verify_features(self.ports[1], Budget(15)))
                if self.captures:
                    self.check(phase, "old_capture_continuity", self.continuity)
                if capture:
                    self.check(phase, "new_stream_and_capture", self.capture)
                if self.captures:
                    _, observed = self.captures[-1]
                    self.check(phase, "mcp_read", lambda: run_mcp(archive.paths["mike-mcp"], self.ports[1],
                        observed["request_id"], observed["session_id"], archive.manifest["version"], self.home, self.cwd))
                if while_ready is not None:
                    while_ready()
            self.check(phase, "owned_stop", lambda: graceful_stop(child))

    def kit(self, archive):
        return InstalledKit(archive.paths, archive.manifest, archive.binding, archive.prefix,
                            archive.inventory, self.home, self.cwd)


def predecessor_backup(runtime, previous, service, bundle):
    kit = runtime.kit(previous)
    roots = runtime_roots(kit, runtime.config, runtime.config.parent)
    before = durable_snapshot(runtime.cluster, runtime.database, service, roots)
    kit.command("backup", ["--config", roots["config"], "--data-root", roots["data"],
                "--state-root", roots["state"], "--pg-service-file", service, "--pg-service", "fixture",
                "--destination", bundle])
    kit.command("verify", ["--backup", bundle])
    saved, index = verify(bundle)
    expected_ledger = [{key: row[key] for key in ("version", "checksum", "success")}
                       for row in read_ledger(runtime.cluster, runtime.database)]
    require(saved["installed_manifest_sha256"] == previous.identity
            and saved["source_commit"] == previous.manifest["source_commit"]
            and saved["version"] == previous.manifest["version"]
            and index["files"] == inventory(roots) and index["migration_ledger"] == expected_ledger,
            "UPGRADE_PREDECESSOR_BACKUP_BINDING")
    require(durable_snapshot(runtime.cluster, runtime.database, service, roots) == before,
            "UPGRADE_BACKUP_CHANGED_SOURCE")
    return roots, {"exact_predecessor_installed_kit": True, "verified_complete_backup": True,
                   "owned_writers_stopped": True, "source_unchanged": True}


REFUSAL_REASONS = (
    b"migration ledger is newer than, changed from, or incompatible with this binary",
    b"was previously applied but is missing in the resolved migrations",
    b"applied but absent from this binary",
)


def incompatible_refusal(runtime, previous, candidate, roots, service):
    previous.recheck(active=False)
    candidate.recheck()
    before = durable_snapshot(runtime.cluster, runtime.database, service, roots)
    child = OwnedProcess([str(previous.paths["mike"]), "--config", str(runtime.config), "serve"],
                         cwd=runtime.cwd, env=runtime_environment(runtime.home),
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, stderr = child.communicate(timeout=60)
        require(type(child.returncode) is int and child.returncode > 0,
                "UPGRADE_INCOMPATIBLE_PREDECESSOR_DID_NOT_REFUSE")
        require(any(reason in stdout + stderr for reason in REFUSAL_REASONS),
                "UPGRADE_PREDECESSOR_WRONG_REFUSAL_REASON")
    except subprocess.TimeoutExpired as error:
        raise JourneyError("UPGRADE_INCOMPATIBLE_PREDECESSOR_TIMEOUT") from error
    finally:
        child.close()
    require(durable_snapshot(runtime.cluster, runtime.database, service, roots) == before,
            "UPGRADE_INCOMPATIBLE_PREDECESSOR_MUTATED_STATE")
    candidate.recheck()
    return {"positive_nonzero_exit": True, "recognized_incompatible_migration_reason": True,
            "complete_logical_database_dump_unchanged": True, "durable_config_data_state_unchanged": True,
            "active_candidate_preserved": True, "ephemeral_lifecycle_and_logs_excluded": True}
