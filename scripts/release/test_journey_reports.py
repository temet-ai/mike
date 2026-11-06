"""Offline diagnostic/MCP envelope regressions; no archive or network qualification."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from journey import create_config, doctor, validate_doctor
from journey_fixture import SECRET
from journey_mcp import Wire, notification, run_mcp
from journey_process import JourneyError


def doctor_report():
    statuses = {
        "configuration": (True, "pass", "CONFIG_LOADED"),
        "embedded_ui": (True, "pass", "UI_EMBEDDED"),
        "mcp_binary": (True, "pass", "MCP_BINARY_FOUND"),
        "mcp_connection": (False, "pass", "MCP_READY"),
        "claude": (False, "unavailable", "CLAUDE_ABSENT"),
        "docker": (False, "unavailable", "DOCKER_ABSENT"),
        "persistence": (False, "pass", "TRIAL_MODE"),
        "daemon": (False, "pass", "HEALTH_REPORTED_READY"),
    }
    return {"schema_version": 1, "product": "mike", "version": "0.0.1", "source_sha": "a" * 40,
            "source_dirty": False, "os": "linux", "architecture": "x86_64",
            "config_source": "selected_file", "read_only": True,
            "checks": [{"id": name, "required": required, "status": status, "code": code,
                        "detail": "Synthetic safe diagnostic metadata"}
                       for name, (required, status, code) in statuses.items()]}


class DoctorReportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-journey-doctor-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.config, self.home, self.cwd = create_config(self.root / "fixture", [10001, 10002, 10003], 10004)
        self.manifest = {"version": "0.0.1", "source_commit": "a" * 40, "target": "x86_64-unknown-linux-gnu"}

    def test_complete_ready_report_retains_only_hash_and_sanitized_command(self):
        raw = json.dumps(doctor_report()).encode()
        with patch("journey.captured_run", return_value=raw) as run:
            result = doctor(self.root / "mike", self.config, self.home, self.cwd, self.manifest)
        self.assertEqual(run.call_count, 2)
        self.assertEqual([item["mode"] for item in result["modes"]], ["--json", "--support-bundle"])
        self.assertTrue(all(item["exit_status"] == 0 for item in result["modes"]))
        self.assertNotIn(str(self.root), json.dumps(result))
        self.assertNotIn(SECRET, json.dumps(result))

    def test_stopped_trial_and_persistent_reports_require_backend_unavailable(self):
        for persistent in (False, True):
            value = doctor_report()
            for item in value["checks"]:
                if item["id"] == "daemon":
                    item.update(status="unavailable", code="DAEMON_NOT_REACHABLE")
                elif item["id"] == "mcp_connection":
                    item.update(status="unavailable", code="MCP_BACKEND_UNAVAILABLE")
                elif item["id"] == "persistence" and persistent:
                    item.update(required=True, code="DATABASE_CURRENT")
            with self.subTest(persistent=persistent):
                validate_doctor(value, self.manifest, persistent=persistent, stopped=True)
                with patch("journey.captured_run", return_value=json.dumps(value).encode()):
                    result = doctor(self.root / "mike", self.config, self.home, self.cwd, self.manifest,
                                    persistent=persistent, stopped=True)
                self.assertEqual(result["daemon_state"], "stopped")
                with self.assertRaisesRegex(JourneyError, "CHECK_FAILED"):
                    validate_doctor(value, self.manifest, persistent=persistent)
        value = doctor_report()
        with self.assertRaisesRegex(JourneyError, "CHECK_FAILED"):
            validate_doctor(value, self.manifest, stopped=True)

    def test_empty_missing_duplicate_or_unknown_checks_cannot_pass(self):
        for change in (lambda value: value.update(checks=[]),
                       lambda value: value["checks"].pop(),
                       lambda value: value["checks"].__setitem__(0, value["checks"][1]),
                       lambda value: value["checks"][0].update(id="unknown")):
            value = doctor_report()
            change(value)
            with self.subTest(value=value), self.assertRaisesRegex(JourneyError, "DOCTOR_CHECK_SET"):
                validate_doctor(value, self.manifest)

    def test_missing_and_extra_metadata_fields_are_refused(self):
        for field in ("checks", "source_sha", "schema_version"):
            value = doctor_report()
            value.pop(field)
            with self.subTest(field=field), self.assertRaisesRegex(JourneyError, "REPORT_SCHEMA"):
                validate_doctor(value, self.manifest)
        value = doctor_report()
        value["environment"] = {"unreviewed": "not an allowed metadata field"}
        with self.assertRaisesRegex(JourneyError, "REPORT_SCHEMA"):
            validate_doctor(value, self.manifest)

    def test_foreign_dirty_wrong_version_or_platform_report_is_refused(self):
        for key, changed in (("source_sha", "b" * 40), ("source_dirty", True), ("version", "9.9.9"),
                             ("os", "macos"), ("architecture", "aarch64"), ("config_source", "defaults")):
            value = doctor_report()
            value[key] = changed
            with self.subTest(key=key), self.assertRaises(JourneyError):
                validate_doctor(value, self.manifest)

    def test_failed_optional_ready_or_malformed_check_is_not_vacuous_success(self):
        for key, changed in (("required", 1), ("required", False), ("status", "fail"),
                             ("code", "NOT_RUN"), ("detail", None)):
            value = doctor_report()
            value["checks"][0][key] = changed
            with self.subTest(key=key, changed=changed), self.assertRaises(JourneyError):
                validate_doctor(value, self.manifest)
        value = doctor_report()
        next(item for item in value["checks"] if item["id"] == "daemon").update(status="unavailable")
        with self.assertRaisesRegex(JourneyError, "CHECK_FAILED"):
            validate_doctor(value, self.manifest)

    def test_planted_content_and_changed_permissions_fail(self):
        value = doctor_report()
        value["checks"][0]["detail"] = SECRET
        with patch("journey.captured_run", return_value=json.dumps(value).encode()):
            with self.assertRaisesRegex(JourneyError, "PRIVATE_CONTENT_LEAK"):
                doctor(self.root / "mike", self.config, self.home, self.cwd, self.manifest)
        def changed_permissions(*_arguments):
            self.config.chmod(0o644)
            return json.dumps(doctor_report()).encode()
        with patch("journey.captured_run", side_effect=changed_permissions):
            with self.assertRaisesRegex(JourneyError, "CONFIG_PERMISSIONS"):
                doctor(self.root / "mike", self.config, self.home, self.cwd, self.manifest)


class McpEnvelopeTests(unittest.TestCase):
    def test_optional_deadline_caps_receive_without_changing_legacy_five_seconds(self):
        for deadline, expected in ((None, 15), (10.25, 10.25), (30, 15)):
            wire = Wire.__new__(Wire)
            wire.frames = 0
            wire.line = Mock(return_value={"jsonrpc": "2.0", "id": 1, "result": {}})
            with self.subTest(deadline=deadline), patch("journey_mcp.time.monotonic", return_value=10):
                self.assertEqual(wire.receive(1, deadline=deadline), {})
            wire.line.assert_called_once_with(expected)

    def test_call_forwards_deadline_and_refuses_expired_or_invalid_before_send(self):
        wire = Wire.__new__(Wire)
        wire.sequence = 0
        wire.send, wire.receive = Mock(), Mock(return_value={})
        with patch("journey_mcp.time.monotonic", return_value=10):
            self.assertEqual(wire.call("tools/list", {}, deadline=10.5), {})
            wire.receive.assert_called_once_with(1, deadline=10.5)
            wire.send.reset_mock()
            for deadline in (10, 9, True, float("nan"), float("inf")):
                with self.subTest(deadline=deadline), self.assertRaises(JourneyError):
                    wire.call("tools/list", {}, deadline=deadline)
                wire.send.assert_not_called()

    def test_buffered_response_cannot_pass_after_absolute_deadline(self):
        wire = Wire.__new__(Wire)
        wire.frames = 0
        wire.buffer = bytearray(b'{"jsonrpc":"2.0","id":1,"result":{}}\n')
        with patch("journey_mcp.time.monotonic", side_effect=[0, 0.1, 0.6]), \
             self.assertRaisesRegex(JourneyError, "MCP_FRAME_TIMEOUT"):
            wire.receive(1, deadline=0.5)

    def test_only_structurally_valid_notifications_are_allowed(self):
        notification({"jsonrpc": "2.0", "method": "notifications/message", "params": {"level": "info"}})
        for value in ({"jsonrpc": "2.0"}, {"jsonrpc": "2.0", "method": None},
                      {"jsonrpc": "2.0", "method": "notice", "params": "not structured"},
                      {"jsonrpc": "2.0", "method": "notice", "id": 1},
                      {"jsonrpc": "2.0", "method": "notice", "result": {}},
                      {"jsonrpc": "2.0", "method": "notice", "error": {}}, []):
            with self.subTest(value=value), self.assertRaises(JourneyError):
                notification(value)

    def test_boolean_response_id_and_notification_response_mix_are_refused(self):
        valid = {"jsonrpc": "2.0", "id": 1, "result": {}}
        for change in ({"id": True}, {"id": "1"}, {"id": 2}, {"method": "notice"}, {"result": []}):
            wire = Wire.__new__(Wire)
            wire.frames = 0
            wire.line = Mock(return_value=valid | change)
            with self.subTest(change=change), self.assertRaises(JourneyError):
                wire.receive(1)

    def test_valid_json_but_invalid_trailing_envelope_fails_real_stdio_helper(self):
        # This fixture exchanges only local pipes. The inspect URL is never contacted.
        from test_journey import JourneyTests
        fixture = JourneyTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        binary = fixture.mcp_fixture("print(json.dumps({'jsonrpc':'2.0'}),flush=True)\n")
        with self.assertRaisesRegex(JourneyError, "MCP_INVALID_NOTIFICATION"):
            run_mcp(binary, 10001, 1, "session", "0.0.1", fixture.root, fixture.root)


if __name__ == "__main__":
    unittest.main()
