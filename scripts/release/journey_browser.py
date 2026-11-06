"""Opt-in browser checks of an already-owned persistent archive fixture, never AC qualification.

Tooling must already be installed and explicitly selected. No discovery, browser
installation, remote debugging attachment, sandbox disabling, or launch fallback.
The parent owns the daemon and verifies its identity before and after this helper.
"""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re

from common import sha256
from journey_fixture import TEXT, TOOL_NAME, TOOL_RESULT
from journey_process import JourneyError, captured_run, private_write, require, runtime_environment

ROOT = Path(__file__).resolve().parents[2]
RUNNER = Path(__file__).with_name("journey_browser.mjs")
PLAYWRIGHT = ROOT / "frontend" / "node_modules" / "playwright"
CHECKS = (
    "capture_request_dom", "capture_response_dom", "direct_link_reload",
    "live_websocket_capture", "live_request_count_dom", "help_navigation",
    "help_selected_endpoints", "packaged_docs_reload", "same_origin_assets",
    "no_console_or_page_errors", "fleet_empty_project_onboarding", "fleet_single_project_selection",
    "fleet_multiple_project_deterministic_selection", "fleet_explicit_project_reload",
)
FAILURE_CODES = frozenset({
    "BROWSER_SPEC_INVALID", "BROWSER_TOOLING_UNAVAILABLE", "BROWSER_LAUNCH_UNAVAILABLE",
    "BROWSER_TIMEOUT", "BROWSER_CAPTURE_DOM", "BROWSER_DIRECT_LINK_RELOAD",
    "BROWSER_LIVE_EVENT", "BROWSER_LIVE_DOM", "BROWSER_HELP_NAVIGATION",
    "BROWSER_HELP_ENDPOINTS", "BROWSER_PACKAGED_DOCS", "BROWSER_NETWORK_SCOPE",
    "BROWSER_CONSOLE_OR_PAGE_ERROR", "BROWSER_CHECK_FAILED", "BROWSER_ONBOARDING_EMPTY",
    "BROWSER_ONBOARDING_SINGLE", "BROWSER_ONBOARDING_MULTIPLE", "BROWSER_ONBOARDING_EXPLICIT",
})


@dataclass(frozen=True)
class BrowserTools:
    node: Path
    chromium: Path
    ipv6: bool = False


def explicit_executable(path):
    path = Path(path)
    require(path.is_absolute(), "BROWSER_EXECUTABLE_MUST_BE_ABSOLUTE")
    require(path.is_file() and os.access(path, os.X_OK), "BROWSER_EXECUTABLE_UNAVAILABLE")
    return path.resolve(strict=True)


def browser_tools(options):
    """Filesystem checks only. In particular this does not probe/launch Chromium."""
    require(isinstance(options, BrowserTools), "BROWSER_TOOL_OPTIONS_INVALID")
    require(type(options.ipv6) is bool, "BROWSER_IPV6_OPTION_INVALID")
    node, chromium = explicit_executable(options.node), explicit_executable(options.chromium)
    require((PLAYWRIGHT / "package.json").is_file() and (PLAYWRIGHT / "index.mjs").is_file(),
            "BROWSER_PLAYWRIGHT_UNAVAILABLE")
    package = json.loads((PLAYWRIGHT / "package.json").read_bytes())
    require(package.get("name") == "playwright" and
            re.fullmatch(r"\d+\.\d+\.\d+", package.get("version", "")), "BROWSER_PLAYWRIGHT_INVALID")
    return BrowserTools(node, chromium, ipv6=options.ipv6)


def browser_spec(tools, ports, session, request_id, fixture):
    require(len(ports) == 3 and len(set(ports)) == 3 and
            all(type(port) is int and 0 < port < 65536 for port in ports), "BROWSER_PORTS_INVALID")
    require(re.fullmatch(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}", session),
            "BROWSER_SESSION_INVALID")
    require(type(request_id) is int and 0 < request_id < 2 ** 53, "BROWSER_REQUEST_INVALID")
    return {"schema_version": 1, "ports": ports, "session": session, "request_id": request_id,
            "chromium": str(tools.chromium), "playwright": str(PLAYWRIGHT / "index.mjs"),
            "request_body": fixture.body.decode(), "text": TEXT, "tool_name": TOOL_NAME,
            "tool_result": TOOL_RESULT}


def validate_browser_report(raw):
    """Reconstruct an allowlisted report; never retain browser output or exception text."""
    require(len(raw) <= 16384, "BROWSER_REPORT_TOO_LARGE")
    value = json.loads(raw)
    require(isinstance(value, dict) and type(value.get("schema_version")) is int and
            value["schema_version"] == 1 and value.get("qualified") is False, "BROWSER_REPORT_SCHEMA")
    if value.get("state") == "FAILED":
        code = value.get("code")
        raise JourneyError(code if isinstance(code, str) and code in FAILURE_CODES else "BROWSER_CHECK_FAILED")
    fields = {"schema_version", "qualified", "state", "checks", "packaged_document_count",
              "browser_version", "websocket_frame_count", "script_response_count"}
    require(set(value) == fields and value["state"] == "PASSED", "BROWSER_REPORT_SCHEMA")
    require(isinstance(value["checks"], dict) and set(value["checks"]) == set(CHECKS) and
            all(item is True for item in value["checks"].values()), "BROWSER_CHECK_SET")
    require(type(value["packaged_document_count"]) is int and value["packaged_document_count"] == 4,
            "BROWSER_DOCUMENT_COUNT")
    for name in ("websocket_frame_count", "script_response_count"):
        require(type(value[name]) is int and 0 < value[name] <= 4096, "BROWSER_OBSERVATION_COUNT")
    require(isinstance(value["browser_version"], str) and
            re.fullmatch(r"\d{1,4}(?:\.\d{1,6}){1,4}", value["browser_version"]), "BROWSER_VERSION_INVALID")
    return {"kind": "INSTALLED_CHROMIUM_PARTIAL_BROWSER", "qualified": False,
            "checks": {name: True for name in CHECKS}, "browser_version": value["browser_version"],
            "packaged_document_count": 4, "websocket_frame_count": value["websocket_frame_count"],
            "script_response_count": value["script_response_count"],
            "synthetic_provider_only": True, "fresh_browser_context": True,
            "chromium_sandbox_requested": True, "external_requests_blocked": True,
            "raw_browser_logs_retained": False,
            "http_transport": "owned_origin_passthrough_redirects_refused"}


def run_browser(tools, ports, session, request_id, fixture, home, cwd):
    spec = browser_spec(tools, ports, session, request_id, fixture)
    path = Path(home) / "browser-fixture.json"
    private_write(path, (json.dumps(spec) + "\n").encode())
    try:
        environment = runtime_environment(home)
        # Deliberately do not inherit NODE_OPTIONS, proxies, credentials, DISPLAY,
        # browser profiles, Playwright debug flags or the operator's PATH.
        raw = captured_run([str(tools.node), str(RUNNER), str(path)], cwd, environment, timeout=110)
        observation = validate_browser_report(raw)
        observation["runner_sha256"] = sha256(RUNNER)
        observation["helper_sha256"] = {"journey_browser_onboarding.mjs":
                                         sha256(RUNNER.with_name("journey_browser_onboarding.mjs"))}
        observation["exit_status"] = 0
        return observation
    finally:
        path.unlink(missing_ok=True)
