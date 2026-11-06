"""Internal normal-default workload, callable only within the isolated runner.

The outer adapter owns engine-verified admission and qualification evidence.
This module reports owned process, HTTP and durable data observations only.
No candidate, server or database is launched merely by importing the module.
"""
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import time

from common import sha256
from journey import check, verify_binding
from journey_persistent import graceful_stop, migration_ledger, verified_migration_inventory
from journey_process import owned_child, private_write, require, reserved_ports, runtime_environment, wait_owned_ready
from journey_reliability import (Budget, MAX_OVERRUN, capture_batch, cleanup, database_counts,
                                 final_accounting, owned_database, reliability_policy)
from journey_reliability_defaults import ENDPOINT_OVERRIDES, WORKER_CONDITIONS, normal_default_config
from journey_reliability_normal_defaults import boot_snapshot, observe_defaults, validate_boot_defaults
from journey_reliability_normal_fixture import (normal_fixture, normal_services, synthetic_project,
                                                validate_normal_response)
from journey_reliability_normal_workers import (LESSONS_SQL, REGISTRATIONS_SQL, project_database, read_rows, setup_project,
                                                worker_api, worker_database)
from journey_reliability_observations import observe_workload
from journey_reliability_profiles import NORMAL_PROFILE, reliability_options
from recovery_database import EMPTY_SQL


def private_setup(root, endpoints, database, project):
    root.mkdir(mode=0o700)
    home, cwd = root / "home", root / "cwd"
    home.mkdir(mode=0o700)
    cwd.mkdir(mode=0o700)
    state = home / ".local" / "state" / "mike"
    state.mkdir(parents=True, mode=0o700)
    # Keep the host identity synthetic: load_or_create_in preserves this file
    # instead of deriving a machine name from the runner. Hash it with config.
    machine = state / "machine.json"
    private_write(machine, json.dumps({key: project[key] for key in
                                     ("machine_uuid", "machine_name")}).encode("ascii"))
    config, baseline = root / "config.toml", root / "defaults.toml"
    private_write(config, normal_default_config(endpoints, database))
    private_write(baseline, b"")
    return config, baseline, home, cwd, project, machine


def workload(child, config, ports, manifest, cluster, database, cohort, project, policy, report,
             services, protected, observations):
    """Private execution seam. Production admission and duration policy live in run_bound."""
    budget = Budget(policy["duration_seconds"] + MAX_OVERRUN)
    generation = None
    def ready():
        nonlocal generation
        health = wait_owned_ready(child, config, ports[1], manifest["source_commit"],
                                  timeout=min(10, budget.remaining()), persistent=True)
        observed = health["instance"]["boot_generation"]
        require(generation is None or generation == observed, "RELIABILITY_GENERATION_CHANGED")
        generation = observed
        require(health.get("persistence", {}).get("capture_stalled") is False,
                "RELIABILITY_CAPTURE_STALLED_OR_UNOBSERVED")
        require(all(sha256(path) == digest for path, digest in protected.items()),
                "NORMAL_DEFAULT_PRIVATE_FILES_CHANGED")
        observe_defaults(ports[1], health, cluster, database, budget, report)
        services.require_no_default_worker_calls()
        report["health_observations"] += 1
    ready()
    database_counts(cluster, database, 0, 0)
    require(read_rows(cluster, database, REGISTRATIONS_SQL) == []
            and read_rows(cluster, database, LESSONS_SQL) == [], "NORMAL_DEFAULT_WORKERS_NOT_EMPTY")
    seen, tools, expected_lessons = {}, {}, {}
    digest = hashlib.sha256()
    started = budget.now()
    while True:
        batch = report["completed_batches"]
        require(batch < policy["max_batches"] and len(seen) + len(cohort.sessions) <= policy["max_requests"],
                "RELIABILITY_REQUEST_LIMIT")
        ready()
        fixtures = cohort.prepare(batch)
        report["attempted_requests"] += len(fixtures)
        from journey_reliability_fixture import run_batch
        observed = run_batch(ports[0], cohort, fixtures,
                             observe_active=lambda deadline: observations.active(len(fixtures), deadline=deadline))
        require(observed["completed"] == len(fixtures) and observed["concurrent_sessions"] == len(fixtures),
                "RELIABILITY_BATCH_ACCOUNTING")
        report["completed_client_requests"] += observed["completed"]
        report["peak_concurrent_sessions"] = max(report["peak_concurrent_sessions"], observed["concurrent_sessions"])
        report["minimum_batch_concurrency"] = (observed["concurrent_sessions"] if batch == 0 else
                                                min(report["minimum_batch_concurrency"], observed["concurrent_sessions"]))
        report["synthetic_request_bytes"] += observed["request_bytes"]
        report["synthetic_response_bytes"] += observed["response_bytes"]
        captured = capture_batch(ports[1], fixtures, batch, seen, budget, response_validator=validate_normal_response)
        for fixture, row in zip(fixtures, captured, strict=True):
            tools[row["request_id"]] = row["tool_use_id"]
            expected_lessons[row["request_id"]] = {"session_id": fixture.session,
                "topic": fixture.lesson_topic, "text": fixture.lesson_text}
            digest.update(json.dumps(row, sort_keys=True, separators=(",", ":")).encode() + b"\n")
        database_counts(cluster, database, len(seen), len(cohort.sessions))
        report["worker_observations"] = worker_database(cluster, database, cohort.sessions, expected_lessons, project, budget)
        report["worker_observations"].update(worker_api(ports[1], fixtures, project, cohort.sessions, budget))
        observations.drain()
        report["durably_accounted_requests"] = len(seen)
        report["completed_batches"] += 1
        ready()
        elapsed = budget.now() - started
        report["duration_seconds"] = elapsed
        if elapsed >= policy["duration_seconds"]:
            break
        time.sleep(min(policy["interval_seconds"], policy["duration_seconds"] - elapsed))
    report.update(final_accounting(cluster, database, seen, tools))
    ready()
    elapsed = budget.now() - started
    require(elapsed >= policy["duration_seconds"] and report["attempted_requests"] == len(seen)
            and report["completed_client_requests"] == len(seen), "RELIABILITY_INCOMPLETE_WORKLOAD")
    report.update(duration_seconds=elapsed, accounting_sha256=digest.hexdigest(),
                  duration_policy_met=elapsed >= policy["minimum_duration_seconds"],
                  concurrency_policy_met=report["minimum_batch_concurrency"] >= 1,
                  workload_state="PASSED")
    report["worker_exercise"] = {"workspace_capture_registration": "PASSED", "harvester": "PASSED",
        "fleet_auto_start": "NOT_RUN", "prompt_assistant": "NOT_RUN",
        "background_worker_internal_cycles": "NOT_RUN", "embedding_consolidation": "NOT_RUN"}
    report["unmeasured"].pop("normal_default_reliability", None)
    return seen, tools, expected_lessons


def run_bound(paths, manifest, sandbox, result, cluster, inventory, policy, *, isolation):
    # No caller-supplied dictionary or environment marker grants admission.
    # The outer adapter separately validates real Docker state and evidence.
    from journey_reliability_container import require_admission
    require_admission(isolation)
    require(policy == reliability_policy(policy["duration_seconds"], policy["concurrent_sessions"]),
            "NORMAL_DEFAULT_DURATION_POLICY")
    report = result["reliability"]
    reliability_options(NORMAL_PROFILE, report["resource_budgets"])
    require(report["profile"] == NORMAL_PROFILE, "NORMAL_DEFAULT_PROFILE")
    require(report["resource_budgets"]["active_streams_peak"] >= policy["concurrent_sessions"],
            "RELIABILITY_ACTIVE_STREAMS_BUDGET_BELOW_WORKLOAD")
    report.update(override_names=list(ENDPOINT_OVERRIDES), requested_disabled_feature_keys=[],
        feature_isolation_scope="compiled_defaults_with_owned_endpoint_relocations",
        setup_workers_may_start_against_empty_database=True,
        background_worker_conditions=dict(WORKER_CONDITIONS), inner_isolation_verified=False,
        isolation_evidence_owner="outer_container_adapter")
    database = cluster.create(seeded=False)
    owned_database(cluster, database)
    require(cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0",
            "RELIABILITY_DATABASE_NOT_EMPTY")
    root = Path(sandbox)
    stack = ExitStack()
    try:
        ports, reservations = stack.enter_context(reserved_ports())
        services, service_port = stack.enter_context(normal_services())
        endpoints = dict(zip(("proxy", "inspect", "spa"), ports, strict=True))
        endpoints.update(services=service_port, postgres=cluster.url.port)
        # Setup requires no server; choose actual upstream before writing config.
        root.mkdir(mode=0o700)
        project = synthetic_project(root)
        cohort, upstream_port = stack.enter_context(normal_fixture(policy["concurrent_sessions"], project))
        endpoints["upstream"] = upstream_port
        config, baseline, home, cwd, project, machine = private_setup(root / "runtime", endpoints, database, project)
        protected = {path: sha256(path) for path in (config, baseline, machine)}
        initial = boot_snapshot(paths["mike"], baseline, home, cwd)
        current, binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
        require(current == paths and binding == result["binding"], "RELIABILITY_BINDING_CHANGED")
        require(verified_migration_inventory(paths, manifest) == inventory, "RELIABILITY_INVENTORY_CHANGED")
        for reservation in reservations:
            reservation.close()
        children = ExitStack()
        try:
            child = children.enter_context(owned_child([str(paths["mike"]), "--config", str(config), "serve"],
                                                       cwd, runtime_environment(home)))
            health = wait_owned_ready(child, config, ports[1], manifest["source_commit"], timeout=240, persistent=True)
            report["boot_defaults"] = check(result, "archive_normal_default_boot", lambda: validate_boot_defaults(
                boot_snapshot(paths["mike"], config, home, cwd), initial, config, baseline, endpoints, database))
            observe_defaults(ports[1], health, cluster, database, Budget(30), report)
            before = check(result, "archive_normal_default_ledger", lambda: migration_ledger(cluster, database, inventory))
            check(result, "archive_normal_default_project_setup", lambda: setup_project(ports[1], project, Budget(30)))
            check(result, "archive_normal_default_project_durable", lambda: project_database(cluster, database, project))
            def measured():
                with observe_workload(child, paths, manifest, ports, home, cwd, report) as observations:
                    require(observations is not None, "NORMAL_DEFAULT_OBSERVATIONS_REQUIRED")
                    evidence = workload(child, config, ports, manifest, cluster, database, cohort, project,
                                        policy, report, services, protected, observations)
                    report["baseline_comparison"]["normal_defaults_control"] = "same_owned_process_idle"
                    return evidence
            seen, tools, lessons = check(result, "archive_normal_default_workload", measured)
            # Replace private per-request return data in the retained check with
            # compact aggregate observations; exact identities remain hash-bound.
            result["checks"][-1]["observation"] = {"requests": len(seen), "harvested_lessons": len(lessons)}
            check(result, "archive_normal_default_owned_stop", lambda: graceful_stop(child))
            report["post_stop_worker_observations"] = worker_database(cluster, database, cohort.sessions, lessons, project, Budget(30))
            require(report["post_stop_worker_observations"]["worker_rows_sha256"]
                    == report["worker_observations"]["worker_rows_sha256"], "NORMAL_DEFAULT_WORKER_DURABILITY_CHANGED")
            final_accounting(cluster, database, seen, tools)
            require(migration_ledger(cluster, database, inventory) == before, "RELIABILITY_LEDGER_CHANGED")
            require(all(sha256(path) == digest for path, digest in protected.items()), "NORMAL_DEFAULT_PRIVATE_FILES_CHANGED")
            current, final_binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
            require(current == paths and final_binding == binding, "RELIABILITY_BINDING_CHANGED")
            validate_boot_defaults(boot_snapshot(paths["mike"], config, home, cwd), initial,
                                   config, baseline, endpoints, database)
            services.require_no_default_worker_calls()
            report["service_observations"] = services.counts()
        finally:
            check(result, "archive_normal_default_process_cleanup", lambda: cleanup(children, "owned_process_group_only"))
    finally:
        check(result, "archive_normal_default_fixture_cleanup", lambda: cleanup(stack, "owned_loopback_fixture_only"))
    return {"independent_owned_database": True, "normal_defaults_observed": True,
            "durable_synthetic_worker_effects": True, "qualification": "NOT_RUN"}
