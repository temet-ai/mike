#!/usr/bin/env python3
"""Verifier helper tests. These synthetic tests never qualify a candidate archive."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from common import sha256
from journey import check, create_config, new_result, run_archive_journey, verify_binding
from journey_fixture import MODEL, TOOL_NAME, upstream_fixture
from journey_http import asset_paths, stream_capture, validate_detail
from journey_mcp import content_json, run_mcp
from journey_process import (JourneyError, captured_run, installation_identity, require,
                             runtime_environment, validate_identity)


class JourneyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="mike-journey-helper-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def binary_fixture(self):
        installed = self.root / "installed"
        installed.mkdir()
        artifacts = []
        for name in ("mike", "mike-mcp"):
            binary = installed / name
            binary.write_bytes(b"synthetic helper fixture, never executed")
            binary.chmod(0o700)
            artifacts.append({"binary": name, "sha256": "a" * 64,
                              "contents": [{"path": name, "sha256": sha256(binary)}]})
        manifest = {"version": "0.0.1", "source_commit": "b" * 40,
                    "target": "x86_64-unknown-linux-gnu", "artifacts": artifacts}
        path = installed / "manifest.json"
        path.write_text(json.dumps(manifest))
        return installed, manifest, sha256(path)

    def test_binding_checks_exact_installed_bytes_and_manifest(self):
        installed, manifest, digest = self.binary_fixture()
        paths, binding = verify_binding(installed, manifest, digest)
        self.assertEqual(set(paths), {"mike", "mike-mcp"})
        self.assertEqual(binding["manifest_sha256"], digest)
        (installed / "mike").write_bytes(b"changed")
        with self.assertRaisesRegex(JourneyError, "BINARY_HASH"):
            verify_binding(installed, manifest, digest)
        with self.assertRaisesRegex(JourneyError, "MANIFEST_MISMATCH"):
            verify_binding(installed, manifest, "0" * 64)

    def test_hash_failure_never_executes_or_qualifies(self):
        installed, manifest, digest = self.binary_fixture()
        with patch("journey.run_bound") as run:
            result = run_archive_journey(installed, manifest, self.root, "f" * 64)
        run.assert_not_called()
        self.assertEqual(result["state"], "FAILED")
        self.assertFalse(result["qualified"])
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in result["full_cases"]))

    def test_partial_passes_do_not_change_full_case_states(self):
        result = new_result({"manifest_sha256": "0" * 64})
        check(result, "helper", lambda: {"observation": True})
        self.assertEqual(result["checks"][0]["state"], "PASSED")
        self.assertEqual(result["state"], "INCOMPLETE")
        self.assertFalse(result["qualified"])
        self.assertEqual(len(result["full_cases"]), 27)
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in result["full_cases"]))

    def test_exception_messages_and_planted_content_are_not_retained(self):
        result = new_result({})
        with self.assertRaises(ValueError):
            check(result, "helper", lambda: (_ for _ in ()).throw(ValueError("PRIVATE_SENTINEL")))
        self.assertNotIn("PRIVATE_SENTINEL", json.dumps(result))
        self.assertEqual(result["checks"][0]["error_type"], "ValueError")

    def test_runtime_environment_replaces_operator_settings(self):
        with patch.dict(os.environ, {"MIKE_SMOKE_RUN": "true", "MIKE_DATABASE_URL": "private", "PATH": "private"}):
            environment = runtime_environment(self.root)
        self.assertEqual(environment["PATH"], "")
        self.assertNotIn("MIKE_SMOKE_RUN", environment)
        self.assertNotIn("MIKE_DATABASE_URL", environment)

    def test_config_is_private_explicit_trial_and_has_no_smoke_override(self):
        config, home, cwd = create_config(self.root / "journey", [10001, 10002, 10003], 10004)
        self.assertEqual(config.stat().st_mode & 0o777, 0o600)
        self.assertIn("persist_enabled = false", config.read_text())
        self.assertNotIn("smoke", config.read_text())
        self.assertTrue((home / ".env").is_file() and (cwd / ".env").is_file())

    def test_identity_rejects_foreign_pid_generation_or_persistence(self):
        expected, path = installation_identity(self.root / "config.toml")
        self.assertEqual(expected, hashlib.sha256(os.fsencode(self.root / "config.toml")).hexdigest()[:32])
        self.assertEqual(path.name, "instance.json")
        record = {"pid": 42, "instance_id": expected, "generation": "a-generation"}
        health = {"instance": {"instance_id": expected, "boot_generation": "a-generation"},
                  "persistence": {"enabled": False}, "build": {"sha": "b" * 40}}
        validate_identity(record, health, expected, 42, "b" * 40)
        with self.assertRaises(JourneyError):
            validate_identity(record, health, expected, 43, "b" * 40)
        health["instance"]["boot_generation"] = "foreign"
        with self.assertRaises(JourneyError):
            validate_identity(record, health, expected, 42, "b" * 40)

    def test_synthetic_stream_fixture_requires_first_event_before_release(self):
        with upstream_fixture() as (fixture, port):
            result = stream_capture(port, fixture)
        self.assertTrue(result["first_event_before_release"])
        self.assertTrue(result["response_order_identical"])
        self.assertEqual(result["event_count"], 9)

    def test_asset_discovery_requires_local_js_and_css(self):
        html = '<link rel="stylesheet" href="./_app/style.css"><script>import("./_app/start.js")</script>'
        self.assertEqual(asset_paths(html), ["/_app/start.js", "/_app/style.css"])
        with self.assertRaisesRegex(JourneyError, "EXTERNAL_ASSET"):
            asset_paths('<script src="https://example.test/app.js"></script>')
        with self.assertRaisesRegex(JourneyError, "STYLESHEET_MISSING"):
            asset_paths('<script src="/_app/start.js"></script>')

    def test_capture_metadata_requires_tool_usage_and_terminal_state(self):
        fixture = type("Fixture", (), {"session": "synthetic-session"})()
        detail = {"id": 1, "session_id": fixture.session, "status": "Complete", "is_streaming": True,
                  "response_status": 200, "completed_at": "fixture", "model": MODEL, "stop_reason": "tool_use",
                  "token_usage": {"input_tokens": 25, "output_tokens": 12},
                  "tool_calls": [{"name": TOOL_NAME, "complete": True, "input_json": '{"echo":"synthetic"}'}]}
        validate_detail(detail, fixture, 1)
        detail["token_usage"]["output_tokens"] = 0
        with self.assertRaisesRegex(JourneyError, "CAPTURE_USAGE"):
            validate_detail(detail, fixture, 1)

    def test_mcp_error_content_is_not_a_success(self):
        with self.assertRaisesRegex(JourneyError, "MCP_TOOL_FAILURE"):
            content_json({"isError": True, "content": [{"type": "text", "text": "private failure"}]})

    def test_timeout_preserves_a_foreign_process(self):
        foreign = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"], start_new_session=True)
        try:
            with self.assertRaisesRegex(JourneyError, "CHILD_TIMEOUT"):
                captured_run([sys.executable, "-c", "import time; time.sleep(30)"], self.root,
                             runtime_environment(self.root), timeout=0.05)
            self.assertIsNone(foreign.poll())
        finally:
            foreign.terminate()
            foreign.wait(timeout=3)

    def mcp_fixture(self, extra=""):
        binary = self.root / "mcp-fixture"
        binary.write_text(f"#!{sys.executable}\n" + '''import json,sys
if 'list-tools' in sys.argv:
 print(json.dumps([{'name':'get_request'}])); sys.exit(0)
for line in sys.stdin:
 message=json.loads(line)
 if 'id' not in message: continue
 method=message['method']
 if method=='initialize':
  result={'serverInfo':{'name':'mike-mcp','version':'0.0.1'},'protocolVersion':'2024-11-05','capabilities':{'tools':{}}}
 elif method=='tools/list': result={'tools':[{'name':'get_request'}]}
 else: result={'content':[{'type':'text','text':json.dumps({'request':{'id':1,'session_id':'session','status':'Complete'}})}]}
 print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}),flush=True)
''' + extra)
        binary.chmod(0o700)
        return binary

    def test_real_stdio_client_checks_profile_and_requested_capture(self):
        result = run_mcp(self.mcp_fixture(), 10001, 1, "session", "0.0.1", self.root, self.root)
        self.assertEqual(result["tool_count"], 1)
        self.assertTrue(result["matching_capture"])

    def test_stdio_rejects_nonprotocol_output_at_shutdown(self):
        with self.assertRaises((JourneyError, json.JSONDecodeError)):
            run_mcp(self.mcp_fixture("print('unframed log',flush=True)\n"), 10001, 1, "session", "0.0.1", self.root, self.root)

class AcceptanceJourneyWiringTests(unittest.TestCase):
    def test_opt_in_runtime_failure_stays_unqualified_and_nonzero(self):
        import test_install
        fixture = test_install.InstallTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        output = fixture.base / "journey-evidence"
        result = subprocess.run([test_install.ROOT / "scripts/release-acceptance", "--candidate",
                                 fixture.candidate, "--output", output, "--trial-journey"],
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 1)
        report = json.loads((output / "result.json").read_bytes())
        self.assertFalse(report["qualified"])
        self.assertEqual(report["state"], "FAILED")
        self.assertEqual(report["required_not_run"], 6)
        self.assertFalse(report["archive_trial_journey"]["qualified"])
        self.assertTrue(any(item["state"] == "FAILED" for item in report["checks"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
