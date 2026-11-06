"""Offline contracts for opt-in restart helpers; no daemon, sockets or database.

Mocked observations test refusal/report orchestration only. They are never candidate
evidence and cannot promote full AC02/AC07 or any other acceptance case.
"""
from contextlib import ExitStack, contextmanager
import copy
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from integration_recovery_fixture import Cluster
from journey import create_config, new_result, validate_doctor
from journey_fixture import Fixture, MODEL, TEXT, TOOL_NAME
from journey_persistent import (graceful_stop, migration_ledger, owned_cluster,
                                run_bound, run_persistent_journey, stored_capture, stored_search,verified_migration_inventory)
from journey_process import JourneyError, validate_identity
from test_journey_reports import doctor_report
from migration_inventory import SOURCE_PATH


def ledger_rows():
    # Test data only; never sent to a PostgreSQL server or candidate binary.
    return [{"version": version, "description": "offline fixture", "installed_on": "2026-10-13T00:00:00Z",
             "success": True, "checksum": str(version) * 96, "execution_time": version}
            for version in (1, 2)]


def synthetic_inventory(count=2):
    return {"schema_version": 1, "source_commit": "b" * 40, "source_path": SOURCE_PATH,
            "checksum_algorithm": "sha384", "migrations": [
                {"version": row["version"], "filename": str(row["version"]) + "_offline_fixture.sql",
                 "checksum": row["checksum"]} for row in ledger_rows()[:count]]}


class PersistentFixtureTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-persistent-offline-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_missing_fixture_refused_before_any_cluster_or_runtime(self):
        with patch.dict(os.environ, {}, clear=True), patch("journey_persistent.Cluster") as cluster:
            with self.assertRaisesRegex(JourneyError, "FIXTURE_REQUIRED"):
                owned_cluster()
        cluster.assert_not_called()

    def test_wrong_fixture_url_is_refused_before_any_subprocess(self):
        urls = ["postgres://postgres@127.0.0.1:5432/postgres?sslmode=disable",
                "postgres://postgres@127.0.0.1:5440/postgres?sslmode=disable",
                "postgres://postgres:private@127.0.0.1:6432/postgres?sslmode=disable",
                "postgres://postgres@example.test:6432/postgres?sslmode=disable",
                "postgres://postgres@127.0.0.1:6432/operator?sslmode=disable",
                "postgres://postgres@127.0.0.1:6432/postgres?sslmode=disable&host=other"]
        for url in urls:
            with self.subTest(url=url), patch.dict(os.environ, {
                "MIKE_TEST_ADMIN_URL": url, "MIKE_RECOVERY_TEST_PGDATA": str(self.root)}, clear=True), \
                    patch("integration_recovery_fixture.subprocess.run") as command:
                with self.assertRaisesRegex(JourneyError, "FIXTURE_INVALID_OR_UNAVAILABLE"):
                    owned_cluster()
                command.assert_not_called()

    def test_nonprivate_symlink_or_wrong_major_pgdata_refused_offline(self):
        data = self.root / "pgdata"
        data.mkdir(mode=0o700)
        (data / "PG_VERSION").write_text("16\n")
        link = self.root / "link"
        link.symlink_to(data, target_is_directory=True)
        for selected, mode in ((data, 0o700), (data, 0o755), (link, 0o700)):
            data.chmod(mode)
            with self.subTest(selected=selected, mode=mode), patch.dict(os.environ, {
                "MIKE_TEST_ADMIN_URL": "postgres://postgres@127.0.0.1:6432/postgres?sslmode=disable",
                "MIKE_RECOVERY_TEST_PGDATA": str(selected)}, clear=True), \
                    patch("integration_recovery_fixture.subprocess.run") as command:
                with self.assertRaisesRegex(JourneyError, "FIXTURE_INVALID_OR_UNAVAILABLE"):
                    owned_cluster()
                command.assert_not_called()

    def test_connected_server_must_match_owned_tcp_only_pgdata(self):
        (self.root / "PG_VERSION").write_text("19\n")
        (self.root / "postmaster.pid").write_text("offline fixture, never used as a process id\n")
        for actual in (["190000", "/unrelated", "127.0.0.1", "", "postgres", "postgres"],
                       ["190000", str(self.root), "*", "", "postgres", "postgres"],
                       ["190000", str(self.root), "127.0.0.1", "/tmp", "postgres", "postgres"]):
            with self.subTest(actual=actual), patch.dict(os.environ, {
                "MIKE_TEST_ADMIN_URL": "postgres://postgres@127.0.0.1:6432/postgres?sslmode=disable",
                "MIKE_RECOVERY_TEST_PGDATA": str(self.root)}, clear=True), \
                    patch("integration_recovery_fixture.shutil.which", return_value="/offline/psql"), \
                    patch.object(Cluster, "command", return_value="psql (PostgreSQL) 19rc1"), \
                    patch.object(Cluster, "sql", return_value=json.dumps(actual)):
                with self.assertRaisesRegex(JourneyError, "FIXTURE_INVALID_OR_UNAVAILABLE"):
                    owned_cluster()

    def test_persistent_config_is_explicit_private_and_ignores_operator_env(self):
        url = "postgres://postgres@127.0.0.1:6432/owned_fixture?sslmode=disable"
        config, _, _ = create_config(self.root / "config", [10001, 10002, 10003], 10004, database_url=url)
        self.assertIn("persist_enabled = true", config.read_text())
        self.assertIn(url, config.read_text())
        self.assertEqual(config.stat().st_mode & 0o777, 0o600)


class PersistentObservationTests(unittest.TestCase):
    def test_successful_but_missing_extra_replaced_or_wrong_checksum_migrations_fail(self):
        cases = [ledger_rows()[:1], ledger_rows()[1:]]
        extra = ledger_rows() + [dict(ledger_rows()[1], version=3)]
        replaced = [dict(ledger_rows()[0], version=3), dict(ledger_rows()[1], version=4)]
        checksum = ledger_rows()
        checksum[0]["checksum"] = "f" * 96
        cases.extend((extra, replaced, checksum))
        for rows in cases:
            with self.subTest(rows=rows), self.assertRaisesRegex(JourneyError, "CANDIDATE_MISMATCH"):
                migration_ledger(Mock(sql=Mock(return_value=json.dumps(rows))), "owned", synthetic_inventory())

    def test_missing_or_malformed_inventory_fails_before_ledger_read(self):
        for inventory in (None, {}, [], dict(synthetic_inventory(), migrations=[])):
            cluster = Mock()
            with self.subTest(inventory=inventory), self.assertRaisesRegex(JourneyError, "INVENTORY_INVALID"):
                migration_ledger(cluster, "owned", inventory)
            cluster.sql.assert_not_called()

    def test_complete_successful_ledger_is_read_only_and_hashed_without_fixed_count(self):
        for count in (1, 2):
            cluster = Mock()
            cluster.sql.return_value = json.dumps(ledger_rows()[:count])
            evidence = migration_ledger(cluster, "owned", synthetic_inventory(count))
            self.assertEqual(evidence["migration_count"], count)
            self.assertEqual(evidence["failed_rows"], 0)
            self.assertTrue(evidence["exact_candidate_set"])
            self.assertRegex(evidence["candidate_inventory_sha256"], r"^[a-f0-9]{64}$")
            self.assertRegex(evidence["full_ledger_sha256"], r"^[a-f0-9]{64}$")
            self.assertTrue(cluster.sql.call_args.args[1].startswith("BEGIN READ ONLY;"))
            self.assertNotIn("offline fixture", json.dumps(evidence))

    def test_empty_failed_malformed_duplicate_and_out_of_order_ledgers_fail(self):
        cases = [[], {}, list(reversed(ledger_rows())), [ledger_rows()[0]] * 2]
        for key, value in (("success", False), ("success", 1), ("checksum", "short"),
                           ("version", True), ("installed_on", None), ("execution_time", -1)):
            rows = ledger_rows()
            rows[0][key] = value
            cases.append(rows)
        rows = ledger_rows()
        rows[0].pop("success")
        cases.append(rows)
        for rows in cases:
            with self.subTest(rows=rows), self.assertRaises(JourneyError):
                migration_ledger(Mock(sql=Mock(return_value=json.dumps(rows))), "owned", synthetic_inventory())

    def test_full_ledger_hash_changes_for_any_rewritten_field(self):
        rows = ledger_rows()
        original = migration_ledger(Mock(sql=Mock(return_value=json.dumps(rows))), "owned", synthetic_inventory())
        rows[0]["installed_on"] = "2026-10-14T00:00:00Z"
        rewritten = migration_ledger(Mock(sql=Mock(return_value=json.dumps(rows))), "owned", synthetic_inventory())
        self.assertNotEqual(original, rewritten)

    def test_persistent_doctor_requires_full_candidate_report_and_current_database(self):
        manifest = {"version": "0.0.1", "source_commit": "a" * 40, "target": "x86_64-unknown-linux-gnu"}
        value = doctor_report()
        with self.assertRaisesRegex(JourneyError, "DOCTOR_CHECK_FAILED"):
            validate_doctor(value, manifest, persistent=True)
        persistence = next(item for item in value["checks"] if item["id"] == "persistence")
        persistence.update(required=True, code="DATABASE_CURRENT")
        validate_doctor(value, manifest, persistent=True)
        for change in (lambda report: report["checks"].pop(),
                       lambda report: report.update(checks=[]),
                       lambda report: report["checks"].append(report["checks"][0]),
                       lambda report: report.update(source_sha="b" * 40),
                       lambda report: next(item for item in report["checks"] if item["id"] == "persistence")
                           .update(code="DATABASE_SCHEMA_NOT_CURRENT", status="fail")):
            altered = copy.deepcopy(value)
            change(altered)
            with self.assertRaises(JourneyError):
                validate_doctor(altered, manifest, persistent=True)

    def test_persistent_health_rejects_trial_unreachable_or_foreign_identity(self):
        record = {"pid": 42, "instance_id": "fixture", "generation": "boot-a"}
        health = {"instance": {"instance_id": "fixture", "boot_generation": "boot-a"},
                  "build": {"sha": "a" * 40}, "status": "ok",
                  "persistence": {"enabled": True, "reachable": True}}
        validate_identity(record, health, "fixture", 42, "a" * 40, persistent=True)
        for change in ({"enabled": False}, {"reachable": False}, {"enabled": 1}):
            altered = copy.deepcopy(health)
            altered["persistence"].update(change)
            with self.assertRaisesRegex(JourneyError, "PERSISTENT_READINESS"):
                validate_identity(record, altered, "fixture", 42, "a" * 40, persistent=True)
        with self.assertRaisesRegex(JourneyError, "IDENTITY"):
            validate_identity(record, health, "fixture", 43, "a" * 40, persistent=True)

    def test_graceful_stop_uses_only_owned_handle_and_requires_zero_exit(self):
        child = Mock(returncode=0)
        child.poll.return_value = None
        evidence = graceful_stop(child, timeout=0.1)
        child.signal_group.assert_called_once_with(signal.SIGTERM)
        child.wait.assert_called_once_with(timeout=0.1)
        self.assertEqual(evidence["installed_cli_tui_stop"], "NOT_RUN")
        child.returncode = -15
        with self.assertRaisesRegex(JourneyError, "STOP_NONZERO"):
            graceful_stop(child)
        child.wait.side_effect = subprocess.TimeoutExpired("offline fixture", 0.1)
        with self.assertRaisesRegex(JourneyError, "STOP_TIMEOUT"):
            graceful_stop(child, timeout=0.1)

    def capture_responses(self, fixture, request_id=7):
        detail = {"id": request_id, "session_id": fixture.session, "status": "Complete", "is_streaming": True,
                  "response_status": 200, "completed_at": "fixture", "model": MODEL, "stop_reason": "tool_use",
                  "token_usage": {"input_tokens": 25, "output_tokens": 12},
                  "tool_calls": [{"name": TOOL_NAME, "complete": True, "input_json": '{"echo":"synthetic"}'}]}
        response = json.dumps({"id": "msg_archive_fixture", "type": "message", "role": "assistant",
            "model": MODEL, "stop_reason": "tool_use", "stop_sequence": None,
            "content": [{"type": "text", "text": TEXT},
                        {"type": "tool_use", "id": "tool_archive_response", "name": TOOL_NAME,
                         "input": {"echo": "synthetic"}}],
            "usage": {"input_tokens": 25, "output_tokens": 12}}).encode()
        bodies = [{"text": body.decode(), "has_body": True, "byte_size": len(body)}
                  for body in (fixture.body, response)]
        return [[{"id": request_id, "status": "Complete"}], detail, *bodies]

    def test_stored_search_requires_the_captured_request_and_session(self):
        fixture = Fixture()
        match = {"request_id": 7, "session_id": fixture.session, "matched": TEXT}
        with patch("journey_persistent.http_json", return_value={"matches": [match]}):
            self.assertEqual(stored_search(10001, fixture, 7)["match_count"], 1)
        for found in ({"matches": [dict(match, request_id=8)]}, {"matches": [dict(match, session_id="other")]}):
            with patch("journey_persistent.http_json", return_value=found), self.assertRaises(JourneyError):
                stored_search(10001, fixture, 7)
        with patch("journey_persistent.http_json", return_value={"matches": []}), \
                patch("journey_persistent.time.sleep"), self.assertRaises(JourneyError):
            stored_search(10001, fixture, 7, timeout=0)

    def test_stored_capture_checks_exact_ids_and_both_body_hashes(self):
        fixture = Fixture()
        with patch("journey_persistent.http_json", side_effect=self.capture_responses(fixture)):
            evidence = stored_capture(10001, fixture, expected_id=7)
        self.assertEqual(evidence["session_id"], fixture.session)
        response_bytes = self.capture_responses(fixture)[3]["text"].encode()
        self.assertEqual(evidence["response_body_sha256"], hashlib.sha256(response_bytes).hexdigest())
        self.assertEqual(evidence["response_body_kind"], "stored_assembled_message_json")
        self.assertNotIn(fixture.body.decode(), json.dumps(evidence))
        cases = []
        for index in (2, 3):
            values = self.capture_responses(fixture)
            values[index]["text"] += "changed"
            cases.append(values)
        changed_session = self.capture_responses(fixture)
        changed_session[1]["session_id"] = "foreign"
        cases.extend([changed_session, self.capture_responses(fixture, request_id=8),
                      [[{"id": 7}, {"id": 8}]]])
        for field, changed in (("content", []), ("usage", {}), ("model", "foreign")):
            values = self.capture_responses(fixture)
            response = json.loads(values[3]["text"])
            response[field] = changed
            values[3]["text"] = json.dumps(response)
            values[3]["byte_size"] = len(values[3]["text"].encode())
            cases.append(values)
        for values in cases:
            with self.subTest(values=values), patch("journey_persistent.http_json", side_effect=values):
                with self.assertRaises(JourneyError):
                    stored_capture(10001, fixture, expected_id=7)

    def test_incomplete_capture_expires_without_a_false_success(self):
        with patch("journey_persistent.http_json", return_value=[]), \
                patch("journey_persistent.time.monotonic", side_effect=[0, 0, 2]), \
                patch("journey_persistent.time.sleep"):
            with self.assertRaisesRegex(JourneyError, "COMPLETION_TIMEOUT"):
                stored_capture(10001, Fixture(), timeout=1)


class PersistentSourceBindingTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-persistent-synthetic-identity-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.paths = {name: self.root / name for name in ("mike", "mike-mcp")}
        self.manifest = {"source_commit": "b" * 40, "source_date_epoch": 0, "version": "0.0.1",
                         "migration_inventory": synthetic_inventory(), "artifacts": []}
        for name in self.paths:
            identity = {"schema_version": 1, "product": name, "version": "0.0.1", "source_sha": "b" * 40,
                        "source_dirty": False, "built_at": "1970-01-01T00:00:00Z"}
            self.manifest["artifacts"].append({"binary": name, "build_info": identity})
            self.binary(name, identity)

    def binary(self, name, identity):
        # Deliberate shell fixture, never evidence of a real Mike executable.
        self.paths[name].write_text("#!/bin/sh\n[ \"$1\" = '--build-info' ] || exit 77\n"
                                    + "printf '%s\\n' '" + json.dumps(identity) + "'\n")
        self.paths[name].chmod(0o755)

    def test_both_exact_executables_bind_to_same_inventory_source(self):
        self.assertEqual(verified_migration_inventory(self.paths, self.manifest), synthetic_inventory())

    def test_manifest_cannot_replace_actual_binary_source_identity(self):
        for name in self.paths:
            expected = next(item["build_info"] for item in self.manifest["artifacts"] if item["binary"] == name)
            for update in ({"source_sha": "c" * 40}, {"source_dirty": True}, {"version": "9.9.9"}):
                self.binary(name, dict(expected, **update))
                with self.subTest(name=name, update=update), \
                        patch("journey_persistent.checked_migration_inventory") as inventory:
                    with self.assertRaisesRegex(JourneyError, "CANDIDATE_SOURCE_IDENTITY"):
                        verified_migration_inventory(self.paths, self.manifest)
                    inventory.assert_not_called()
                self.binary(name, expected)

    def test_missing_or_tampered_artifact_identity_and_foreign_inventory_fail(self):
        changes = [lambda manifest: manifest["artifacts"][0].pop("build_info"),
                   lambda manifest: manifest.update(artifacts=[]),
                   lambda manifest: manifest["artifacts"].append(manifest["artifacts"][0]),
                   lambda manifest: manifest["migration_inventory"].update(source_commit="c" * 40),
                   lambda manifest: manifest.pop("migration_inventory")]
        for change in changes:
            manifest = copy.deepcopy(self.manifest)
            change(manifest)
            with self.subTest(manifest=manifest), self.assertRaises(JourneyError):
                verified_migration_inventory(self.paths, manifest)


class PersistentRunnerTests(unittest.TestCase):
    def setUp(self):
        inventory = patch("journey_persistent.verified_migration_inventory", return_value=synthetic_inventory())
        self.verified_inventory = inventory.start()
        self.addCleanup(inventory.stop)

    def binding(self):
        return ({"mike": Path("/offline/mike"), "mike-mcp": Path("/offline/mike-mcp")},
                {"manifest_sha256": "a" * 64})

    def test_missing_fixture_is_sanitized_failure_and_all_full_cases_not_run(self):
        with patch("journey_persistent.verify_binding", return_value=self.binding()), \
                patch.dict(os.environ, {}, clear=True), patch("journey_persistent.run_bound") as run:
            report = run_persistent_journey("/offline", {}, "/offline", "a" * 64)
        run.assert_not_called()
        self.assertEqual(report["failure_code"], "PERSISTENT_FIXTURE_REQUIRED")
        self.assertEqual(report["state"], "FAILED")
        self.assertFalse(report["qualified"])
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["full_cases"]))

    def test_partial_success_never_qualifies_and_owned_database_cleanup_is_required(self):
        cluster = Mock()
        with patch("journey_persistent.verify_binding", return_value=self.binding()), \
                patch("journey_persistent.owned_cluster", return_value=cluster), \
                patch("journey_persistent.run_bound", return_value={"mocked_helper_only": True}):
            report = run_persistent_journey("/offline", {}, "/offline", "a" * 64)
        cluster.cleanup.assert_called_once_with()
        self.assertEqual(report["state"], "INCOMPLETE")
        self.assertFalse(report["qualified"])
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["full_cases"]))

    def test_failed_run_or_cleanup_retains_no_private_diagnostics(self):
        for phase in ("run", "cleanup"):
            cluster = Mock()
            if phase == "cleanup":
                cluster.cleanup.side_effect = RuntimeError("PRIVATE_DATABASE_URL")
            with self.subTest(phase=phase), patch("journey_persistent.verify_binding", return_value=self.binding()), \
                    patch("journey_persistent.owned_cluster", return_value=cluster), \
                    patch("journey_persistent.run_bound", side_effect=ValueError("PRIVATE_DATABASE_URL")
                          if phase == "run" else None, return_value={"mocked_helper_only": True}):
                report = run_persistent_journey("/offline", {}, "/offline", "a" * 64)
            cluster.cleanup.assert_called_once_with()
            self.assertEqual(report["state"], "FAILED")
            self.assertNotIn("PRIVATE_DATABASE_URL", json.dumps(report))
            self.assertTrue(any(item["state"] == "FAILED" for item in report["checks"]))

    def test_hash_failure_never_uses_fixture_or_runtime(self):
        with patch("journey_persistent.verify_binding", side_effect=JourneyError("INSTALLED_BINARY_HASH_MISMATCH")), \
                patch("journey_persistent.owned_cluster") as cluster:
            report = run_persistent_journey("/offline", {}, "/offline", "a" * 64)
        cluster.assert_not_called()
        self.assertEqual(report["failure_code"], "INSTALLED_BINARY_HASH_MISMATCH")

    def test_source_identity_or_inventory_failure_precedes_database_allocation(self):
        for failure in ("PERSISTENT_CANDIDATE_SOURCE_IDENTITY", "PERSISTENT_CANDIDATE_INVENTORY_INVALID"):
            self.verified_inventory.side_effect = JourneyError(failure)
            with self.subTest(failure=failure), \
                    patch("journey_persistent.verify_binding", return_value=self.binding()), \
                    patch("journey_persistent.owned_cluster") as cluster, \
                    patch("journey_persistent.run_bound") as run:
                report = run_persistent_journey("/offline", {}, "/offline", "a" * 64)
            cluster.assert_not_called()
            run.assert_not_called()
            self.assertEqual(report["failure_code"], failure)
            self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["full_cases"]))

    @contextmanager
    def orchestration(self, root, *, same_generation=False, changed_ledger=False, start_error=False):
        fixture = Fixture()
        cluster = Mock()
        cluster.create.return_value = "owned_fixture"
        cluster.sql.return_value = "0"
        cluster.config_url.return_value = "postgres://postgres@127.0.0.1:6432/owned_fixture?sslmode=disable"
        starts = []
        @contextmanager
        def child(args, cwd, environment):
            starts.append((args, cwd, environment))
            if start_error:
                raise OSError("private spawn failure")
            yield Mock()
        @contextmanager
        def upstream():
            yield fixture, 10004
        @contextmanager
        def ports():
            yield [10001, 10002, 10003], [Mock(), Mock(), Mock()]
        capture = {"request_id": 7, "session_id": fixture.session, "request_count": 1,
                   "request_body_sha256": "a" * 64, "response_body_sha256": "b" * 64}
        initial_ledger = {"migration_count": 2, "full_ledger_sha256": "c" * 64, "failed_rows": 0}
        later_ledger = initial_ledger | ({"full_ledger_sha256": "d" * 64} if changed_ledger else {})
        with ExitStack() as stack:
            stack.enter_context(patch("journey_persistent.verify_binding", return_value=self.binding()))
            for name, replacement in (("upstream_fixture", upstream), ("reserved_ports", ports), ("owned_child", child)):
                stack.enter_context(patch("journey_persistent." + name, replacement))
            ready = stack.enter_context(patch("journey_persistent.wait_owned_ready", side_effect=[
                {"instance": {"boot_generation": "first"}},
                {"instance": {"boot_generation": "first" if same_generation else "second"}}]))
            ledger = stack.enter_context(patch("journey_persistent.migration_ledger", side_effect=[initial_ledger, later_ledger]))
            doctor = stack.enter_context(patch("journey_persistent.doctor", return_value={"mocked": True}))
            capture_read = stack.enter_context(patch("journey_persistent.stored_capture", side_effect=[capture, dict(capture)]))
            stack.enter_context(patch("journey_persistent.stored_search", return_value={"mocked": True}))
            stream = stack.enter_context(patch("journey_persistent.stream_capture", return_value={"mocked": True}))
            mcp =stack.enter_context(patch("journey_persistent.run_mcp", return_value={"mocked": True}))
            stop = stack.enter_context(patch("journey_persistent.graceful_stop", return_value={"mocked": True}))
            yield cluster, starts, ready, doctor, capture_read, stream, mcp, stop, ledger

    def test_two_owned_generations_use_same_config_new_cwd_and_candidate_migrations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "journey"
            paths, binding = self.binding()
            result = new_result(binding)
            manifest = {"source_commit": "b" * 40, "version": "0.0.1"}
            with self.orchestration(root) as values:
                cluster, starts, ready, doctor, capture_read, stream, mcp, stop, ledger = values
                observation = run_bound(paths, manifest, root, result, cluster, synthetic_inventory())
            cluster.create.assert_called_once_with(seeded=False)
            self.assertEqual(len(starts), 2)
            self.assertEqual(starts[0][0], starts[1][0])
            self.assertEqual(starts[0][0], ["/offline/mike", "--config", str(root / "config.toml"), "serve"])
            self.assertNotEqual(starts[0][1], starts[1][1])
            self.assertTrue(all(start[2]["PATH"] == "" for start in starts))
            self.assertEqual(stream.call_count, 1)
            self.assertEqual(mcp.call_count, 2)
            self.assertEqual(stop.call_count, 2)
            self.assertEqual(ledger.call_count, 2)
            self.assertTrue(all(call.kwargs["persistent"] for call in ready.call_args_list))
            self.assertTrue(all(call.kwargs["persistent"] for call in doctor.call_args_list))
            self.assertEqual(capture_read.call_args.kwargs["expected_id"], 7)
            self.assertTrue(observation["different_restart_cwd"])
            self.assertTrue(all(item["state"] == "NOT_RUN" for item in result["full_cases"]))
            self.assertEqual(result["state"], "INCOMPLETE")

    def test_same_generation_changed_ledger_or_launch_failure_cannot_pass(self):
        for options, code in (({"same_generation": True}, "BOOT_GENERATION_REUSED"),
                              ({"changed_ledger": True}, "LEDGER_CHANGED_AFTER_RESTART"),
                              ({"start_error": True}, None)):
            with self.subTest(options=options), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / "journey"
                with self.orchestration(root, **options) as values:
                    paths, binding = self.binding()
                    manifest = {"source_commit": "b" * 40, "version": "0.0.1"}
                    if code:
                        with self.assertRaisesRegex(JourneyError, code):
                            run_bound(paths, manifest, root, new_result(binding), values[0], synthetic_inventory())
                    else:
                        with self.assertRaises(OSError):
                            run_bound(paths, manifest, root, new_result(binding), values[0], synthetic_inventory())

    def test_nonempty_created_database_never_launches_candidate(self):
        cluster = Mock()
        cluster.sql.return_value = "1"
        with patch("journey_persistent.upstream_fixture") as upstream:
            with self.assertRaisesRegex(JourneyError, "FIXTURE_NOT_EMPTY"):
                run_bound({}, {}, Path("/offline"), new_result({}), cluster, synthetic_inventory())
        upstream.assert_not_called()


class PersistentAcceptanceWiringTests(unittest.TestCase):
    def test_missing_fixture_opt_in_writes_nonzero_sanitized_evidence_without_runtime(self):
        import test_install
        fixture = test_install.InstallTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        environment = dict(os.environ)
        for name in ("MIKE_TEST_ADMIN_URL", "MIKE_RECOVERY_TEST_PGDATA"):
            environment.pop(name, None)
        output = fixture.base / "persistent-evidence"
        command = [test_install.ROOT / "scripts/release-acceptance", "--candidate", fixture.candidate,
                   "--output", output, "--persistent-journey"]
        completed = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=30)
        self.assertEqual(completed.returncode, 1, completed.stderr)
        report = json.loads((output / "result.json").read_bytes())
        self.assertEqual(report["required_not_run"], 6)
        self.assertFalse(report["qualified"])
        self.assertEqual(report["archive_persistent_journey"]["failure_code"], "PERSISTENT_FIXTURE_REQUIRED")
        self.assertTrue(all(item["state"] == "NOT_RUN" for item in report["cases"]))


if __name__ == "__main__":
    unittest.main()
