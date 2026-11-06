"""Offline browser-harness contracts. No Chromium launch or candidate qualification."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import acceptance
import journey
import journey_browser as browser
import journey_browser_installed as installed
from journey_fixture import Fixture, SECRET
from journey_process import JourneyError


def passed_report():
    return {"schema_version": 1, "state": "PASSED", "qualified": False,
            "checks": {name: True for name in browser.CHECKS}, "packaged_document_count": 4,
            "browser_version": "142.0.1234.5", "websocket_frame_count": 10,
            "script_response_count": 24}


class BrowserContractTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-browser-harness-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = Fixture()
        self.tools = browser.BrowserTools(Path(sys.executable), Path(sys.executable))

    def test_tooling_validation_never_launches_or_discovers_a_browser(self):
        package = self.root / "playwright"
        package.mkdir()
        (package / "index.mjs").write_text("// Synthetic dependency; never executed\n")
        (package / "package.json").write_text(json.dumps({"name": "playwright", "version": "1.62.1"}))
        with patch.object(browser, "PLAYWRIGHT", package), patch.object(browser, "captured_run") as launch:
            tools = browser.browser_tools(self.tools)
            self.assertEqual(tools.node, Path(sys.executable).resolve())
            launch.assert_not_called()
            self.assertTrue(browser.browser_tools(browser.BrowserTools(
                Path(sys.executable), Path(sys.executable), ipv6=True)).ipv6)
            with self.assertRaisesRegex(JourneyError, "IPV6_OPTION"):
                browser.browser_tools(browser.BrowserTools(Path(sys.executable), Path(sys.executable), ipv6=1))
            with self.assertRaisesRegex(JourneyError, "ABSOLUTE"):
                browser.browser_tools(browser.BrowserTools(Path("node"), Path(sys.executable)))
            with self.assertRaisesRegex(JourneyError, "UNAVAILABLE"):
                browser.browser_tools(browser.BrowserTools(self.root / "absent", Path(sys.executable)))
            (package / "index.mjs").unlink()
            with self.assertRaisesRegex(JourneyError, "PLAYWRIGHT_UNAVAILABLE"):
                browser.browser_tools(self.tools)
            launch.assert_not_called()

    def test_spec_is_limited_to_three_owned_ports_and_exact_capture(self):
        spec = browser.browser_spec(self.tools, [11001, 11002, 11003], self.fixture.session, 42, self.fixture)
        self.assertEqual(spec["session"], self.fixture.session)
        self.assertEqual(spec["request_id"], 42)
        self.assertEqual(spec["request_body"].encode(), self.fixture.body)
        for ports in ([80, 80, 81], [True, 80, 81], [0, 80, 81], [80, 81, 65536], [80, 81]):
            with self.subTest(ports=ports), self.assertRaisesRegex(JourneyError, "PORTS"):
                browser.browser_spec(self.tools, ports, self.fixture.session, 42, self.fixture)
        for session in ("http://foreign.test/", "../other", "session"):
            with self.subTest(session=session), self.assertRaisesRegex(JourneyError, "SESSION"):
                browser.browser_spec(self.tools, [80, 81, 82], session, 42, self.fixture)
        for request_id in (True, 0, -1, 2 ** 53, "42"):
            with self.subTest(request_id=request_id), self.assertRaisesRegex(JourneyError, "REQUEST"):
                browser.browser_spec(self.tools, [80, 81, 82], self.fixture.session, request_id, self.fixture)

    def test_success_is_partial_and_allowlisted(self):
        value = browser.validate_browser_report(json.dumps(passed_report()).encode())
        self.assertFalse(value["qualified"])
        self.assertEqual(value["kind"], "INSTALLED_CHROMIUM_PARTIAL_BROWSER")
        self.assertTrue(value["chromium_sandbox_requested"])
        self.assertFalse(value["raw_browser_logs_retained"])
        self.assertNotIn(self.fixture.session, json.dumps(value))

    def test_missing_failed_extra_and_nonboolean_checks_cannot_pass(self):
        for edit in (lambda value: value["checks"].pop(browser.CHECKS[0]),
                     lambda value: value["checks"].update(unreviewed=True),
                     lambda value: value["checks"].update(capture_request_dom=False),
                     lambda value: value["checks"].update(capture_request_dom=1),
                     lambda value: value.update(qualified=True),
                     lambda value: value.update(raw_console=SECRET),
                     lambda value: value.update(websocket_frame_count=0),
                     lambda value: value.update(script_response_count=4097),
                     lambda value: value.update(packaged_document_count=True),
                     lambda value: value.update(browser_version=SECRET)):
            value = passed_report()
            edit(value)
            with self.subTest(value=value), self.assertRaises(JourneyError):
                browser.validate_browser_report(json.dumps(value).encode())

    def test_failure_reports_retain_only_static_codes(self):
        for supplied, expected in (("BROWSER_LAUNCH_UNAVAILABLE", "BROWSER_LAUNCH_UNAVAILABLE"),
                                   (SECRET, "BROWSER_CHECK_FAILED"), ({"private": SECRET}, "BROWSER_CHECK_FAILED")):
            value = {"schema_version": 1, "state": "FAILED", "qualified": False,
                     "code": supplied, "raw_error": SECRET}
            with self.subTest(supplied=supplied), self.assertRaisesRegex(JourneyError, expected) as caught:
                browser.validate_browser_report(json.dumps(value).encode())
            self.assertNotIn(SECRET, str(caught.exception))
        with self.assertRaisesRegex(JourneyError, "TOO_LARGE"):
            browser.validate_browser_report(b" " * 16385)

    def test_launch_uses_private_spec_owned_process_deadline_and_clean_environment(self):
        def fixture_runner(args, cwd, environment, timeout):
            self.assertEqual(args[:2], [str(self.tools.node), str(browser.RUNNER)])
            spec_path = Path(args[2])
            self.assertEqual(spec_path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads(spec_path.read_text())["ports"], [11001, 11002, 11003])
            self.assertEqual(cwd, self.root)
            self.assertEqual(environment["PATH"], "")
            self.assertNotIn("NODE_OPTIONS", environment)
            self.assertNotIn("HTTP_PROXY", environment)
            self.assertNotIn("PLAYWRIGHT_BROWSERS_PATH", environment)
            self.assertEqual(timeout, 110)
            return json.dumps(passed_report()).encode()
        with patch.object(browser, "captured_run", side_effect=fixture_runner), patch.dict(os.environ, {
                "NODE_OPTIONS": "private", "HTTP_PROXY": "private", "PLAYWRIGHT_BROWSERS_PATH": "private"}):
            result = browser.run_browser(self.tools, [11001, 11002, 11003], self.fixture.session,
                                         42, self.fixture, self.root, self.root)
        self.assertFalse((self.root / "browser-fixture.json").exists())
        self.assertRegex(result["runner_sha256"], r"^[a-f0-9]{64}$")
        self.assertNotIn(str(self.root), json.dumps(result))

    def test_spec_cleanup_on_timeout_and_invalid_output(self):
        for output in (JourneyError("CHILD_TIMEOUT"), b"not JSON " + SECRET.encode()):
            target = {"side_effect": output} if isinstance(output, Exception) else {"return_value": output}
            with self.subTest(output=type(output).__name__), patch.object(browser, "captured_run", **target):
                with self.assertRaises((JourneyError, json.JSONDecodeError)):
                    browser.run_browser(self.tools, [11001, 11002, 11003], self.fixture.session,
                                        42, self.fixture, self.root, self.root)
            self.assertFalse((self.root / "browser-fixture.json").exists())

    def test_trial_journey_has_no_browser_hook_or_tooling_dependency(self):
        from test_journey import JourneyTests
        fixture = JourneyTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        binary_dir, manifest, digest = fixture.binary_fixture()
        with patch.object(journey, "run_bound") as run, patch.object(browser, "browser_tools") as tooling:
            result = journey.run_archive_journey(binary_dir, manifest, self.root, digest)
        tooling.assert_not_called()
        self.assertEqual(len(run.call_args.args), 4)
        self.assertEqual(run.call_args.kwargs, {})
        self.assertFalse(any('browser' in item['id'] for item in result['checks']))
        self.assertFalse(result["qualified"])

    def test_bad_tooling_stops_before_any_owned_runtime_is_started(self):
        with patch.object(installed, "verify_binding", return_value=({}, {})), patch.object(installed, "run_bound") as run:
            result = installed.run_installed_browser_journey(self.root, {}, self.root, "a" * 64,
                                                browser=browser.BrowserTools(Path("node"), Path("chromium")))
        run.assert_not_called()
        self.assertEqual(result["failure_code"], "BROWSER_EXECUTABLE_MUST_BE_ABSOLUTE")
        self.assertFalse(result["qualified"])

    def test_cli_requires_explicit_opt_in_and_persistence(self):
        base = ["release-acceptance", "--candidate", "/tmp/candidate", "--output", "/tmp/evidence"]
        with patch.object(sys, "argv", base), patch.object(acceptance, "run_acceptance", return_value=2) as run:
            self.assertEqual(acceptance.main(), 2)
            self.assertIsNone(run.call_args.args[-1])
        for flags in (["--browser-journey"], ["--browser-ipv6"], ["--browser-node", "/bin/node"],
                      ["--browser-journey", "--browser-node", "/bin/node", "--browser-chromium", "/bin/chromium"]):
            with self.subTest(flags=flags), patch.object(sys, "argv", base + flags), patch('sys.stderr'), \
                    patch.object(acceptance, "run_acceptance") as run, self.assertRaises(SystemExit) as caught:
                acceptance.main()
            self.assertEqual(caught.exception.code, 2)
            run.assert_not_called()
        flags = ["--persistent-journey", "--browser-journey", "--browser-node", "/bin/node",
                 "--browser-chromium", "/bin/chromium"]
        with patch.object(sys, "argv", base + flags), patch.object(acceptance, "run_acceptance", return_value=2) as run:
            acceptance.main()
            self.assertEqual(run.call_args.args[-1], browser.BrowserTools(Path('/bin/node'), Path('/bin/chromium')))
        with patch.object(sys, 'argv', base + flags + ['--browser-ipv6']), \
                patch.object(acceptance, 'run_acceptance', return_value=2) as run:
            acceptance.main()
            self.assertTrue(run.call_args.args[-1].ipv6)

    def test_programmatic_browser_without_persistence_refused_before_candidate_reads(self):
        with patch.object(acceptance, "verified_inputs") as read:
            with self.assertRaisesRegex(Exception, "requires the owned persistent"):
                acceptance.run_helpers(self.root, acceptance.new_report(), browser=self.tools)
        read.assert_not_called()

    @unittest.skipUnless(shutil.which("node"), "existing Node unavailable; no download attempted")
    def test_node_offline_contracts(self):
        result = subprocess.run([shutil.which("node"), "--test", browser.RUNNER.with_name("journey_browser_contracts.mjs")],
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode())

    @unittest.skipUnless(shutil.which("node"), "existing Node unavailable; no download attempted")
    def test_node_syntax_and_invalid_spec_without_import_or_browser_launch(self):
        node = shutil.which("node")
        result = subprocess.run([node, "--check", browser.RUNNER], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        spec = self.root / "invalid.json"
        spec.write_text(json.dumps({"private": SECRET}))
        result = subprocess.run([node, browser.RUNNER, spec], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stdout), {"schema_version": 1, "state": "FAILED", "qualified": False,
                                                    "code": "BROWSER_SPEC_INVALID"})
        self.assertNotIn(SECRET.encode(), result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
