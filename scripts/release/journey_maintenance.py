"""Optional installed-kit roundtrip of the persistent journey's disposable capture.

Call only after both source owned_child contexts have exited and while the source
upstream fixture remains alive. The caller owns the Cluster and sandbox cleanup,
including any newly allocated target on failure. No existing database is adopted,
no cluster/service is started, and no complete acceptance case is qualified.
"""
import json
import os
from pathlib import Path
import stat
import sys
import tomllib
from urllib.parse import urlsplit

from archive import SUPPORT_FILES
from common import sha256
from journey import check, verify_binding
from journey_mcp import run_mcp
from journey_persistent import (graceful_stop, migration_ledger, stored_capture,
                                verified_migration_inventory)
from journey_process import (captured_run, installation_identity, owned_child,
                             private_write, require, runtime_environment, wait_owned_ready)
from recovery import installed, selected_roots, verify
from recovery_database import EMPTY_SQL
from recovery_files import inventory as file_inventory, plain_path
from recovery_tools import selected_tools

LIMITS = [
    "Opt-in installed maintenance kit against the already owned PG19 fixture only; no host services or real data.",
    "Backup/verify/fresh restore and inspect/MCP continuity are partial AC22 helpers only; AC22/AC23/AC24 remain NOT_RUN.",
    "Only restored fixture config/paths are rebound after byte-exact offline verification; no general config migration is implemented.",
    "No upgrade, downgrade, interruption, uninstall/reinstall, browser, native-account or managed-Docker qualification.",
    "Installed maintenance needs its documented Bash/Python/PG19 tools; verifier-only relocated library overrides are not inherited.",
]
CONFIG_KEYS = {"persist_enabled", "proxy_listen_addr", "inspect_listen_addr", "spa_bind",
               "upstream_url", "database_url"}
MCP_KEYS = {"protocol", "profile", "tool_count", "read_tool", "matching_capture"}


class InstalledKit:
    """Recheck the active installation, complete kit and runtime before every use."""
    def __init__(self, paths, manifest, binding, prefix, candidate_inventory, home, cwd):
        self.paths, self.manifest, self.binding = paths, manifest, binding
        self.prefix = Path(prefix)
        self.inventory, self.home, self.cwd = candidate_inventory, home, cwd
        self.tool_directory = os.environ.get("MIKE_RECOVERY_TEST_TOOL_DIR") or None
        if self.tool_directory is not None:
            selected_tools(self.tool_directory, ("psql", "pg_dump", "pg_restore"))

    def recheck(self):
        prefix, state, identity, manifest = installed(self.prefix)
        require(prefix == self.prefix and identity == self.binding["manifest_sha256"]
                and manifest == self.manifest, "MAINTENANCE_INSTALLATION_CHANGED")
        paths, binding = verify_binding(prefix / "bin", manifest, identity)
        require(paths == self.paths and binding == self.binding,
                "MAINTENANCE_RUNTIME_BINDING_CHANGED")
        require(verified_migration_inventory(paths, manifest) == self.inventory,
                "MAINTENANCE_CANDIDATE_INVENTORY_CHANGED")
        generation = state / "generations" / identity
        require(all(path.parent == generation for path in paths.values()),
                "MAINTENANCE_GENERATION_MISMATCH")
        artifact = next(item for item in manifest["artifacts"] if item["binary"] == "mike")
        digests = {}
        for name, mode in SUPPORT_FILES.items():
            entries = [entry for entry in artifact["contents"] if entry["path"] == name]
            require(len(entries) == 1, "MAINTENANCE_KIT_NOT_IN_MANIFEST")
            path = plain_path(generation / name)
            info = path.stat()
            require(info.st_nlink == 1 and stat.S_IMODE(info.st_mode) == mode
                    and info.st_size == entries[0]["size"] and sha256(path) == entries[0]["sha256"],
                    "MAINTENANCE_KIT_HASH_MISMATCH")
            digests[name] = entries[0]["sha256"]
        self.launcher = generation / "mike-maintenance"
        return {"exact_installed_runtime_and_kit": True, "kit_sha256": digests}

    def command(self, operation, arguments):
        self.recheck()
        environment = runtime_environment(self.home)
        # This PATH is for optional maintenance prerequisites, never mike serve.
        environment["PATH"] = str(Path(sys.executable).resolve().parent) + ":/usr/bin:/bin"
        options = []
        if operation != "verify":
            options = ["--prefix", str(self.prefix), "--confirm-stopped-writers"]
            if self.tool_directory is not None:
                options.extend(["--tool-directory", self.tool_directory])
        raw = captured_run([str(self.launcher), operation, *options, *map(str, arguments)],
                           self.cwd, environment, timeout=360)
        value = json.loads(raw)
        statuses = {"backup": "backup-complete", "verify": "verified-checksums",
                    "restore": "restored-offline-review-required"}
        require(isinstance(value, dict) and set(value) == {"created_utc", "version", "source_commit",
                "postgres_major", "files", "bytes", "status", "qualified"}
                and value["status"] == statuses[operation] and value["qualified"] is False
                and value["version"] == self.manifest["version"]
                and value["source_commit"] == self.manifest["source_commit"]
                and type(value["postgres_major"]) is int and value["postgres_major"] == 19
                and all(type(value[key]) is int and value[key] >= 0 for key in ("files", "bytes")),
                "MAINTENANCE_RECEIPT_INVALID")
        # Never retain arbitrary candidate output, timestamps, paths or DB names.
        return {"status": statuses[operation], "qualified": False, "installed_kit": True,
                "files": value["files"], "bytes": value["bytes"]}


def runtime_roots(kit, config, boundary):
    kit.recheck()
    raw = captured_run([str(kit.paths["mike"]), "--config", str(config), "print-config", "--paths"],
                       kit.cwd, runtime_environment(kit.home))
    paths = json.loads(raw)
    identity, record = installation_identity(config)
    require(isinstance(paths, dict) and set(paths) == {
        "config_file", "installation_id", "data_dir", "state_dir", "log_dir"}
        and paths["config_file"] == str(config) and paths["installation_id"] == identity
        and paths["state_dir"] == str(record.parent), "MAINTENANCE_RUNTIME_PATHS_INVALID")
    for key in ("data_dir", "state_dir", "log_dir"):
        require(isinstance(paths[key], str) and Path(paths[key]).is_absolute()
                and boundary in Path(paths[key]).parents
                and Path(paths[key]).resolve() == Path(paths[key]), "MAINTENANCE_ROOT_OUTSIDE_FIXTURE")
    return selected_roots(config, paths["data_dir"], paths["state_dir"])


def source_contract(source, result, cluster, root):
    require(source["database"] in cluster.created, "MAINTENANCE_SOURCE_NOT_OWNED")
    require(len(source["generations"]) == 2 and len(set(source["generations"])) == 2,
            "MAINTENANCE_SOURCE_RESTART_REQUIRED")
    passed = {entry["id"] for entry in result["checks"] if entry["state"] == "PASSED"}
    require({"archive_persistent_initial_owned_stop", "archive_persistent_restarted_owned_stop"} <= passed,
            "MAINTENANCE_SOURCE_NOT_STOPPED")
    config = plain_path(source["config"], private=True)
    require(config.parent.parent == root.parent and config.parent != root,
            "MAINTENANCE_SOURCE_OUTSIDE_SANDBOX")
    for key in ("home", "cwd"):
        path = plain_path(source[key], directory=True, private=True)
        require(config.parent in path.parents, "MAINTENANCE_SOURCE_OUTSIDE_SANDBOX")
    value = tomllib.loads(config.read_text())
    require(set(value) == CONFIG_KEYS and value["persist_enabled"] is True
            and value["database_url"] == cluster.config_url(source["database"]),
            "MAINTENANCE_SOURCE_CONFIG_INVALID")
    require(len(source["ports"]) == 3 and all(type(port) is int and 1024 <= port <= 65535
            for port in source["ports"]) and len(set(source["ports"])) == 3
            and [value[key] for key in ("proxy_listen_addr", "inspect_listen_addr", "spa_bind")]
                == ["127.0.0.1:" + str(port) for port in source["ports"]],
            "MAINTENANCE_SOURCE_LISTENERS_INVALID")
    upstream = urlsplit(value["upstream_url"])
    require(upstream.scheme == "http" and upstream.hostname == "127.0.0.1"
            and upstream.username is None and upstream.password is None
            and upstream.port is not None and 1024 <= upstream.port <= 65535
            and not any((upstream.path, upstream.query, upstream.fragment)),
            "MAINTENANCE_SOURCE_UPSTREAM_INVALID")
    require(source["capture"]["session_id"] == source["fixture"].session
            and source["capture"]["request_count"] == 1,
            "MAINTENANCE_SOURCE_CAPTURE_INVALID")
    require(isinstance(source["mcp"], dict) and MCP_KEYS <= set(source["mcp"])
            and source["mcp"]["matching_capture"] is True,
            "MAINTENANCE_SOURCE_MCP_INVALID")
    return {"owned_source_only": True, "both_owned_generations_stopped": True}


def verify_offline_copy(bundle, output, source_roots, manifest, binding):
    saved, index = verify(bundle)
    require(saved["installed_manifest_sha256"] == binding["manifest_sha256"]
            and saved["version"] == manifest["version"]
            and saved["source_commit"] == manifest["source_commit"],
            "MAINTENANCE_BACKUP_BINDING_MISMATCH")
    restored = {"config": output / "config.toml", "data": output / "data", "state": output / "state"}
    require(index["files"] == file_inventory(source_roots)
            and index["files"] == file_inventory(restored), "MAINTENANCE_RESTORED_FILES_MISMATCH")
    require(not any((restored["state"] / name).exists() for name in
                    ("daemon.lock", "start.lock", "control.sock", "instance.json")),
            "MAINTENANCE_RESTORED_EPHEMERAL_STATE")
    require({path.name for path in output.iterdir()} == {"config.toml", "data", "state", "RESTORED.json"},
            "MAINTENANCE_RESTORED_OUTPUT_SET")
    return {"byte_exact_config_data_state": True, "ephemeral_lifecycle_state_absent": True,
            "restored_offline_before_rebinding": True}


def rebind_restored_fixture(output, target_url, source_config):
    """Narrow fixture-only edit, after complete offline-copy verification."""
    config = plain_path(output / "config.toml", private=True)
    require(config.read_bytes() == source_config.read_bytes(), "MAINTENANCE_RESTORED_CONFIG_CHANGED")
    values = tomllib.loads(config.read_text())
    require(set(values) == CONFIG_KEYS and values["persist_enabled"] is True,
            "MAINTENANCE_REBIND_CONFIG_UNSUPPORTED")
    values.update(database_url=target_url, data_dir=str(output / "data"))
    rewritten = "\n".join(key + " = " + json.dumps(value) for key, value in values.items()) + "\n"
    identity, record = installation_identity(config)
    identity_root = output / (".mike-" + identity)
    identity_root.mkdir(mode=0o700)
    plain_path(output / "state", directory=True, private=True).rename(record.parent)
    temporary = output / "rebound-config.toml"
    private_write(temporary, rewritten.encode())
    os.replace(temporary, config)
    return config, {"config": config, "data": output / "data", "state": record.parent}


def restore_and_compare(kit, manifest, result, cluster, candidate_inventory, source, output, target):
    config, expected_roots = rebind_restored_fixture(output, cluster.config_url(target), source["config"])
    require(runtime_roots(kit, config, output) == expected_roots, "MAINTENANCE_REBOUND_ROOTS_MISMATCH")
    config_digest = sha256(config)
    check(result, "archive_maintenance_restored_runtime_binding", kit.recheck)
    with owned_child([str(kit.paths["mike"]), "--config", str(config), "serve"],
                     kit.cwd, runtime_environment(kit.home)) as child:
        def ready():
            health = wait_owned_ready(child, config, source["ports"][1], manifest["source_commit"],
                                      timeout=240, persistent=True)
            require(health["instance"]["boot_generation"] not in source["generations"],
                    "MAINTENANCE_BOOT_GENERATION_REUSED")
            return {"owned_pid_and_new_generation": True, "fresh_restored_database": True, "runtime_path": "empty"}
        check(result, "archive_maintenance_restored_readiness", ready)
        def continuity():
            observed = stored_capture(source["ports"][1], source["fixture"],
                                      expected_id=source["capture"]["request_id"])
            require(observed == source["capture"], "MAINTENANCE_RESTORED_CAPTURE_CHANGED")
            require(migration_ledger(cluster, target, candidate_inventory) == source["ledger"],
                    "MAINTENANCE_RESTORED_LEDGER_CHANGED")
            return {"same_single_request_session_and_body_hashes": True, "full_candidate_ledger_unchanged": True}
        check(result, "archive_maintenance_restored_capture_and_ledger", continuity)
        def mcp_continuity():
            kit.recheck()
            observed = run_mcp(kit.paths["mike-mcp"], source["ports"][1], source["capture"]["request_id"],
                               source["fixture"].session, manifest["version"], kit.home, kit.cwd)
            require({key: observed.get(key) for key in MCP_KEYS} ==
                    {key: source["mcp"][key] for key in MCP_KEYS}, "MAINTENANCE_RESTORED_MCP_CHANGED")
            return {"same_read_tool_capture_and_profile": True, "real_installed_stdio": True}
        check(result, "archive_maintenance_restored_mcp", mcp_continuity)
        check(result, "archive_maintenance_restored_owned_stop", lambda: graceful_stop(child))
    require(sha256(config) == config_digest, "MAINTENANCE_REBOUND_CONFIG_MUTATED")


def run_maintenance_roundtrip(paths, manifest, root, result, cluster, candidate_inventory, source):
    """Use the caller's owned fixture; it must finally clean Cluster and sandbox.

    source contains prefix, config, home, cwd, database, capture, ledger, mcp,
    fixture, ports and the two observed generations. No new fixture is provisioned.
    Exceptions propagate to the persistent journey's sanitized failure/cleanup.
    """
    root = Path(root)
    plain_path(root.parent, directory=True, private=True)
    check(result, "archive_maintenance_source_contract", lambda: source_contract(source, result, cluster, root))
    # The installed runtime uses the portable 103-byte Unix control socket bound.
    # Fail before any backup/DB allocation rather than discovering this at start.
    _, record = installation_identity(root / "r" / "config.toml")
    require(len(os.fsencode(record.parent / "control.sock")) <= 103, "MAINTENANCE_RESTORE_PATH_TOO_LONG")
    root.mkdir(mode=0o700)
    kit = InstalledKit(paths, manifest, result["binding"], source["prefix"], candidate_inventory,
                       source["home"], source["cwd"])
    check(result, "archive_maintenance_installed_kit_binding", kit.recheck)
    roots = runtime_roots(kit, source["config"], source["config"].parent)
    before = file_inventory(roots)
    protected = {path: sha256(path) for path in (source["home"] / ".env", source["cwd"] / ".env")}
    require(migration_ledger(cluster, source["database"], candidate_inventory) == source["ledger"],
            "MAINTENANCE_SOURCE_LEDGER_CHANGED")
    source_service = cluster.service(root / "source.service", source["database"])
    bundle, output = root / "backup", root / "r"
    check(result, "archive_maintenance_installed_backup", lambda: kit.command("backup", [
        "--config", roots["config"], "--data-root", roots["data"], "--state-root", roots["state"],
        "--pg-service-file", source_service, "--pg-service", "fixture", "--destination", bundle]))
    check(result, "archive_maintenance_installed_verify", lambda: kit.command("verify", ["--backup", bundle]))
    previous_databases = tuple(cluster.created)
    target = cluster.create(seeded=False)
    def fresh():
        require(target not in previous_databases and target in cluster.created,
                "MAINTENANCE_TARGET_NOT_NEW_OWNED_DATABASE")
        require(cluster.sql(target, "BEGIN READ ONLY; " + EMPTY_SQL + "; COMMIT;") == "0",
                "MAINTENANCE_TARGET_NOT_EMPTY")
        return {"distinct_owned_target": True, "empty_before_restore": True, "synthetic_seed_used": False}
    check(result, "archive_maintenance_fresh_target", fresh)
    target_service = cluster.service(root / "target.service", target)
    check(result, "archive_maintenance_installed_restore", lambda: kit.command("restore", [
        "--backup", bundle, "--pg-service-file", target_service, "--pg-service", "fixture", "--destination", output]))
    check(result, "archive_maintenance_offline_copy", lambda: verify_offline_copy(
        bundle, output, roots, manifest, result["binding"]))
    require(migration_ledger(cluster, target, candidate_inventory) == source["ledger"],
            "MAINTENANCE_RESTORED_OFFLINE_LEDGER_CHANGED")
    restore_and_compare(kit, manifest, result, cluster, candidate_inventory, source, output, target)
    def retained():
        require(file_inventory(roots) == before and all(sha256(path) == digest for path, digest in protected.items()),
                "MAINTENANCE_SOURCE_FILES_CHANGED")
        require(migration_ledger(cluster, source["database"], candidate_inventory) == source["ledger"],
                "MAINTENANCE_SOURCE_LEDGER_CHANGED")
        kit.recheck()
        return {"source_ledger_and_files_unchanged": True, "restore_used_only_fresh_owned_target": True}
    check(result, "archive_maintenance_source_retained", retained)
    return {"installed_backup_verify_restore": True, "restored_inspect_and_mcp_match": True,
            "qualified": False, "database_cleanup": "caller_owned_cluster_finally"}
