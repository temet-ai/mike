"""Disposable installed unregister/removal/reinstall phase; no host integration."""
import copy
import json
from pathlib import Path
import subprocess
import sys

from common import sha256
from journey_process import (JourneyError, captured_run, installation_identity, private_write,
                             require, runtime_environment)
from journey_upgrade_state import durable_snapshot, tree_digest
from owned_process import OwnedProcess
from recovery_files import plain_path


def registration_command(runtime, candidate, client, operation):
    candidate.recheck()
    return [str(candidate.paths["mike"]), "--config", str(runtime.config), "mcp", operation,
            "--binary", str(candidate.prefix / "bin/mike-mcp"), "--claude-config", str(client), "--json"]


def private_backup(value, client, expected):
    require(isinstance(value, str), "REINSTALL_CLIENT_BACKUP_MISSING")
    backup = Path(value)
    require(backup.parent == client.parent and backup.name.startswith(client.name + ".mike-backup-"),
            "REINSTALL_CLIENT_BACKUP_OUTSIDE_FIXTURE")
    require(plain_path(backup, private=True).read_bytes() == expected, "REINSTALL_CLIENT_BACKUP_CHANGED")


def registration_roundtrip(runtime, candidate):
    """Called while the owned candidate is ready; both clients are freshly made."""
    client = runtime.home / "synthetic-claude.json"
    original = {"theme": "synthetic-preserve", "mcpServers": {"other": {"command": "never-executed"}}}
    before = json.dumps(original, sort_keys=True).encode() + b"\n"
    private_write(client, before)
    env = runtime_environment(runtime.home)
    value = json.loads(captured_run(registration_command(runtime, candidate, client, "register"),
                                   runtime.cwd, env, timeout=45))
    require(isinstance(value, dict) and set(value) == {"client_config", "changed", "backup", "probe"}
            and value["client_config"] == str(client) and value["changed"] is True,
            "REINSTALL_REGISTER_RECEIPT")
    probe = value["probe"]
    identity, _ = installation_identity(runtime.config)
    require(isinstance(probe, dict) and probe.get("handshake") is True and probe.get("backend_read") is True
            and probe.get("instance_id") == identity and probe.get("version") == candidate.manifest["version"],
            "REINSTALL_REGISTER_NOT_CONNECTED_TO_OWNED_INSTANCE")
    private_backup(value["backup"], client, before)
    expected = copy.deepcopy(original)
    expected["mcpServers"]["mike"] = {"command": str(candidate.prefix / "bin/mike-mcp"), "args": ["--core"],
        "env": {"MIKE_INSPECT_URL": f"http://127.0.0.1:{runtime.ports[1]}", "MIKE_REGISTRATION_INSTANCE_ID": identity}}
    require(json.loads(plain_path(client, private=True).read_bytes()) == expected, "REINSTALL_REGISTER_CHANGED_FOREIGN_DATA")
    registered = client.read_bytes()
    removed = json.loads(captured_run(registration_command(runtime, candidate, client, "unregister"),
                                     runtime.cwd, env, timeout=20))
    require(isinstance(removed, dict) and set(removed) == {"client_config", "changed", "backup"}
            and removed["client_config"] == str(client) and removed["changed"] is True,
            "REINSTALL_UNREGISTER_RECEIPT")
    private_backup(removed["backup"], client, registered)
    require(json.loads(plain_path(client, private=True).read_bytes()) == original,
            "REINSTALL_UNREGISTER_CHANGED_FOREIGN_DATA")
    changed = runtime.home / "changed-synthetic-claude.json"
    expected["mcpServers"]["mike"]["args"].append("--synthetic-foreign-edit")
    changed_bytes = json.dumps(expected).encode()
    private_write(changed, changed_bytes)
    command = registration_command(runtime, candidate, changed, "unregister")
    child = OwnedProcess(command, cwd=runtime.cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        _, stderr = child.communicate(timeout=20)
        require(child.returncode is not None and child.returncode > 0
                and b"changed or foreign mike registration" in stderr, "REINSTALL_CHANGED_REGISTRATION_NOT_REFUSED")
    finally:
        child.close()
    # The CLI creates only its bounded lock before checking an entry. It must
    # leave the client itself byte-exact and must not create a backup on refusal.
    require(changed.read_bytes() == changed_bytes and not list(runtime.home.glob(changed.name + ".mike-backup-*")),
            "REINSTALL_CHANGED_REGISTRATION_MUTATED")
    return {"real_installed_register_and_unregister": True, "matching_owned_instance_probe": True,
            "only_synthetic_client_files": True, "unrelated_entry_and_settings_preserved": True,
            "byte_exact_private_backups": True, "changed_registration_refused": True}


def retained_reinstall(runtime, candidate, roots, service, bundle):
    kit = runtime.kit(candidate)
    kit_binding = kit.recheck()
    before = durable_snapshot(runtime.cluster, runtime.database, service, roots)
    generations = candidate.prefix / "lib/mike-archive/generations"
    retained = tree_digest(generations)
    backup = tree_digest(bundle)
    sentinel = candidate.prefix / "bin/unrelated-synthetic-command"
    private_write(sentinel, b"Not executable; retained uninstall fixture.\n")
    environment = runtime_environment(runtime.home)
    environment["PATH"] = str(Path(sys.executable).resolve().parent) + ":/usr/bin:/bin"
    value = json.loads(captured_run([str(kit.launcher), "uninstall", "--prefix", str(candidate.prefix),
        "--config", str(runtime.config), "--state-root", str(roots["state"]), "--confirm-stopped-writers"],
        runtime.cwd, environment, timeout=60))
    expected = {"status": "owned-commands-unlinked", "data_retained": True, "configuration_retained": True,
                "database_resources_retained": True, "backups_retained": True,
                "recovery_generations_retained": True, "integrations_changed": False}
    require(value == expected, "REINSTALL_UNINSTALL_RECEIPT")
    require(all(not (candidate.prefix / "bin" / name).exists()
                and not (candidate.prefix / "bin" / name).is_symlink() for name in ("mike", "mike-mcp")),
            "REINSTALL_OWNED_COMMANDS_NOT_REMOVED")
    require(sentinel.read_bytes() == b"Not executable; retained uninstall fixture.\n"
            and tree_digest(generations) == retained and tree_digest(bundle) == backup
            and durable_snapshot(runtime.cluster, runtime.database, service, roots) == before,
            "REINSTALL_UNINSTALL_CHANGED_RETAINED_STATE")
    candidate.recheck(active=False)
    require(all(sha256(kit.launcher.parent / name) == digest
                for name, digest in kit_binding["kit_sha256"].items()), "REINSTALL_RETAINED_KIT_CHANGED")
    value = json.loads(captured_run([str(kit.launcher), "install", "--candidate", str(candidate.snapshot),
        "--prefix", str(candidate.prefix)], runtime.cwd, environment, timeout=360))
    require(isinstance(value, dict) and set(value) == {"version", "source_commit", "manifest_sha256", "prefix", "status", "qualified"}
            and value == {"version": candidate.manifest["version"], "source_commit": candidate.manifest["source_commit"],
                         "manifest_sha256": candidate.identity, "prefix": str(candidate.prefix), "status": "unchanged", "qualified": False},
            "REINSTALL_EXACT_CANDIDATE_RECEIPT")
    candidate.bind(candidate.prefix)
    require(tree_digest(generations) == retained and tree_digest(bundle) == backup
            and durable_snapshot(runtime.cluster, runtime.database, service, roots) == before,
            "REINSTALL_CHANGED_RETAINED_STATE")
    return {"installed_maintenance_uninstall_and_install": True, "stable_pair_restored": True,
            "same_exact_candidate": True, "database_config_data_state_backups_generations_retained": True,
            "unrelated_command_retained": True, "purge": "NOT_RUN", "qualified": False}
