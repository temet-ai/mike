"""Partial, hash-bound journeys against verified archives. Never qualifies an AC case."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

from common import sha256
from journey_fixture import SECRET, TEXT, TOOL_RESULT, upstream_fixture
from journey_http import inspect_capture, spa_http, stream_capture
from journey_mcp import run_mcp
from journey_process import (JourneyError, captured_run, owned_child, private_write, require,
                             reserved_ports, runtime_environment, wait_owned_ready)
from journey_security import raw_http_boundary, raw_websocket_boundary

LIMITS = [
    "Trial capture only: no durable database, migration ledger, search or persistent restart qualification.",
    "Packaged HTML, assets and deep-link HTTP are not browser DOM, JavaScript, console or WebSocket evidence.",
    "Raw HTTP rejection and WebSocket handshakes are partial checks, not browser or complete AC06 qualification.",
    "No real Claude provider/client, native clean-account, Docker or published-channel journey.",
    "A private HOME and empty runtime PATH are not an OS sandbox. Candidate executables must already be trusted.",
    "All AC01-AC27 remain required NOT_RUN. Partial subchecks never qualify a case or release.",
]


def verify_binding(binary_dir, manifest, manifest_sha256):
    require(re.fullmatch(r"[0-9a-f]{64}", manifest_sha256), "MANIFEST_DIGEST_INVALID")
    require(re.fullmatch(r"[0-9a-f]{40}", manifest.get("source_commit", "")), "SOURCE_COMMIT_INVALID")
    artifacts = manifest.get("artifacts", [])
    require(len(artifacts) == 2 and {a.get("binary") for a in artifacts} == {"mike", "mike-mcp"}, "ARTIFACT_SET_INVALID")
    binding, paths = [], {}
    for artifact in artifacts:
        name = artifact["binary"]
        binary = (Path(binary_dir) / name).resolve(strict=True)
        require(binary.is_file() and os.access(binary, os.X_OK), "INSTALLED_BINARY_MISSING")
        installed_manifest = binary.parent / "manifest.json"
        require(installed_manifest.is_file() and sha256(installed_manifest) == manifest_sha256,
                "INSTALLED_MANIFEST_MISMATCH")
        require(json.loads(installed_manifest.read_bytes()) == manifest, "MANIFEST_VALUE_MISMATCH")
        inventory = [entry for entry in artifact.get("contents", []) if entry.get("path") == name]
        require(len(inventory) == 1, "BINARY_INVENTORY_INVALID")
        digest = sha256(binary)
        require(digest == inventory[0].get("sha256"), "INSTALLED_BINARY_HASH_MISMATCH")
        require(re.fullmatch(r"[0-9a-f]{64}", artifact.get("sha256", "")), "ARCHIVE_DIGEST_INVALID")
        binding.append({"binary": name, "binary_sha256": digest, "archive_sha256": artifact["sha256"]})
        paths[name] = binary
    return paths, {"manifest_sha256": manifest_sha256, "source_commit": manifest["source_commit"],
                   "version": manifest["version"], "target": manifest["target"], "artifacts": binding}


def new_result(binding):
    return {"schema_version": 1, "runner": "archive-trial-journey", "qualified": False,
            "state": "INCOMPLETE", "binding": binding, "checks": [], "limits": list(LIMITS),
            "full_cases": [{"id": f"AC{number:02}", "state": "NOT_RUN", "required": True}
                           for number in range(1, 28)]}


def check(result, identifier, action):
    item = {"id": identifier, "state": "RUNNING", "binding": result["binding"]}
    result["checks"].append(item)
    try:
        value = action()
        item.update(state="PASSED", observation=value)
        return value
    except Exception as error:
        item.update(state="FAILED", error_type=type(error).__name__)
        if isinstance(error, JourneyError):
            item["code"] = str(error)
        result["state"] = "FAILED"
        raise


def create_config(root, ports, upstream_port, *, database_url=None):
    root.mkdir(mode=0o700)
    home, cwd = root / "home", root / "unrelated cwd"
    home.mkdir(mode=0o700)
    cwd.mkdir(mode=0o700)
    poison = ("MIKE_PERSIST_ENABLED=true\nMIKE_DATABASE_URL=postgres://fixture:" + SECRET
              + "@127.0.0.1:1/poison\n").encode()
    private_write(home / ".env", poison)
    private_write(cwd / ".env", poison)
    values = {"persist_enabled": database_url is not None, "proxy_listen_addr": f"127.0.0.1:{ports[0]}",
              "inspect_listen_addr": f"127.0.0.1:{ports[1]}", "spa_bind": f"127.0.0.1:{ports[2]}",
              "upstream_url": f"http://127.0.0.1:{upstream_port}",
              "database_url": database_url or f"postgres://fixture:{SECRET}@127.0.0.1:1/unused"}
    config = root / "config.toml"
    lines = [key + " = " + json.dumps(value) for key, value in values.items()]
    private_write(config, ("\n".join(lines) + "\n").encode())
    return config, home, cwd


def validate_doctor(value, manifest, *, persistent=False, stopped=False):
    fields = {"schema_version", "product", "version", "source_sha", "source_dirty", "os",
              "architecture", "config_source", "read_only", "checks"}
    require(isinstance(value, dict) and set(value) == fields, "DOCTOR_REPORT_SCHEMA")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["read_only"] is True and value["product"] == "mike", "DOCTOR_IDENTITY")
    require(value["source_sha"] == manifest["source_commit"] and value["version"] == manifest["version"]
            and value["source_dirty"] is False, "DOCTOR_BUILD_MISMATCH")
    platforms = {"x86_64-unknown-linux-gnu": ("linux", "x86_64"),
                 "aarch64-apple-darwin": ("macos", "aarch64")}
    require((value["os"], value["architecture"]) == platforms.get(manifest["target"]), "DOCTOR_PLATFORM_MISMATCH")
    require(value["config_source"] == "selected_file", "DOCTOR_CONFIG_SOURCE")
    expected = {
        "configuration": (True, "pass", "CONFIG_LOADED"),
        "embedded_ui": (True, "pass", "UI_EMBEDDED"),
        "mcp_binary": (True, "pass", "MCP_BINARY_FOUND"),
        "mcp_connection": (False, "pass", "MCP_READY"),
        "claude": (False, "unavailable", "CLAUDE_ABSENT"),
        "docker": (False, "unavailable", "DOCKER_ABSENT"),
        "persistence": (False, "pass", "TRIAL_MODE"),
        "daemon": (False, "pass", "HEALTH_REPORTED_READY"),
    }
    if persistent:
        expected["persistence"] = (True, "pass", "DATABASE_CURRENT")
    if stopped:
        expected["mcp_connection"] = (False, "unavailable", "MCP_BACKEND_UNAVAILABLE")
        expected["daemon"] = (False, "unavailable", "DAEMON_NOT_REACHABLE")
    checks = value["checks"]
    require(isinstance(checks, list) and len(checks) == len(expected), "DOCTOR_CHECK_SET")
    seen = set()
    for item in checks:
        require(isinstance(item, dict) and set(item) == {"id", "required", "status", "code", "detail"},
                "DOCTOR_CHECK_SCHEMA")
        name = item["id"]
        require(isinstance(name, str) and name in expected and name not in seen, "DOCTOR_CHECK_SET")
        seen.add(name)
        require(type(item["required"]) is bool and
                (item["required"], item["status"], item["code"]) == expected[name], "DOCTOR_CHECK_FAILED")
        require(isinstance(item["detail"], str) and 0 < len(item["detail"]) <= 4096, "DOCTOR_CHECK_SCHEMA")


def doctor(binary, config, home, cwd, manifest, *, persistent=False, stopped=False):
    protected = [config, home / ".env", cwd / ".env"]
    before = {path: sha256(path) for path in protected}
    observations = []
    for flag in ("--json", "--support-bundle"):
        raw = captured_run([str(binary), "--config", str(config), "doctor", flag], cwd, runtime_environment(home))
        for planted in (SECRET, TEXT, TOOL_RESULT):
            require(planted.encode() not in raw, "DOCTOR_PRIVATE_CONTENT_LEAK")
        value = json.loads(raw)
        validate_doctor(value, manifest, persistent=persistent, stopped=stopped)
        observations.append({"mode": flag, "read_only": True, "planted_content_absent": True,
                             "report_sha256": hashlib.sha256(raw).hexdigest(), "exit_status": 0,
                             "command": ["mike", "--config", "<owned-fixture>", "doctor", flag]})
    require(all(sha256(path) == digest for path, digest in before.items()), "DOCTOR_CHANGED_PRIVATE_FILES")
    require(stat.S_IMODE(config.stat().st_mode) == 0o600, "DOCTOR_CHANGED_CONFIG_PERMISSIONS")
    return {"modes": observations, "config_unchanged": True, "private_config": True,
            "daemon_state": "stopped" if stopped else "ready"}


def run_bound(paths, manifest, root, result):
    with upstream_fixture() as (fixture, upstream_port), reserved_ports() as (ports, reservations):
        config, home, cwd = create_config(root, ports, upstream_port)
        # Release only our reservations immediately before launch. Identity verification
        # rejects a listener which wins the race; it is never adopted or signalled.
        for reservation in reservations:
            reservation.close()
        environment = runtime_environment(home)
        with owned_child([str(paths["mike"]), "--config", str(config), "serve"], cwd, environment) as child:
            def readiness():
                wait_owned_ready(child, config, ports[1], manifest["source_commit"])
                return {"owned_pid_and_generation": True, "trial_explicit": True, "runtime_path": "empty"}
            check(result, "archive_owned_trial_readiness", readiness)
            check(result, "archive_synchronized_streaming", lambda: stream_capture(ports[0], fixture))
            captured = []
            def inspect():
                request_id, evidence = inspect_capture(ports[1], fixture)
                captured.append(request_id)
                return evidence
            check(result, "archive_request_metadata_and_body", inspect)
            request_id = captured[0]
            for name, port in (("inspect", ports[1]), ("spa", ports[2])):
                check(result, "archive_" + name + "_raw_http_access_boundary",
                      lambda: raw_http_boundary(port, request_id, fixture))
                check(result, "archive_" + name + "_raw_websocket_access_boundary",
                      lambda: raw_websocket_boundary(port, fixture))
            check(result, "archive_packaged_spa_http_only", lambda: spa_http(ports[2], ports[1], ports[0], fixture.session, request_id))
            check(result, "archive_real_mcp_stdio_read", lambda: run_mcp(paths["mike-mcp"], ports[1], request_id,
                  fixture.session, manifest["version"], home, cwd))
            check(result, "archive_safe_doctor_and_support_json", lambda: doctor(paths["mike"], config, home, cwd, manifest))


def run_archive_journey(binary_dir, manifest, sandbox, manifest_sha256):
    """Caller owns sandbox cleanup; binary_dir must come from verified_inputs/install_verified.

    The caller passes the SHA-256 of the exact verified manifest bytes. This runner
    checks the installed manifest and executable hashes again immediately before use.
    Failures are returned as sanitized subchecks. It never changes full AC states.
    """
    result = new_result(None)
    try:
        paths, binding = verify_binding(binary_dir, manifest, manifest_sha256)
        result["binding"] = binding
        check(result, "archive_journey_hash_binding", lambda: {"exact_installed_hashes": True})
        run_bound(paths, manifest, Path(sandbox) / "trial journey", result)
    except Exception as error:
        result["state"] = "FAILED"
        result["failure_type"] = type(error).__name__
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    return result
