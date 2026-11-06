#!/usr/bin/env python3
"""Local archive acceptance foundation: explicit incomplete results, never qualification."""

import argparse
import json
from pathlib import Path
import sys
import tempfile
import time

from acceptance_ledger import INSTALL_CHECKS, POLICY, alpha_complete, case_ledger, journey_passed, record_pass
from acceptance_isolated import runner_options, validate_request, run_normal_helpers
from archive import BINARIES
from common import ReleaseError, sha256
from inputs import interruption_cleanup, isolated_smoke as smoke, verified_inputs
from install import install_verified
from install_state import safe_prefix
from journey import run_archive_journey
from journey_browser import BrowserTools
from journey_browser_installed import run_installed_browser_journey
from journey_persistent import run_persistent_journey
from journey_faults import run_fault_journey
from journey_modes import run_mode_journey
from journey_upgrade import run_upgrade_journey
from journey_reliability import reliability_policy, run_reliability_journey
from journey_reliability_profiles import SYNTHETIC_PROFILE, reliability_options

ROOT = Path(__file__).resolve().parents[2]
CASE_NAMES = (
    "archive runtime, embedded assets and reviewed contents", "persistent startup and migrations",
    "streaming capture", "packaged browser journey", "MCP stdio journey", "browser access boundary",
    "persistent restart", "native clean-account installation", "managed Docker storage",
    "BYO PostgreSQL", "configuration selection and precedence", "literal secrets and private writes",
    "trial/persistent transitions", "instance ownership", "manual restart and claimed autostart",
    "startup failure and recovery", "Claude MCP registration", "installed onboarding and help",
    "real Claude compatibility", "bounded reliability", "runtime fault recovery", "backup and restore",
    "upgrade, interruption and incompatible downgrade", "safe removal and reinstall",
    "pure preparation and publication retries", "published-channel installation", "safe diagnostics",
)


def new_report():
    return {"schema_version": 1, "runner": "local-archive-foundation", "qualified": False,
            "state": "INCOMPLETE", "candidate": None, "checks": [],
            "policy": POLICY, "cases": case_ledger(CASE_NAMES),
            "limits": ["A fresh temporary HOME is not a new OS account or a security sandbox.",
                       "Helper checks do not establish any complete AC01-AC27 case.",
                       "No real database, browser, Claude provider, published channel, or macOS policy qualification."]}


def checked(report, name, action):
    started = time.monotonic()
    item = {"id": name, "state": "RUNNING", "kind": "helper",
            "command": None, "exit_status": None, "elapsed_seconds": None}
    report["checks"].append(item)
    try:
        value = action()
        item["state"] = "PASSED"
        return value
    except Exception as exc:
        # Candidate stdout/errors can contain private planted fixture material.
        # Keep only the exception category; do not retain raw logs or env values.
        item.update(state="FAILED", error_type=type(exc).__name__)
        raise
    finally:
        item["elapsed_seconds"] = round(time.monotonic() - started, 6)


def requested_reliability(reliability, duration_seconds, concurrent_sessions):
    if not reliability:
        if duration_seconds is not None or concurrent_sessions is not None:
            raise ReleaseError("reliability duration/concurrency options require --reliability-journey")
        return None
    return reliability_policy(duration_seconds, concurrent_sessions)


def requested_observations(reliability, profile, budgets):
    if not reliability and (profile != SYNTHETIC_PROFILE or budgets is not None):
        raise ReleaseError("reliability profile/budgets require --reliability-journey")
    return reliability_options(profile, budgets)


def resource_budget_file(path):
    if path is None:
        return None
    from notice_inventory import read_bounded, read_json
    value = read_json(read_bounded(path, 16384))
    reliability_options(SYNTHETIC_PROFILE, value)
    return value


def run_helpers(candidate, report, trial_journey=False, persistent_journey=False, browser=None, maintenance=False, *,
                reliability=False, reliability_duration_seconds=None, reliability_concurrent_sessions=None, fault_journey=False,
                mode_journey=False, mode_tui_journey=False, upgrade_predecessor=None,
                reliability_profile=SYNTHETIC_PROFILE, reliability_resource_budgets=None, normal_runner=None):
    if mode_tui_journey and not mode_journey:
        raise ReleaseError("--mode-tui-journey requires --mode-journey")
    observation_options = requested_observations(reliability, reliability_profile, reliability_resource_budgets)
    reliability = requested_reliability(reliability, reliability_duration_seconds, reliability_concurrent_sessions)
    validate_request(reliability_profile, normal_runner, other_journeys=any((
        trial_journey, persistent_journey, browser, maintenance, fault_journey,
        mode_journey, mode_tui_journey, upgrade_predecessor)))
    if reliability_profile == "normal_defaults":
        return run_normal_helpers(candidate, report, normal_runner,
                                  duration_seconds=reliability_duration_seconds,
                                  concurrent_sessions=reliability_concurrent_sessions,
                                  resource_budgets=reliability_resource_budgets)
    if browser is not None and not persistent_journey:
        raise ReleaseError("browser journey requires the owned persistent journey")
    with tempfile.TemporaryDirectory(prefix="mike-acceptance-", dir="/tmp") as temporary:
        sandbox = Path(temporary).resolve()
        home, cwd, prefix = (sandbox / name for name in ("home", "unrelated cwd", "install prefix"))
        home.mkdir(mode=0o700)
        cwd.mkdir(mode=0o700)
        report["isolation"] = {"fresh_home": True, "unrelated_cwd": True,
                               "runtime_path": "empty", "cleanup": "owned temporary directory only",
                               "process_cleanup": "owned verification process groups, bounded deadlines"}
        verification = {"id": "local_archive_integrity_and_cli_smoke", "state": "RUNNING"}
        report["checks"].append(verification)
        try:
            with verified_inputs(candidate) as (snapshot, manifest):
                verification["state"] = "PASSED"
                report["candidate"] = {key: manifest[key] for key in ("version", "source_commit", "target")}
                report["candidate"]["manifest_sha256"] = sha256(snapshot / "manifest.json")
                report["candidate"]["archives"] = [{key: a[key] for key in ("binary", "archive", "sha256")}
                                                       for a in manifest["artifacts"]]
                for case in report["cases"]:
                    case["runtime_input_identity"] = dict(report["candidate"])
                checked(report, "disposable_local_install", lambda: install_verified(snapshot, manifest, prefix))
                def installed_smoke():
                    for name in BINARIES:
                        observed = smoke(prefix / "bin" / name, manifest["version"], cwd, home)
                        expected = next(a["smoke"] for a in manifest["artifacts"] if a["binary"] == name)
                        if observed != expected:
                            raise ReleaseError("installed smoke differs from archived evidence")
                checked(report, "installed_cli_smoke", installed_smoke)
                def repeat():
                    result = install_verified(snapshot, manifest, prefix)
                    if result["status"] != "unchanged":
                        raise ReleaseError("repeat install did not preserve selected generation")
                    installed_smoke()
                checked(report, "repeat_install_and_cli_smoke", repeat)
                if trial_journey:
                    journey = run_archive_journey(prefix / "bin", manifest, sandbox,
                                                  sha256(snapshot / "manifest.json"))
                    report["archive_trial_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive trial journey failed; inspect sanitized check codes")
                if persistent_journey:
                    journey = run_persistent_journey(prefix / "bin", manifest, sandbox,
                                                     sha256(snapshot / "manifest.json"),
                                                     **({"maintenance": True} if maintenance else {}))
                    report["archive_persistent_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive persistent journey failed; inspect sanitized check codes")
                if browser is not None:
                    journey = run_installed_browser_journey(prefix / "bin", manifest, sandbox,
                                                            sha256(snapshot / "manifest.json"), browser)
                    report["archive_browser_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive browser journey failed; inspect sanitized check codes")
                if reliability is not None:
                    # Never reuse the restart/maintenance or browser capture.
                    # This helper allocates its own fresh database and runtime.
                    with tempfile.TemporaryDirectory(prefix="reliability-", dir=sandbox) as reliability_sandbox:
                        journey = run_reliability_journey(prefix / "bin", manifest, Path(reliability_sandbox),
                                                          sha256(snapshot / "manifest.json"),
                                                          duration_seconds=reliability_duration_seconds,
                                                          concurrent_sessions=reliability_concurrent_sessions,
                                                          **({} if observation_options == reliability_options() else observation_options))
                    report["archive_reliability_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive reliability journey failed; inspect sanitized check codes")
                if fault_journey:
                    # Independent config, sessions, DB and process; never fault
                    # the baseline, maintenance, browser or reliability fixture.
                    with tempfile.TemporaryDirectory(prefix="faults-", dir=sandbox) as fault_sandbox:
                        journey = run_fault_journey(prefix / "bin", manifest, Path(fault_sandbox),
                                                    sha256(snapshot / "manifest.json"))
                    report["archive_fault_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive fault journey failed; inspect sanitized check codes")
                if mode_journey:
                    journey = run_mode_journey(prefix / "bin", manifest, sandbox,
                                               sha256(snapshot / "manifest.json"), tui=mode_tui_journey)
                    report["archive_mode_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive mode journey failed; inspect sanitized check codes")
                if upgrade_predecessor is not None:
                    journey = run_upgrade_journey(snapshot, manifest, sandbox,
                                                  sha256(snapshot / "manifest.json"), upgrade_predecessor)
                    report["archive_upgrade_journey"] = journey
                    report["checks"].extend(journey["checks"])
                    report["limits"].extend(journey["limits"])
                    if journey["state"] == "FAILED":
                        raise ReleaseError("archive upgrade journey failed; inspect sanitized check codes")
                record_passes(report, sha256(snapshot / "manifest.json"), maintenance)
        except Exception as exc:
            if verification["state"] == "RUNNING":
                verification.update(state="FAILED", error_type=type(exc).__name__)
            raise


def record_passes(report, manifest_sha256, maintenance):
    """Promote a case only from a journey that ran on this candidate with every check PASSED."""
    def ids(journey):
        return [item["id"] for item in journey["checks"]]
    install = [item for item in report["checks"] if item["id"] in INSTALL_CHECKS]
    installed = len(install) == len(INSTALL_CHECKS) and all(item["state"] == "PASSED" for item in install)
    trial = report.get("archive_trial_journey")
    persistent = report.get("archive_persistent_journey")
    upgrade = report.get("archive_upgrade_journey")
    if installed:
        record_pass(report, ("AC08",), "disposable_local_install", INSTALL_CHECKS)
    persistent_passed = bool(persistent) and journey_passed(persistent, manifest_sha256)
    if installed and trial and persistent_passed and journey_passed(trial, manifest_sha256):
        # Search and the durable restart are persistent-journey steps; the trial journey cannot show them.
        record_pass(report, ("AC01",), "archive_trial_and_persistent_journeys", ids(trial) + ids(persistent))
    if persistent_passed:
        record_pass(report, ("AC02", "AC03", "AC07"), "archive_persistent_journey", ids(persistent))
        if maintenance:
            record_pass(report, ("AC22",), "archive_persistent_journey", ids(persistent))
    if upgrade and journey_passed(upgrade, manifest_sha256):
        record_pass(report, ("AC23", "AC24"), "archive_upgrade_journey", ids(upgrade))


def output_directory(path, candidate, predecessor=None):
    path = safe_prefix(path)
    protected_roots = [ROOT, candidate.resolve()]
    if predecessor is not None:
        protected_roots.append(Path(predecessor).resolve())
    for protected in protected_roots:
        if path == protected or protected in path.parents:
            raise ReleaseError("acceptance evidence must be outside the source checkout and candidate")
    if not path.parent.is_dir():
        raise ReleaseError("evidence output parent must already exist")
    path.mkdir(mode=0o700)
    return path


def run_acceptance(candidate, output, trial_journey=False, persistent_journey=False, browser=None, maintenance=False, *,
                   reliability=False, reliability_duration_seconds=None, reliability_concurrent_sessions=None, fault_journey=False,
                mode_journey=False, mode_tui_journey=False, upgrade_predecessor=None,
                reliability_profile=SYNTHETIC_PROFILE, reliability_resource_budgets=None, normal_runner=None):
    requested_reliability(reliability, reliability_duration_seconds, reliability_concurrent_sessions)
    requested_observations(reliability, reliability_profile, reliability_resource_budgets)
    if maintenance and not persistent_journey:
        raise ReleaseError("--maintenance-journey requires --persistent-journey")
    if mode_tui_journey and not mode_journey:
        raise ReleaseError("--mode-tui-journey requires --mode-journey")
    validate_request(reliability_profile, normal_runner, other_journeys=any((
        trial_journey, persistent_journey, browser, maintenance, fault_journey,
        mode_journey, mode_tui_journey, upgrade_predecessor)))
    output = output_directory(output, candidate, upgrade_predecessor)
    report = new_report()
    try:
        run_helpers(candidate, report, trial_journey, persistent_journey, browser, maintenance=maintenance,
                    reliability=reliability, reliability_duration_seconds=reliability_duration_seconds,
                    reliability_concurrent_sessions=reliability_concurrent_sessions, fault_journey=fault_journey,
                    mode_journey=mode_journey, mode_tui_journey=mode_tui_journey,
                    upgrade_predecessor=upgrade_predecessor, reliability_profile=reliability_profile,
                    reliability_resource_budgets=reliability_resource_budgets, normal_runner=normal_runner)
    except Exception as exc:
        report.update(state="FAILED", failure_type=type(exc).__name__)
    report["required_not_run"] = sum(item["required"] and item["state"] == "NOT_RUN"
                                      for item in report["cases"])
    complete = alpha_complete(report)
    report["verdict"] = "FAIL" if report["state"] == "FAILED" else "PASS" if complete else "NOT RUN"
    report["qualified"] = complete
    report["scope"] = "alpha: install and run, plus every case that can lose data"
    if complete:
        report["state"] = "COMPLETE"
    (output / "result.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    summary = ["Release acceptance: " + report["verdict"], f"Alpha gate complete: {str(complete).lower()}", ""]
    summary.extend(f"{item['state']}: {item['id']}" for item in report["checks"])
    summary.extend(f"{item['state']}: {item['id']} {item['name']}" for item in report["cases"])
    summary.append("\nA gating NOT RUN case prevents the alpha gate. Non-gating cases do not gate an alpha.")
    (output / "summary.txt").write_text("\n".join(summary) + "\n")
    for path in output.iterdir():
        path.chmod(0o600)
    print("\n".join(summary))
    print(f"Evidence: {output / 'result.json'}")
    return 1 if report["state"] == "FAILED" else 0 if complete else 2


def main():
    parser = argparse.ArgumentParser(prog="scripts/release-acceptance", description=__doc__, epilog=
        "Runs only explicitly trusted local archives; no build/download/external provider calls. "
        "Exit 0: every alpha-gating case PASS; exit 1: helper failure; exit 2: a gating case NOT RUN.")
    parser.add_argument("--candidate", required=True, type=Path,
                        help="directory containing manifest.json and both archive/checksum pairs")
    parser.add_argument("--output", required=True, type=Path,
                        help="new private evidence directory outside source and candidate (parent must exist)")
    parser.add_argument("--trial-journey", action="store_true",
                        help="also run owned foreground trial, synchronized streaming, raw HTTP/WebSocket access-boundary probes, packaged assets, MCP stdio and doctor helpers; full cases remain NOT_RUN")
    parser.add_argument("--persistent-journey", action="store_true",
                        help="also run candidate-owned migrations, capture and foreground restart helpers; requires explicit MIKE_TEST_ADMIN_URL and MIKE_RECOVERY_TEST_PGDATA for an owned disposable TCP-only PG19 fixture; full cases remain NOT_RUN")
    parser.add_argument("--maintenance-journey", action="store_true",
                        help="also exercise installed backup/verify/fresh restore and inspect/MCP continuity after owned persistent restart; requires --persistent-journey; all full cases remain NOT_RUN")
    parser.add_argument("--browser-journey", action="store_true",
                        help="opt in to partial installed Chromium DOM, refresh, help and live-event checks in a separate owned persistent fixture after --persistent-journey; no browser download, remote attachment or sandbox workaround")
    parser.add_argument("--browser-node", type=Path,
                        help="explicit absolute path to an already-installed trusted Node executable")
    parser.add_argument("--browser-chromium", type=Path,
                        help="explicit absolute path to an already-installed trusted Chromium executable; existing frontend Playwright dependency required")
    parser.add_argument("--reliability-journey", action="store_true",
                        help="opt in to a separate owned PG19 synthetic reliability workload of at least 900 seconds; isolation overrides are not normal-default AC20 qualification")
    parser.add_argument("--reliability-duration-seconds", type=float,
                        help="explicit real monotonic workload duration, 900 to 7200; requires --reliability-journey")
    parser.add_argument("--reliability-concurrent-sessions", type=int,
                        help="synthetic sessions per overlapping paced batch, 1-32; requires --reliability-journey")
    parser.add_argument("--fault-journey", action="store_true",
                        help="opt in to separate owned PG19 synthetic 429/500, stalled-stream cancellation and durable recovery checks; DB outage/disk faults and full AC21 remain NOT_RUN")
    parser.add_argument("--mode-journey", action="store_true",
                        help="opt in to actual selected-installation trial/DB mode transitions in a separate owned fixture; requires disposable PG19 and private control-socket support; full cases remain NOT_RUN")
    parser.add_argument("--mode-tui-journey", action="store_true",
                        help="also drive the installed setup/control TUI through an owned bounded PTY; requires --mode-journey")
    parser.add_argument("--upgrade-predecessor", type=Path,
                        help="explicit trusted frozen predecessor archive pair for populated two-version upgrade/interruption/retained reinstall checks; never inferred or downloaded")
    parser.add_argument("--browser-ipv6", action="store_true",
                        help="also run the owned browser boundary fixture on IPv6 loopback; requires --browser-journey")
    parser.add_argument("--reliability-profile", choices=(SYNTHETIC_PROFILE, "normal_defaults"),
                        default=SYNTHETIC_PROFILE, help="explicit reliability profile; normal defaults require the separately configured isolated Linux runner, never host execution or disabled-feature coverage")
    parser.add_argument("--reliability-resource-budgets", type=Path,
                        help="explicit bounded JSON containing all six resource/drain/responsiveness budgets; requires --reliability-journey; no implicit limits")
    parser.add_argument("--normal-runner-image", help="already-local explicitly trusted repository@sha256 image; never pulled; requires normal_defaults")
    parser.add_argument("--normal-runner-docker", type=Path, help="absolute already-installed trusted Docker executable for the isolated runner")
    parser.add_argument("--normal-runner-runtime", help="explicit already-configured custom native runc alias with an absolute daemon path; never configured by this helper")
    parser.add_argument("--normal-runner-endpoint", type=Path, help="absolute local Docker Unix socket; no remote daemon or inherited context")
    args = parser.parse_args()
    if args.mode_tui_journey and not args.mode_journey:
        parser.error("--mode-tui-journey requires --mode-journey")
    if args.browser_ipv6 and not args.browser_journey:
        parser.error("--browser-ipv6 requires --browser-journey")
    try:
        budgets = resource_budget_file(args.reliability_resource_budgets)
        requested_reliability(args.reliability_journey, args.reliability_duration_seconds, args.reliability_concurrent_sessions)
        requested_observations(args.reliability_journey, args.reliability_profile, budgets)
        normal_runner = runner_options(args.normal_runner_image, args.normal_runner_docker,
                                       args.normal_runner_endpoint, args.normal_runner_runtime)
        validate_request(args.reliability_profile, normal_runner, other_journeys=any((
            args.trial_journey, args.persistent_journey, args.browser_journey, args.maintenance_journey,
            args.fault_journey, args.mode_journey, args.mode_tui_journey, args.upgrade_predecessor)))
    except (ReleaseError, OSError, ValueError):
        parser.error("invalid reliability options or unreadable bounded budget file; no helper was started")
    browser = None
    if args.browser_journey:
        if not args.persistent_journey or args.browser_node is None or args.browser_chromium is None:
            parser.error("--browser-journey requires --persistent-journey, --browser-node and --browser-chromium")
        browser = BrowserTools(args.browser_node, args.browser_chromium,
                               **({"ipv6": True} if args.browser_ipv6 else {}))
    elif args.browser_node is not None or args.browser_chromium is not None:
        parser.error("browser executable paths require --browser-journey")
    return run_acceptance(args.candidate, args.output, args.trial_journey, args.persistent_journey, browser,
                          maintenance=args.maintenance_journey, reliability=args.reliability_journey,
                          reliability_duration_seconds=args.reliability_duration_seconds,
                          reliability_concurrent_sessions=args.reliability_concurrent_sessions, fault_journey=args.fault_journey,
                          mode_journey=args.mode_journey, mode_tui_journey=args.mode_tui_journey,
                          upgrade_predecessor=args.upgrade_predecessor, reliability_profile=args.reliability_profile,
                          reliability_resource_budgets=budgets, normal_runner=normal_runner)


if __name__ == "__main__":
    try:
        with interruption_cleanup():
            sys.exit(main())
    except (ReleaseError, OSError, ValueError, KeyError, TypeError) as exc:
        print(f"release-acceptance: {exc}", file=sys.stderr)
        sys.exit(1)
