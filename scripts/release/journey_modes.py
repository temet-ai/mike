"""Opt-in installed setup trial → PostgreSQL → trial → PostgreSQL helpers.

Actual CLI/TUI commands change the selected file and reconcile its daemon. The
helper compares requested, saved and live modes and durable capture identities.
It never promotes AC13 (or any full case); Docker/native/manual UX remain open.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import shutil
import tempfile
import tomllib
import uuid

from common import sha256
from journey import check, create_config, doctor, new_result, verify_binding
from journey_fixture import SECRET, upstream_fixture
from journey_http import inspect_capture, stream_capture
from journey_modes_tui import tui_restart, tui_setup
from journey_persistent import (inventory_digest, migration_ledger, owned_cluster,
                                stored_capture, verified_migration_inventory)
from journey_process import JourneyError, private_write, require, reserved_ports, runtime_environment
from journey_reliability import Budget, isolated_config
from journey_reliability_isolation import disable_features, verify_features, verify_service_prerequisites
from journey_selected import SelectedInstallation, private_read
from recovery_database import EMPTY_SQL
from owned_process import OwnedProcess

LIMITS = [
    "Explicit owned PostgreSQL 19 fixture; exact installed executable and migration hashes only.",
    "Detached setup daemons require Linux SO_PEERCRED and permitted AF_UNIX control; no denied-socket workaround.",
    "The PTY path is opt-in prompt/key automation, not visual or manual native-account TUI qualification.",
    "CLI restart uses exact-generation private cooperative stop followed by installed setup; no standalone CLI stop exists.",
    "Supported optional service URL overrides point at the synthetic endpoint; embedding stays disabled at its unowned compiled URL and the provider key is absent. These prerequisites and private HOME are not an OS sandbox.",
    "No managed Docker transition, native-account/logout/autostart, real provider or MCP registration.",
    "Degraded/outage/capture-gap doctor states remain NOT_RUN: this helper never disrupts a database server or injects disk faults.",
    "All full AC01-AC27 remain required NOT_RUN; helper subchecks never qualify AC13 or a release.",
]


LITERAL_INPUT = SECRET + "_$()_\"_'_\\_`"


def literal_database_url(url):
    return url + "&application_name=" + LITERAL_INPUT


def setup_command(binary, config, home, cwd, args, *, environment=None, timeout=330):
    env = runtime_environment(home) if environment is None else environment
    child = OwnedProcess([str(binary), "--config", str(config), "setup", *args], cwd=cwd, env=env,
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        try:
            output, diagnostic = child.communicate(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            raise JourneyError("MODE_SETUP_TIMEOUT") from error
        return child.returncode, output, diagnostic
    finally:
        child.close(grace=1)


def cli_setup(binary, config, home, cwd, database_file=None):
    before = sha256(config)
    mode = "existing" if database_file is not None else "trial"
    args = ["--mode", mode, "--accept-trial-loss", "--json"]
    if database_file is not None:
        args.extend(("--database-url-file", str(database_file)))
    code, raw, _ = setup_command(binary, config, home, cwd, args)
    require(code == 0, "MODE_SETUP_NONZERO")
    value = json.loads(raw)
    require(isinstance(value, dict) and set(value) == {
        "persistent", "config_file", "previous_config_backup", "verified_running"}, "MODE_SETUP_REPORT_SCHEMA")
    require(value["persistent"] is (database_file is not None) and value["verified_running"] is True
            and value["config_file"] == str(config), "MODE_SETUP_REPORTED_MODE")
    backup = Path(value["previous_config_backup"] or "")
    require(backup.parent == config.parent and backup.name.startswith(config.name + ".backup-"),
            "MODE_SETUP_BACKUP_PATH")
    require(hashlib.sha256(private_read(backup)).hexdigest() == before, "MODE_SETUP_BACKUP_CHANGED")
    return {"surface": "installed_setup_cli", "requested_mode": mode,
            "verified_running_reported": True, "previous_private_config_preserved": True,
            "exit_status": 0, "report_sha256": hashlib.sha256(raw).hexdigest()}


def refused_setup(binary, config, home, cwd, database_file=None):
    protected = [config, home / ".env", cwd / ".env"]
    before = {path: sha256(path) for path in protected}
    attempts = [("trial_loss_acknowledgement", b"Repeat with --accept-trial-loss",
                 ["--mode", "trial", "--json"], runtime_environment(home)),
                ("conflicting_environment_override", b"override the requested storage mode or URL", ["--mode", "trial", "--accept-trial-loss", "--json"],
                 dict(runtime_environment(home), MIKE_PERSIST_ENABLED="true"))]
    if database_file is not None:
        malformed = config.parent / "malformed-literal-url"
        private_write(malformed, ("invalid://" + LITERAL_INPUT + "\n").encode())
        protected.extend((database_file, malformed))
        before = {path: sha256(path) for path in protected}
        existing = ["--mode", "existing", "--accept-trial-loss", "--json", "--database-url-file", str(database_file)]
        for name, key, value in (("persistent_flag_override", "MIKE_PERSIST_ENABLED", "false"),
                                 ("fallback_override", "MIKE_DB_FALLBACK_ENABLED", "true"),
                                 ("database_url_override", "MIKE_DATABASE_URL", "postgres://fixture@127.0.0.1:1/unused")):
            attempts.append((name, b"override the requested storage mode or URL", existing,
                             dict(runtime_environment(home), **{key: value})))
        attempts.append(("malformed_literal_url", b"provide a postgres:// or postgresql:// URL",
                         existing[:-1] + [str(malformed)], runtime_environment(home)))
    for _, diagnostic, args, environment in attempts:
        code, output, error = setup_command(binary, config, home, cwd, args, environment=environment, timeout=15)
        require(code == 1 and not output.strip() and diagnostic in error, "MODE_EXPECTED_REFUSAL_MISSING")
        require(SECRET.encode() not in output + error and LITERAL_INPUT.encode() not in output + error,
                "MODE_REFUSAL_LITERAL_LEAK")
        require(all(sha256(path) == digest for path, digest in before.items()), "MODE_REFUSAL_CHANGED_FILES")
    return {"refused_before_change": [name for name, _, _, _ in attempts], "protected_files_unchanged": True}


def saved_mode(config, persistent, database_url=None):
    value = tomllib.loads(private_read(config).decode())
    require(value.get("persist_enabled") is persistent, "MODE_SAVED_MODE_MISMATCH")
    if database_url is not None:
        require(value.get("database_url") == database_url, "MODE_SAVED_DATABASE_CHANGED")
    if persistent:
        require(value.get("db_fallback_enabled") is False, "MODE_FALLBACK_NOT_DISABLED")
    return {"saved_persistent": persistent, "private_config": True,
            "durable_database_url_retained": database_url is not None,
            "saved_config_sha256": sha256(config),
            "database_url_sha256": hashlib.sha256(database_url.encode()).hexdigest() if database_url is not None else None}


def capture_continuity(observed, expected, ledger, expected_ledger):
    require(observed == expected, "MODE_DURABLE_CAPTURE_CHANGED")
    require(ledger == expected_ledger, "MODE_DURABLE_LEDGER_CHANGED")
    return {"same_request_id_session_and_body_hashes": True, "full_ledger_unchanged": True,
            "no_duplicate_capture": True, "capture": observed}


def rearm_fixture(fixture):
    fixture.session = str(uuid.uuid4())
    for event in (fixture.release, fixture.first_sent, fixture.completed):
        event.clear()
    fixture.received = fixture.failure = None


def transition(context, persistent, phase, database_known):
    paths, manifest, result = (context[key] for key in ("paths", "manifest", "result"))
    config, home, cwd = (context[key] for key in ("config", "home", "cwd"))
    identifier = "archive_mode_" + context["surface"] + "_" + phase
    def bound():
        current_paths, binding = verify_binding(paths["mike"].parent, manifest, result["binding"]["manifest_sha256"])
        require(current_paths == paths and binding == result["binding"], "MODE_INSTALLED_BINDING_CHANGED")
        return {"exact_installed_hashes": True}
    check(result, identifier + "_binding", bound)
    if context["surface"] == "tui":
        action = lambda: tui_setup(paths["mike"], config, home, cwd, context["url"] if persistent else None)
    else:
        action = lambda: cli_setup(paths["mike"], config, home, cwd, context["url_file"] if persistent else None)
    context["selected"].begin_launch()
    check(result, identifier + "_setup", action)
    saved = check(result, identifier + "_saved", lambda: saved_mode(
        config, persistent, context["url"] if database_known else None))
    def ready():
        health = context["selected"].ready(persistent, timeout=250)
        verify_service_prerequisites(health)
        generation = health["instance"]["boot_generation"]
        require(generation not in context["generations"], "MODE_GENERATION_REUSED")
        context["generations"].append(generation)
        return dict(saved, requested_persistent=persistent, live_persistent=persistent,
                    unique_boot_generation=True, requested_saved_runtime_agree=True)
    check(result, identifier + "_runtime", ready)
    check(result, identifier + "_doctor", lambda: doctor(paths["mike"], config, home, cwd, manifest, persistent=persistent))


def run_sequence(context, cluster, database, inventory, fixture):
    result, ports = context["result"], context["ports"]
    label = "archive_mode_" + context["surface"]
    check(result, label + "_stopped_trial_doctor", lambda: stopped_doctor(context, False))
    transition(context, False, "initial_trial", False)
    check(result, label + "_ephemeral_capture", lambda: stream_capture(ports[0], fixture))
    check(result, label + "_ephemeral_inspect", lambda: inspect_capture(ports[1], fixture)[1])
    transition(context, True, "first_database", True)
    check(result, label + "_trial_not_imported", lambda: empty_capture(ports[1]))
    ledger = check(result, label + "_ledger", lambda: migration_ledger(cluster, database, inventory))
    check(result, label + "_disable_optional_features", lambda: disable_features(ports[1], Budget(180)))
    check(result, label + "_isolation_stop", context["selected"].stop)
    check(result, label + "_stopped_database_doctor", lambda: stopped_doctor(context, True))
    transition(context, True, "database_isolation_restart", True)
    check(result, label + "_isolated_features", lambda: verify_features(ports[1], Budget(10)))
    rearm_fixture(fixture)
    check(result, label + "_durable_capture", lambda: stream_capture(ports[0], fixture))
    before = check(result, label + "_durable_bodies", lambda: stored_capture(ports[1], fixture))
    transition(context, False, "return_trial", True)
    check(result, label + "_database_history_not_trial", lambda: empty_capture(ports[1]))
    require(migration_ledger(cluster, database, inventory) == ledger, "MODE_TRIAL_CHANGED_DURABLE_LEDGER")
    transition(context, True, "return_database", True)
    def continuity():
        return capture_continuity(stored_capture(ports[1], fixture, expected_id=before["request_id"]), before,
                                  migration_ledger(cluster, database, inventory), ledger)
    check(result, label + "_return_continuity", continuity)
    context["cwd"] = context["second_cwd"]
    if context["surface"] == "tui":
        def installed_restart():
            evidence, health = tui_restart(context["paths"]["mike"], context["config"], context["home"],
                                          context["cwd"], context["selected"], True)
            generation = health["instance"]["boot_generation"]
            require(generation not in context["generations"], "MODE_GENERATION_REUSED")
            context["generations"].append(generation)
            saved_mode(context["config"], True, context["url"])
            verify_service_prerequisites(health)
            return dict(evidence, requested_saved_runtime_agree=True)
        check(result, label + "_installed_stop_start", installed_restart)
    else:
        check(result, label + "_private_restart_stop", context["selected"].stop)
        transition(context, True, "database_restart", True)
    check(result, label + "_restart_features", lambda: verify_features(ports[1], Budget(10)))
    check(result, label + "_restart_continuity", continuity)
    return {"sequence": ["trial", "database", "trial", "database", "database_restart"],
            "distinct_boot_generations": len(context["generations"]), "restart_from_different_cwd": True,
            "additional_optional_feature_isolation_restart": True}


def empty_capture(port):
    require(http_requests(port) == [], "MODE_UNEXPECTED_CAPTURE")
    return {"request_count": 0, "no_capture_imported_between_modes": True}


def http_requests(port):
    from journey_process import http_json
    return http_json(port, "/api/requests")


def run_bound(paths, manifest, root, result, cluster, inventory, *, surface):
    database = cluster.create(seeded=False)
    require(cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0", "MODE_DATABASE_NOT_EMPTY")
    with upstream_fixture() as (fixture, upstream_port), reserved_ports() as (ports, reservations):
        config, home, cwd = create_config(root, ports, upstream_port)
        isolated_config(config, upstream_port, {"max_requests": 10, "concurrent_sessions": 1})
        url = literal_database_url(cluster.config_url(database))
        url_file = root / "database-url"
        private_write(url_file, (url + "\n").encode())
        second_cwd = root / "second cwd"
        second_cwd.mkdir(mode=0o700)
        private_write(second_cwd / ".env", (cwd / ".env").read_bytes())
        protected = [home / ".env", cwd / ".env", second_cwd / ".env", url_file]
        before = {path: sha256(path) for path in protected}
        selected = SelectedInstallation(config, ports[1], manifest["source_commit"])
        context = dict(paths=paths, manifest=manifest, result=result, config=config, home=home, cwd=cwd,
                       second_cwd=second_cwd, selected=selected, url=url, url_file=url_file,
                       surface=surface, ports=ports, upstream_port=upstream_port, generations=[])
        try:
            check(result, "archive_mode_" + surface + "_refusals", lambda: refused_setup(paths["mike"], config, home, cwd, url_file))
            require(not selected.root.exists(), "MODE_REFUSAL_STARTED_DAEMON")
            for reservation in reservations:
                reservation.close()
            evidence = run_sequence(context, cluster, database, inventory, fixture)
            require(all(sha256(path) == digest for path, digest in before.items()), "MODE_PROTECTED_FILES_CHANGED")
            return dict(evidence, protected_files_unchanged=True)
        finally:
            try:
                check(result, "archive_mode_" + surface + "_owned_cleanup", selected.stop)
            except BaseException:
                result.setdefault("retained_private_fixtures", []).append({
                    "surface": surface, "path": str(root.parent),
                    "reason": "Cooperative cleanup did not verify; control state was preserved, no PID was signalled."})
                raise


def run_mode_journey(binary_dir, manifest, sandbox, manifest_sha256, tui=False):
    result = new_result(None)
    result.update(runner="archive-installed-mode-helper", limits=list(LIMITS))
    try:
        paths, result["binding"] = verify_binding(binary_dir, manifest, manifest_sha256)
        inventory = verified_migration_inventory(paths, manifest)
        check(result, "archive_mode_candidate_binding", lambda: {
            "exact_installed_hashes": True, "executable_source_identity": True,
            "candidate_inventory_sha256": inventory_digest(inventory)})
        cluster = owned_cluster()
        try:
            for surface in (["cli", "tui"] if tui else ["cli"]):
                # Short selected paths preserve the portable control-socket bound.
                check(result, "archive_mode_" + surface + "_sequence", lambda: run_isolated(
                    paths, manifest, result, cluster, inventory, surface=surface))
            if not tui:
                result["checks"].append({"id": "archive_mode_tui_sequence", "state": "NOT_RUN",
                    "binding": result["binding"], "reason": "Requires explicit --mode-tui-journey."})
        finally:
            check(result, "archive_mode_database_cleanup", lambda: cleanup_database(cluster))
    except Exception as error:
        result.update(state="FAILED", failure_type=type(error).__name__)
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    return result


def cleanup_database(cluster):
    cluster.cleanup()
    return {"only_databases_created_by_this_run": True, "cluster_left_running": True}


def run_isolated(paths, manifest, result, cluster, inventory, *, surface):
    # Detached daemon control state must outlive the caller's temporary sandbox
    # if cooperative stop cannot be verified. Never rename the selected path.
    root = Path(tempfile.mkdtemp(prefix="mike-mode-", dir="/tmp"))
    try:
        return run_bound(paths, manifest, root / "m", result, cluster, inventory, surface=surface)
    finally:
        retained = result.get("retained_private_fixtures", [])
        if not any(item["path"] == str(root) for item in retained):
            shutil.rmtree(root)


def stopped_doctor(context, persistent):
    require(not context["selected"].held(), "MODE_STOPPED_DOCTOR_HAS_OWNER")
    observation = doctor(context["paths"]["mike"], context["config"], context["home"], context["cwd"],
                         context["manifest"], persistent=persistent, stopped=True)
    require(not context["selected"].held(), "MODE_DOCTOR_STARTED_DAEMON")
    return observation
