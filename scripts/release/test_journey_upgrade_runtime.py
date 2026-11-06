"""Mocked capture/lifecycle contracts; no Mike, control socket or database starts."""
from contextlib import ExitStack, contextmanager
import copy
import hashlib
import json
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

from journey import new_result
from journey_fixture import MODEL, TEXT, TOOL_NAME
from journey_process import JourneyError
from journey_reliability_fixture import Cohort
from journey_upgrade_runtime import Runtime, capture_evidence, predecessor_backup


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.fixture = Cohort(1).prepare(0)[0]
        self.detail = {"id": 7, "session_id": self.fixture.session, "status": "Complete", "is_streaming": True,
            "response_status": 200, "completed_at": "synthetic", "model": MODEL, "stop_reason": "tool_use",
            "token_usage": {"input_tokens": 25, "output_tokens": 12},
            "tool_calls": [{"name": TOOL_NAME, "complete": True, "input_json": '{"echo":"synthetic"}'}]}
        response = json.dumps({"id": self.fixture.message_id, "type": "message", "role": "assistant",
            "model": MODEL, "stop_reason": "tool_use", "stop_sequence": None,
            "content": [{"type": "text", "text": TEXT}, {"type": "tool_use", "id": self.fixture.tool_use_id,
                        "name": TOOL_NAME, "input": {"echo": "synthetic"}}],
            "usage": {"input_tokens": 25, "output_tokens": 12}}).encode()
        self.bodies = [{"has_body": True, "text": body.decode(), "byte_size": len(body), "reconstructed": False}
                       for body in (self.fixture.body, response)]
        self.expected = {"request_id": 7, "session_id": self.fixture.session, "tool_use_id": self.fixture.tool_use_id,
            "request_body_sha256": hashlib.sha256(self.fixture.body).hexdigest(),
            "response_body_sha256": hashlib.sha256(response).hexdigest()}

    def test_old_capture_requires_same_request_session_tool_and_both_exact_hashes(self):
        with patch("journey_upgrade_runtime.http_json", side_effect=[self.detail, *self.bodies]):
            self.assertEqual(capture_evidence(12001, self.fixture, self.expected), self.expected)

    def test_missing_reconstructed_or_changed_body_session_and_hash_all_fail(self):
        altered = []
        for body_index, key, value in ((0, "has_body", False), (0, "reconstructed", True),
                                      (1, "text", "{}"), (0, "byte_size", True)):
            bodies = copy.deepcopy(self.bodies)
            bodies[body_index][key] = value
            altered.append(([self.detail, *bodies], self.expected))
        altered.append(([self.detail | {"session_id": "foreign"}, *self.bodies], self.expected))
        altered.append(([self.detail, *self.bodies], self.expected | {"request_body_sha256": "f" * 64}))
        for responses, expected in altered:
            with self.subTest(expected=expected), patch("journey_upgrade_runtime.http_json", side_effect=responses), \
                    self.assertRaises(JourneyError):
                capture_evidence(12001, self.fixture, expected)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.report = new_result({"manifest_sha256": "b" * 64})
        self.runtime = Runtime(self.report, Mock(), "owned", Path("/owned/config"), Path("/owned/home"),
                               Path("/owned/cwd"), [12001, 12002, 12003], 12004, Mock())
        self.archive = Mock(paths={"mike": Path("/owned/mike"), "mike-mcp": Path("/owned/mike-mcp")},
                            manifest={"source_commit": "a" * 40, "version": "0.1.0"})
        self.events = []

    @contextmanager
    def mocked(self, *, ready_error=False):
        @contextmanager
        def child(args, cwd, environment):
            self.events.append("start")
            self.assertEqual(args[-1], "serve")
            self.assertEqual(environment["PATH"], "")
            try:
                yield Mock()
            finally:
                self.events.append("owned-cleanup")
        with ExitStack() as stack:
            stack.enter_context(patch("journey_upgrade_runtime.owned_child", child))
            ready = stack.enter_context(patch.object(self.runtime, "ready", return_value={"owned": True},
                side_effect=JourneyError("TEST_READINESS_FAILED") if ready_error else None))
            stack.enter_context(patch("journey_upgrade_runtime.migration_ledger", return_value={"exact": True}))
            isolation = stack.enter_context(patch("journey_upgrade_runtime.disable_features", return_value={"disabled": True}))
            verify = stack.enter_context(patch("journey_upgrade_runtime.verify_features", return_value={"verified": True}))
            stop = stack.enter_context(patch("journey_upgrade_runtime.graceful_stop", side_effect=lambda _: self.events.append("stop")))
            yield isolation, verify, stop

    def test_initial_isolation_stops_owned_process_before_predecessor_capture(self):
        with self.mocked() as (isolation, verify, stop):
            self.runtime.serve(self.archive, "isolate", isolate=True)
        isolation.assert_called_once()
        verify.assert_not_called()
        self.assertEqual(self.events, ["start", "stop", "owned-cleanup"])
        self.assertTrue(all(item["state"] == "PASSED" for item in self.report["checks"]))

    def test_readiness_failure_cleans_owned_process_and_never_captures(self):
        with self.mocked(ready_error=True) as (isolation, verify, stop), self.assertRaisesRegex(JourneyError, "READINESS_FAILED"):
            self.runtime.serve(self.archive, "capture", capture=True)
        self.assertEqual(self.events, ["start", "owned-cleanup"])
        isolation.assert_not_called()
        verify.assert_not_called()
        stop.assert_not_called()
        self.assertEqual(self.report["state"], "FAILED")

    def test_same_generation_reuse_fails(self):
        health = {"instance": {"boot_generation": "same"}}
        self.runtime.generations.append("same")
        with patch("journey_upgrade_runtime.wait_owned_ready", return_value=health), \
                patch("journey_upgrade_runtime.verify_service_prerequisites"), self.assertRaisesRegex(JourneyError, "GENERATION_REUSED"):
            self.runtime.ready(Mock(), self.archive)

    def test_runtime_readiness_checks_compiled_disabled_embedding_and_absent_key(self):
        health = {"instance": {"boot_generation": "new"},
                  "embedding": {"url": "http://127.0.0.1:7997", "reachable": False,
                                "reachable_state": "not_configured"},
                  "llm": {"key_configured": False}}
        with patch("journey_upgrade_runtime.wait_owned_ready", return_value=health):
            observation = self.runtime.ready(Mock(), self.archive)
            self.assertTrue(observation["embedding_disabled"])
            self.assertFalse(observation["embedding_endpoint_owned"])
            self.assertFalse(observation["all_transport_isolation_verified"])
            health["instance"]["boot_generation"] = "next"
            health["embedding"]["reachable"] = True
            with self.assertRaisesRegex(JourneyError, "SERVICE_PREREQUISITES"):
                self.runtime.ready(Mock(), self.archive)


class BackupTests(unittest.TestCase):
    def setUp(self):
        self.kit = Mock()
        self.runtime = Mock(config=Path("/owned/r/config"))
        self.runtime.kit.return_value = self.kit
        self.previous = Mock(identity="a" * 64, manifest={"version": "0.1.0", "source_commit": "b" * 40})
        self.roots = {"config": Path("/owned/r/config"), "data": Path("/owned/r/data"), "state": Path("/owned/r/state")}
        self.saved = {"installed_manifest_sha256": "a" * 64, "version": "0.1.0", "source_commit": "b" * 40}
        self.ledger = [{"version": 1, "checksum": "a" * 96, "success": True}]
        self.index = {"files": [{"fixture": "files"}], "migration_ledger": self.ledger}
        self.enterContext(patch("journey_upgrade_runtime.runtime_roots", return_value=self.roots))
        self.snapshot = self.enterContext(patch("journey_upgrade_runtime.durable_snapshot", return_value={"complete": "same"}))
        self.enterContext(patch("journey_upgrade_runtime.verify", return_value=(self.saved, self.index)))
        self.enterContext(patch("journey_upgrade_runtime.inventory", return_value=self.index["files"]))
        self.enterContext(patch("journey_upgrade_runtime.read_ledger", return_value=self.ledger))

    def run_backup(self):
        return predecessor_backup(self.runtime, self.previous, Path("/owned/service"), Path("/owned/backup"))

    def test_backup_and_verify_both_use_predecessor_installed_kit_before_returning_roots(self):
        roots, observation = self.run_backup()
        self.assertEqual(roots, self.roots)
        self.assertTrue(observation["exact_predecessor_installed_kit"])
        self.assertEqual([call.args[0] for call in self.kit.command.call_args_list], ["backup", "verify"])

    def test_changed_source_manifest_or_backup_ledger_prevents_upgrade(self):
        self.saved["installed_manifest_sha256"] = "f" * 64
        with self.assertRaisesRegex(JourneyError, "BACKUP_BINDING"):
            self.run_backup()
        self.saved["installed_manifest_sha256"] = "a" * 64
        self.snapshot.side_effect = [{"complete": "before"}, {"complete": "mutated"}]
        with self.assertRaisesRegex(JourneyError, "BACKUP_CHANGED_SOURCE"):
            self.run_backup()


if __name__ == "__main__":
    unittest.main()
