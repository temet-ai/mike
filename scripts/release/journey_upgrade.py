"""Opt-in two-version populated-data upgrade/removal helper, never qualification.

The caller supplies an open verified candidate snapshot and an explicit frozen
predecessor archive directory. No predecessor is inferred, rebuilt or downloaded.
Only these exact executables migrate the newly allocated disposable database.
"""
import os
from pathlib import Path

from common import sha256
from inputs import verified_inputs
from journey import check, create_config, new_result
from journey_maintenance import InstalledKit
from journey_persistent import migration_ledger, owned_cluster
from journey_process import JourneyError, installation_identity, private_write, require, reserved_ports
from journey_reinstall import registration_roundtrip, retained_reinstall
from journey_reliability import isolated_config
from journey_reliability_fixture import reliability_fixture
from journey_upgrade_runtime import Runtime, incompatible_refusal, predecessor_backup, read_ledger
from journey_upgrade_state import Archive, compatible_pair, durable_snapshot, interrupted_install, refuse_install, tree_digest
from recovery_database import EMPTY_SQL
from recovery_files import plain_path

LIMITS = [
    "Requires two explicitly selected, trusted, frozen native archive pairs with distinct source commits and increasing SemVer.",
    "The candidate migration inventory must strictly extend the predecessor's unchanged migration prefix; no fake schema or ledger edits.",
    "Uses a fresh owned TCP-only PostgreSQL 19 database, private prefix/HOME/config and synthetic capture only; no existing installation is adopted.",
    "The predecessor must include the installed maintenance kit and both versions must support this helper's capture and feature APIs.",
    "Installer SIGKILL is injected immediately before and after the atomic paired-generation swap; migration-in-progress interruption remains NOT_RUN.",
    "Uses synthetic isolation overrides and supported feature disables, not normal-default or native-account qualification.",
    "Downgrade mutation evidence covers full logical PostgreSQL dump and durable config/data/state; lifecycle files and diagnostic logs are excluded.",
    "Retained-state uninstall/reinstall and synthetic MCP registration removal do not qualify native removal, purge, service/autostart or real client integration.",
    "Owned foreground SIGTERM/serve restarts are not installed CLI/TUI lifecycle qualification; no real provider, browser or published channel.",
    "Private HOME and empty runtime PATH are not an OS sandbox; no unpublished source guard is assumed to exist in either artifact.",
    "All AC01-AC27 remain required NOT_RUN. Passing partial AC23/AC24 subchecks never qualifies a case or release.",
]


def apply_with_interruptions(runtime, previous, candidate, roots, service, root):
    before = durable_snapshot(runtime.cluster, runtime.database, service, roots)
    for phase, selected in (("before_swap", previous), ("after_swap", candidate)):
        runtime.check(phase, "installer_sigkill", lambda: interrupted_install(
            candidate, previous.prefix, phase, root, runtime.home, runtime.cwd))
        if phase == "after_swap":
            candidate.bind(previous.prefix)
        runtime.check(phase, "selected_pair", selected.recheck)
        def unchanged():
            require(durable_snapshot(runtime.cluster, runtime.database, service, roots) == before,
                    "UPGRADE_INSTALL_INTERRUPTION_CHANGED_DATA")
            return {"database_and_durable_config_data_state_unchanged": True,
                    "predecessor_selected": phase == "before_swap", "candidate_selected": phase == "after_swap"}
        runtime.check(phase, "durable_state", unchanged)
    receipt = runtime.check("retry", "exact_install", lambda: candidate.select(previous.prefix))
    require(receipt["status"] == "unchanged", "UPGRADE_FINAL_RETRY_NOT_IDEMPOTENT")
    require(durable_snapshot(runtime.cluster, runtime.database, service, roots) == before,
            "UPGRADE_FINAL_RETRY_CHANGED_DATA")


def run_bound(previous, candidate, root, result, cluster):
    database = cluster.create(seeded=False)
    require(database in cluster.created and cluster.sql(database, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0",
            "UPGRADE_REQUIRES_NEW_EMPTY_OWNED_DATABASE")
    check(result, "archive_upgrade_empty_database", lambda: {"fresh_owned_database": True, "synthetic_schema_seed_used": False})
    with reliability_fixture(1) as (cohort, upstream_port), reserved_ports() as (ports, reservations):
        config, home, cwd = create_config(root / "r", ports, upstream_port, database_url=cluster.config_url(database))
        isolated_config(config, upstream_port, {"max_requests": 2, "concurrent_sessions": 1})
        _, record = installation_identity(config)
        record.parent.parent.mkdir(mode=0o700)
        data = record.parent.parent / "data"
        data.mkdir(mode=0o700)
        private_write(data / "synthetic-retained-file.txt", b"Owned synthetic upgrade/removal sentinel.\n")
        protected = {path: sha256(path) for path in (config, home / ".env", cwd / ".env")}
        runtime = Runtime(result, cluster, database, config, home, cwd, ports, upstream_port, cohort)
        for reservation in reservations:
            reservation.close()
        runtime.serve(previous, "predecessor_isolation", isolate=True)
        runtime.serve(previous, "predecessor_capture", capture=True)
        old_ledger = read_ledger(cluster, database)
        service = cluster.service(root / "owned.service", database)
        bundle = root / "backup"
        roots = None
        def backup():
            nonlocal roots
            roots, observation = predecessor_backup(runtime, previous, service, bundle)
            return observation
        check(result, "archive_upgrade_predecessor_backup", backup)
        backup_digest = tree_digest(bundle)
        apply_with_interruptions(runtime, previous, candidate, roots, service, root)
        runtime.serve(candidate, "candidate_capture", capture=True, while_ready=lambda: runtime.check(
            "candidate", "synthetic_registration_removal", lambda: registration_roundtrip(runtime, candidate)))
        current_ledger = read_ledger(cluster, database)
        def ledger_extension():
            require(current_ledger[:len(old_ledger)] == old_ledger and len(current_ledger) > len(old_ledger),
                    "UPGRADE_REWROTE_PREDECESSOR_MIGRATION_HISTORY")
            migration_ledger(cluster, database, candidate.inventory)
            return {"full_predecessor_ledger_rows_unchanged": True, "only_candidate_extension_applied": True}
        check(result, "archive_upgrade_preserved_migration_history", ledger_extension)
        check(result, "archive_upgrade_installer_downgrade_refusal", lambda: refuse_install(previous, candidate))
        check(result, "archive_upgrade_runtime_incompatible_predecessor_refusal", lambda: incompatible_refusal(
            runtime, previous, candidate, roots, service))
        check(result, "archive_upgrade_retained_removal_reinstall", lambda: retained_reinstall(
            runtime, candidate, roots, service, bundle))
        runtime.serve(candidate, "reinstalled_continuity")
        def final_state():
            require(read_ledger(cluster, database) == current_ledger, "UPGRADE_FINAL_LEDGER_CHANGED")
            require(tree_digest(bundle) == backup_digest, "UPGRADE_PREDECESSOR_BACKUP_CHANGED")
            require(all(sha256(path) == digest for path, digest in protected.items()), "UPGRADE_CONFIG_OR_POISON_CHANGED")
            candidate.recheck()
            previous.recheck(active=False)
            candidate.snapshot_binding()
            previous.snapshot_binding()
            return {"two_durable_capture_ids_and_body_hashes_survived": True,
                    "exact_old_and_new_archives_retained": True, "backup_and_selected_config_unchanged": True}
        check(result, "archive_upgrade_final_retained_state", final_state)


def run_upgrade_journey(snapshot, manifest, sandbox, manifest_sha256, predecessor):
    """Explicit opt-in; caller owns private sandbox, this helper owns DB cleanup."""
    result = new_result(None)
    result.update(runner="archive-two-version-upgrade-helper", limits=list(LIMITS))
    cluster = None
    try:
        root = Path(sandbox) / "u"
        plain_path(root.parent, directory=True, private=True)
        _, record = installation_identity(root / "r/config.toml")
        require(len(os.fsencode(record.parent / "control.sock")) <= 103, "UPGRADE_CONTROL_PATH_TOO_LONG")
        root.mkdir(mode=0o700)
        candidate = Archive(snapshot, manifest, manifest_sha256)
        with verified_inputs(Path(predecessor)) as (old_snapshot, old_manifest):
            previous = Archive(old_snapshot, old_manifest, sha256(old_snapshot / "manifest.json"))
            check(result, "archive_upgrade_real_predecessor_pair", lambda: compatible_pair(old_manifest, manifest))
            # Verify both executable source identities before connecting to a DB.
            candidate.select(root / "v")
            result["binding"] = candidate.binding
            previous.select(root / "p")
            result["predecessor_binding"] = previous.binding
            check(result, "archive_upgrade_both_exact_archives", lambda: {
                "candidate": candidate.binding, "predecessor": previous.binding})
            def maintenance_kits():
                for archive in (previous, candidate):
                    InstalledKit(archive.paths, archive.manifest, archive.binding, archive.prefix,
                                 archive.inventory, root, root).recheck()
                return {"both_complete_installed_kits_hash_bound_before_database_use": True}
            check(result, "archive_upgrade_installed_maintenance_kits", maintenance_kits)
            cluster = owned_cluster()
            check(result, "archive_upgrade_owned_fixture", lambda: {"postgres_major": 19,
                "owned_private_pgdata": True, "password_free_loopback_admin": True, "tcp_only": True})
            try:
                check(result, "archive_upgrade_populated_two_version_run", lambda: run_bound(previous, candidate, root, result, cluster))
            finally:
                def cleanup():
                    cluster.cleanup()
                    return {"only_databases_created_by_this_run": True, "cluster_left_running": True}
                check(result, "archive_upgrade_database_cleanup", cleanup)
    except Exception as error:
        result.update(state="FAILED", failure_type=type(error).__name__)
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    return result
