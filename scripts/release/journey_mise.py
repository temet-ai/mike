"""Linux release journey through mise on a fresh HOME. Partial and hash-bound; never qualifies an AC case.

The candidate archives are served to mise from a local file registry (the `http:` backend with
`file://` URLs, no network). Every step records the candidate binding through `journey.check`.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import sys
import time

from common import sha256
from inputs import verified_inputs
from journey import check, new_result
from journey_fixture import upstream_fixture
from journey_http import inspect_capture, stream_capture
from journey_process import (JourneyError, captured_run, installation_identity, http_json, owned_child,
                             private_write, require, reserved_ports, runtime_environment, validate_identity)

TOOLS = ("mike", "mike-mcp")
LIMITS = [
    "Linux x86_64 only, through mise's http backend over a local file registry; no published mise registry or channel.",
    "Trial (in-memory) capture only: no durable database, migration ledger or persistent restart qualification.",
    "Restart and upgrade are owned foreground `mike serve` processes, not the systemd user services.",
    "Upgrade is a second mise version of the same archive bytes, so it proves the mise path and not a data migration.",
    "A private HOME and a minimal PATH are not an OS sandbox. Candidate executables must already be trusted.",
    "All AC01-AC27 remain required NOT_RUN. Passing steps never qualify a case or release.",
]


def upgrade_version(version):
    return version + ".1"


def mise_environment(layout, mise_dir):
    root = layout["mise"]
    return {"HOME": str(layout["home"]), "TMPDIR": str(layout["tmp"]), "PATH": str(mise_dir),
            "LC_ALL": "C", "LANG": "C", "MISE_DATA_DIR": str(root / "data"),
            "MISE_CONFIG_DIR": str(root / "config"), "MISE_CACHE_DIR": str(root / "cache"),
            "MISE_STATE_DIR": str(root / "state"), "MISE_GLOBAL_CONFIG_FILE": str(root / "global.toml"),
            "MISE_TRUSTED_CONFIG_PATHS": str(layout["project"]), "MISE_YES": "1",
            "MISE_CD": str(layout["project"]), "MISE_NO_ENV": "1", "MISE_OFFLINE": "1",
            "XDG_CONFIG_HOME": str(layout["home"] / ".config"), "XDG_DATA_HOME": str(layout["home"] / ".local/share"),
            "XDG_STATE_HOME": str(layout["home"] / ".local/state"), "XDG_CACHE_HOME": str(layout["home"] / ".cache")}


def make_layout(output):
    layout = {name: output / path for name, path in (
        ("home", "home"), ("tmp", "home/tmp"), ("project", "project"), ("mise", "mise"),
        ("registry", "registry"), ("cwd", "cwd"))}
    output.mkdir(mode=0o700, exist_ok=True)
    for name in ("home", "project", "mise", "registry", "cwd"):
        layout[name].mkdir(mode=0o700)
    layout["tmp"].mkdir(mode=0o700)
    for name in ("data", "config", "cache", "state"):
        (layout["mise"] / name).mkdir(mode=0o700)
    private_write(layout["mise"] / "global.toml", b"")
    return layout


def stage_registry(layout, snapshot, manifest, manifest_sha256):
    """Publish the verified archive bytes under two mise versions; return the byte digests."""
    versions = (manifest["version"], upgrade_version(manifest["version"]))
    digests = {}
    for artifact in manifest["artifacts"]:
        source = snapshot / artifact["archive"]
        digest = sha256(source)
        require(digest == artifact["sha256"], "REGISTRY_ARCHIVE_HASH_MISMATCH")
        for version in versions:
            directory = layout["registry"] / version
            directory.mkdir(mode=0o700, exist_ok=True)
            shutil.copyfile(source, directory / artifact["archive"])
            require(sha256(directory / artifact["archive"]) == digest, "REGISTRY_COPY_HASH_MISMATCH")
        digests[artifact["binary"]] = digest
    return versions, digests


def write_project(layout, manifest, version):
    lines = []
    for artifact in manifest["artifacts"]:
        url = "file://" + str(layout["registry"]) + "/{{ version }}/" + artifact["archive"]
        lines += [f'[tools."http:{artifact["binary"]}"]', f'version = "{version}"', f'url = "{url}"', ""]
    pending = layout["project"] / "mise.toml.pending"
    private_write(pending, "\n".join(lines).encode())
    os.replace(pending, layout["project"] / "mise.toml")


def mise(layout, mise_binary, *args, timeout=180):
    return captured_run([str(mise_binary), *args], layout["project"],
                        mise_environment(layout, Path(mise_binary).parent), timeout=timeout)


def installed_binaries(layout, mise_binary, manifest):
    """Resolve the mike binaries mise installed and require the manifest's exact bytes."""
    paths = {}
    for line in mise(layout, mise_binary, "bin-paths").decode().splitlines():
        for artifact in manifest["artifacts"]:
            candidate = Path(line.strip()) / artifact["binary"]
            if candidate.is_file():
                paths[artifact["binary"]] = candidate.resolve(strict=True)
    require(set(paths) == set(TOOLS), "MISE_INSTALLED_BINARY_MISSING")
    for artifact in manifest["artifacts"]:
        expected = [entry for entry in artifact["contents"] if entry["path"] == artifact["binary"]]
        require(len(expected) == 1 and sha256(paths[artifact["binary"]]) == expected[0]["sha256"],
                "MISE_INSTALLED_BINARY_HASH_MISMATCH")
        require(os.access(paths[artifact["binary"]], os.X_OK), "MISE_INSTALLED_BINARY_NOT_EXECUTABLE")
    return paths


def path_for(paths):
    return ":".join(sorted({str(path.parent) for path in paths.values()}))


def run_environment(layout, paths, ports, upstream_port):
    environment = runtime_environment(layout["home"])
    environment.update(PATH=path_for(paths), TMPDIR=str(layout["tmp"]),
                       MIKE_LISTEN_ADDR=f"127.0.0.1:{ports[0]}", MIKE_INSPECT_ADDR=f"127.0.0.1:{ports[1]}",
                       MIKE_SPA_BIND=f"127.0.0.1:{ports[2]}", MIKE_UPSTREAM_URL=f"http://127.0.0.1:{upstream_port}")
    return environment


def record_path_for(config):
    """The default config path keeps state under XDG_STATE_HOME; any other path uses `.mike-<id>/state`."""
    _, selected = installation_identity(config)
    home = config.parent.parent.parent
    if config == home / ".config/mike/config.toml":
        return home / ".local/state/mike/instance.json"
    return selected


def identity_record(config):
    return json.loads(record_path_for(config).read_bytes())


def wait_ready(config, inspect_port, commit, pid=None, timeout=30):
    identity, _ = installation_identity(config)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            record = identity_record(config)
            health = http_json(inspect_port, "/api/health")
            validate_identity(record, health, identity, pid or record["pid"], commit)
            return record
        except (OSError, ValueError, KeyError, JourneyError):
            time.sleep(0.1)
    raise JourneyError("MISE_DAEMON_READINESS_TIMEOUT")


def pid_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def stop_recorded(config, binary):
    """SIGTERM the process the private identity record names, after proving it runs this binary."""
    record = identity_record(config)
    pid = record["pid"]
    require(isinstance(pid, int) and pid > 1 and os.readlink(f"/proc/{pid}/exe") == str(binary), "MISE_STOP_NOT_OWNED")
    os.kill(pid, signal.SIGTERM)
    deadline = time.monotonic() + 20
    while pid_alive(pid):
        require(time.monotonic() < deadline, "MISE_STOP_TIMEOUT")
        time.sleep(0.1)
    return {"stopped_recorded_pid": True, "generation": record["generation"]}


def capture_check(ports, fixture):
    stream = stream_capture(ports[0], fixture)
    request_id, evidence = inspect_capture(ports[1], fixture)
    return {"stream": stream, "inspect": evidence, "request_id_found": bool(request_id)}


def setup_trial(layout, binary, config, environment):
    """Non-interactive setup. It saves the config and leaves an owned server running."""
    raw = captured_run([str(binary), "--config", str(config), "setup", "--mode", "trial",
                        "--accept-trial-loss", "--json"], layout["cwd"], environment, timeout=90)
    value = json.loads(raw)
    require(value.get("persistent") is False and value.get("verified_running") is True
            and value.get("config_file") == str(config), "MISE_SETUP_RECEIPT")
    require(config.stat().st_mode & 0o077 == 0, "MISE_SETUP_CONFIG_NOT_PRIVATE")
    return {"non_interactive": True, "mode": "trial", "verified_running": True, "private_config": True}


def serve_phase(layout, binary, config, environment, ports, manifest, label, result):
    """Run `mike serve` as an owned child, capture one request, and return the identity record."""
    commit = manifest["source_commit"]
    with upstream_fixture() as (fixture, upstream_port):
        environment = dict(environment, MIKE_UPSTREAM_URL=f"http://127.0.0.1:{upstream_port}")
        with owned_child([str(binary), "--config", str(config), "serve"], layout["cwd"], environment) as child:
            record = check(result, label + "_ready", lambda: wait_ready(config, ports[1], commit, pid=child.pid))
            check(result, label + "_one_capture_found_in_inspect_api", lambda: capture_check(ports, fixture))
            return {"pid_is_owned_child": record["pid"] == child.pid, "generation": record["generation"]}


def real_claude_phase(layout, binary, config, environment, ports, manifest, claude, result):
    """Serve with the real upstream, run one real Claude print session through the proxy, and read its capture."""
    token = os.environ.get("CLAUDE_CODE_OAUTH_TOKEN")
    require(token and Path(claude).is_file() and os.access(claude, os.X_OK), "REAL_CLAUDE_INPUT_MISSING")
    commit = manifest["source_commit"]
    served = {key: value for key, value in environment.items() if key != "MIKE_UPSTREAM_URL"}
    client = {"HOME": str(layout["home"]), "TMPDIR": str(layout["tmp"]), "PATH": served["PATH"], "LC_ALL": "C", "LANG": "C",
              "ANTHROPIC_BASE_URL": f"http://127.0.0.1:{ports[0]}", "CLAUDE_CODE_OAUTH_TOKEN": token}
    with owned_child([str(binary), "--config", str(config), "serve"], layout["cwd"], served) as child:
        check(result, "real_claude_ready", lambda: wait_ready(config, ports[1], commit, pid=child.pid))
        def session():
            before = len(http_json(ports[1], "/api/requests"))
            captured_run([str(claude), "-p", "Reply with the single word: ok", "--max-turns", "1"],
                         layout["cwd"], client, timeout=120)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                rows = http_json(ports[1], "/api/requests")
                done = [row for row in rows if row.get("status") == "Complete" and row.get("session_id")]
                if len(rows) > before and done:
                    detail = http_json(ports[1], f"/api/requests/{done[0]['id']}")
                    usage = detail.get("token_usage") or {}
                    require(detail.get("response_status") == 200, "REAL_CLAUDE_UPSTREAM_STATUS")
                    return {"real_session_captured": True, "captured_requests": len(rows) - before,
                            "response_status": detail.get("response_status"), "is_streaming": detail.get("is_streaming"),
                            "input_tokens": usage.get("input_tokens"), "output_tokens": usage.get("output_tokens")}
                time.sleep(0.2)
            raise JourneyError("REAL_CLAUDE_CAPTURE_TIMEOUT")
        check(result, "real_claude_session_captured_in_inspect_api", session)


def uninstall_phase(layout, mise_binary, paths, config, environment, versions, result):
    kept = {config: sha256(config)}
    state = config.parent
    check(result, "mike_uninstall_keeps_config_and_data", lambda: uninstall_mike(paths["mike"], config, environment, kept, state))
    def remove():
        for tool in TOOLS:
            for version in versions:
                mise(layout, mise_binary, "uninstall", f"http:{tool}@{version}")
        installs = layout["mise"] / "data" / "installs"
        leftovers = [str(path) for path in installs.rglob("*") if path.name in TOOLS and path.is_file()] if installs.exists() else []
        require(not leftovers, "MISE_UNINSTALL_LEFT_BINARIES")
        require(all(sha256(path) == digest for path, digest in kept.items()), "MISE_UNINSTALL_CHANGED_CONFIG")
        return {"binaries_removed": True, "config_kept": True, "mise_versions_removed": list(versions)}
    check(result, "mise_uninstall_both_versions", remove)


def uninstall_mike(binary, config, environment, kept, state):
    raw = captured_run([str(binary), "--config", str(config), "uninstall"], config.parent, environment, timeout=60)
    require(config.is_file() and sha256(config) == kept[config], "MIKE_UNINSTALL_CHANGED_CONFIG")
    require(state.is_dir(), "MIKE_UNINSTALL_REMOVED_STATE")
    return {"purge": False, "config_kept": True, "output_sha256": sha256_bytes(raw)}


def sha256_bytes(data):
    import hashlib
    return hashlib.sha256(data).hexdigest()


def mise_install(layout, mise_binary, manifest, version):
    write_project(layout, manifest, version)
    mise(layout, mise_binary, "install", timeout=240)
    paths = installed_binaries(layout, mise_binary, manifest)
    return paths, {"mise_version": version, "binaries_match_manifest": True, "sources": "file_registry"}


def run_bound(layout, snapshot, manifest, manifest_sha256, mise_binary, result, claude=None):
    versions, _ = (None, None)
    def staged():
        nonlocal versions
        versions, digests = stage_registry(layout, snapshot, manifest, manifest_sha256)
        return {"versions": list(versions), "archive_sha256": digests, "network": False}
    check(result, "mise_file_registry_hash_bound", staged)
    first = {}
    def install_first():
        first["paths"], observation = mise_install(layout, mise_binary, manifest, versions[0])
        return observation
    check(result, "mise_install_first_version", install_first)
    paths = first["paths"]
    config = layout["home"] / ".config/mike/config.toml"
    commit = manifest["source_commit"]
    with reserved_ports() as (ports, reservations), upstream_fixture() as (fixture, upstream_port):
        for reservation in reservations:
            reservation.close()
        environment = run_environment(layout, paths, ports, upstream_port)
        check(result, "setup_non_interactive_trial", lambda: setup_trial(layout, paths["mike"], config, environment))
        record = check(result, "setup_server_ready_and_owned", lambda: wait_ready(config, ports[1], commit))
        check(result, "setup_server_one_capture_found_in_inspect_api", lambda: capture_check(ports, fixture))
        config_digest = sha256(config)
        check(result, "setup_server_stopped", lambda: stop_recorded(config, paths["mike"]))
        first_generation = record["generation"]
        def restarted():
            observed = serve_phase(layout, paths["mike"], config, environment, ports, manifest, "restart", result)
            require(observed["generation"] != first_generation and sha256(config) == config_digest,
                    "MISE_RESTART_NOT_A_NEW_GENERATION_OR_CONFIG_CHANGED")
            return dict(observed, new_boot_generation=True, config_unchanged=True)
        check(result, "restart_new_generation_with_unchanged_config", restarted)
        def upgraded():
            new_paths, observation = mise_install(layout, mise_binary, manifest, versions[1])
            require(all(sha256(new_paths[name]) == sha256(paths[name]) for name in TOOLS), "MISE_UPGRADE_BYTES_DIFFER")
            require(all(new_paths[name] != paths[name] for name in TOOLS), "MISE_UPGRADE_SAME_INSTALL_PATH")
            first["upgraded"] = new_paths
            return dict(observation, same_bytes=True, new_install_path=True)
        check(result, "mise_upgrade_second_version_same_bytes", upgraded)
        new_paths = first["upgraded"]
        upgrade_environment = dict(environment, PATH=path_for(new_paths))
        def serve_upgraded():
            observed = serve_phase(layout, new_paths["mike"], config, upgrade_environment, ports, manifest, "upgrade", result)
            require(sha256(config) == config_digest, "MISE_UPGRADE_CHANGED_CONFIG")
            return dict(observed, config_unchanged=True)
        check(result, "upgrade_serves_and_captures_with_kept_config", serve_upgraded)
        if claude:
            real_claude_phase(layout, new_paths["mike"], config, upgrade_environment, ports, manifest, claude, result)
        uninstall_phase(layout, mise_binary, new_paths, config, upgrade_environment, versions, result)


def binding_for(manifest, manifest_sha256):
    return {"manifest_sha256": manifest_sha256, "source_commit": manifest["source_commit"],
            "version": manifest["version"], "target": manifest["target"],
            "artifacts": [{"binary": a["binary"], "archive_sha256": a["sha256"],
                           "binary_sha256": [e["sha256"] for e in a["contents"] if e["path"] == a["binary"]][0]}
                          for a in manifest["artifacts"]]}


def validate_output(output, candidate):
    output = Path(os.path.abspath(output))
    repo = Path(__file__).resolve().parents[2]
    for forbidden in (repo, Path(os.path.abspath(candidate))):
        require(forbidden != output and forbidden not in output.parents and output not in forbidden.parents,
                "MISE_OUTPUT_INSIDE_CHECKOUT_OR_CANDIDATE")
    require(not os.path.lexists(output), "MISE_OUTPUT_NOT_NEW")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.mkdir(mode=0o700)
    return output


def summary(result):
    lines = [f"mise Linux journey: {result['state']} (qualified: False; AC01-AC27 NOT_RUN)"]
    lines += [f"  {item['state']:7} {item['id']}" for item in result["checks"]]
    return "\n".join(lines) + "\n"


def run_journey(candidate, output, claude=None):
    mise_binary = shutil.which("mise")
    require(mise_binary, "MISE_NOT_ON_PATH")
    output = validate_output(output, candidate)
    result = new_result(None)
    result.update(runner="mise-linux-journey", limits=list(LIMITS))
    try:
        with verified_inputs(Path(candidate)) as (snapshot, manifest):
            digest = sha256(snapshot / "manifest.json")
            result["binding"] = binding_for(manifest, digest)
            check(result, "mise_journey_hash_binding", lambda: {"candidate_verified": True, "manifest_sha256": digest})
            layout = make_layout(output / "run")
            run_bound(layout, snapshot, manifest, digest, Path(mise_binary).resolve(), result, claude)
        result["state"] = "PASSED_PARTIAL"
    except Exception as error:
        result["state"] = "FAILED"
        result["failure_type"] = type(error).__name__
        if isinstance(error, JourneyError):
            result["failure_code"] = str(error)
    private_write(output / "result.json", (json.dumps(result, indent=2, sort_keys=True) + "\n").encode())
    private_write(output / "summary.txt", summary(result).encode())
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(prog="scripts/release-journey-mise", description=__doc__.splitlines()[0])
    parser.add_argument("--candidate", required=True, type=Path, help="candidate directory from scripts/release.sh")
    parser.add_argument("--output", required=True, type=Path, help="new directory outside the checkout and candidate")
    parser.add_argument("--real-claude", type=Path, help="absolute path of the claude executable; needs CLAUDE_CODE_OAUTH_TOKEN in the environment")
    args = parser.parse_args(argv)
    result = run_journey(args.candidate, args.output, args.real_claude)
    sys.stdout.write(summary(result))
    return 0 if result["state"] == "PASSED_PARTIAL" else 1


if __name__ == "__main__":
    sys.exit(main())
