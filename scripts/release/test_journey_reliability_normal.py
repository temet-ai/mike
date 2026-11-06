"""Offline normal-default contracts. Never candidate/container/PG runtime evidence."""
import copy
import hashlib
import json
from pathlib import Path
import re
import tempfile
import tomllib
import unittest
from unittest.mock import Mock, patch

import journey_reliability as base
import journey_reliability_normal as normal
import journey_reliability_normal_defaults as defaults
import journey_reliability_normal_fixture as fixture
import journey_reliability_normal_workers as workers
from journey_fixture import MODEL, TEXT, TOOL_NAME
from journey_process import JourneyError, installation_identity
from journey_reliability_defaults import normal_default_config

ROOT = Path(__file__).resolve().parents[2]


class OfflineTests(unittest.TestCase):
    def setUp(self):
        for name in ("socket.socket", "socket.create_connection", "subprocess.Popen", "subprocess.run"):
            self.enterContext(patch(name, side_effect=AssertionError("Offline test attempted runtime")))


def project():
    return {"root": "/work/normal/synthetic-project", "id": 7,
            "project_uuid": "11111111-1111-4111-8111-111111111111",
            "location_uuid": "22222222-2222-4222-8222-222222222222",
            "machine_uuid": "33333333-3333-4333-8333-333333333333",
            "machine_name": "archive-synthetic-machine",
            "path": "mike:11111111-1111-4111-8111-111111111111"}


def response(item):
    return {"id": item.message_id, "type": "message", "role": "assistant", "model": MODEL,
            "stop_reason": "tool_use", "stop_sequence": None,
            "content": [{"type": "text", "text": item.response_text},
                        {"type": "tool_use", "id": item.tool_use_id, "name": TOOL_NAME, "input": {"echo": "synthetic"}}],
            "usage": {"input_tokens": 25, "output_tokens": 12}}


class FixtureTests(OfflineTests):
    def test_private_setup_preseeds_only_owned_synthetic_machine_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            generated = fixture.synthetic_project(root)
            endpoints = dict(zip(("proxy", "inspect", "spa", "upstream", "services", "postgres"),
                                 range(14000, 14006)))
            config, baseline, home, cwd, same, machine = normal.private_setup(
                root / "runtime", endpoints, "recovery_fixture_0123456789abcdef_0", generated)
            self.assertIs(same, generated)
            self.assertEqual(machine, home / ".local/state/mike/machine.json")
            self.assertEqual(json.loads(machine.read_text()), {
                "machine_uuid": generated["machine_uuid"], "machine_name": "archive-synthetic-machine"})
            self.assertEqual(machine.stat().st_mode & 0o777, 0o600)
            self.assertEqual(baseline.read_bytes(), b"")
            self.assertEqual(list(cwd.iterdir()), [])
            self.assertEqual(list(Path(generated["root"]).iterdir()), [])
            self.assertNotIn("machine", config.read_text())

    def test_exact_system_marker_and_source_formed_update_tag(self):
        cohort = fixture.NormalCohort(8, project())
        items = cohort.prepare(0)
        self.assertEqual(len(items), 8)
        for item in items:
            body = json.loads(item.body)
            self.assertEqual(body["system"], "Working directory: " + project()["root"])
            self.assertNotIn("Working directory:", body["messages"][0]["content"])
            delta = json.loads(item.events[2].split(b"data: ")[1])
            self.assertEqual(delta["delta"]["text"], item.response_text)
            self.assertIn('<mem:update topic="' + item.lesson_topic + '">' + item.lesson_text + '</mem:update>', item.response_text)
            self.assertIs(cohort.claim(item.body), item)
            cohort.finish()
        self.assertEqual(len({item.lesson_text for item in items}), 8)
        second = cohort.prepare(1)
        self.assertTrue({item.lesson_text for item in items}.isdisjoint(item.lesson_text for item in second))

    def test_upstream_body_mutation_is_still_rejected(self):
        cohort = fixture.NormalCohort(1, project())
        item = cohort.prepare(0)[0]
        with self.assertRaises(JourneyError):
            cohort.claim(item.body + b" ")

    def test_exact_harvester_text_is_checked_before_standard_metadata(self):
        item = fixture.NormalCohort(1, project()).prepare(0)[0]
        body = response(item)
        fixture.validate_normal_response(json.dumps(body).encode(), item)
        for field, value in (("text", TEXT), ("text", item.response_text + " altered")):
            altered = copy.deepcopy(body)
            altered["content"][0][field] = value
            with self.assertRaisesRegex(JourneyError, "TRIGGER_CHANGED"):
                fixture.validate_normal_response(json.dumps(altered).encode(), item)
        body["usage"]["output_tokens"] = 99
        with self.assertRaisesRegex(JourneyError, "USAGE"):
            fixture.validate_normal_response(json.dumps(body).encode(), item)

    def test_capture_batch_accepts_explicit_response_validator_without_changing_request_checks(self):
        item = fixture.NormalCohort(1, project()).prepare(0)[0]
        raw = json.dumps(response(item)).encode()
        values = [{"session_id": item.session, "offset": 0, "limit": 2, "total_requests": 1,
                   "requests": [{"id": 1, "status": "Complete", "session_id": item.session}]}, {},
                  {"has_body": True, "text": item.body.decode(), "byte_size": len(item.body), "reconstructed": False},
                  {"has_body": True, "text": raw.decode(), "byte_size": len(raw)}]
        validator = Mock(wraps=fixture.validate_normal_response)
        with patch.object(base, "api_json", side_effect=values), patch.object(base, "validate_detail"):
            rows = base.capture_batch(12345, [item], 0, {}, base.Budget(10), response_validator=validator)
        validator.assert_called_once_with(raw, item)
        self.assertEqual(rows[0]["request_id"], 1)

    def test_marker_and_tag_source_contracts_remain_current(self):
        parser = (ROOT / "crates/mike/src/memory_harvester/parser.rs").read_text()
        self.assertIn('find("<mem:update ")', parser)
        self.assertIn('let close_tag = "</mem:update>"', parser)
        source = (ROOT / "crates/mike/src/extract/request/project_scan.rs").read_text()
        self.assertIn('stripped.strip_prefix("Working directory:")', source)
        self.assertIn('out.push(sp)', source)


class WorkerTests(OfflineTests):
    def setUp(self):
        super().setUp()
        self.project = project()
        self.item = fixture.NormalCohort(1, self.project).prepare(0)[0]
        self.expected = {10: {"session_id": self.item.session, "topic": self.item.lesson_topic, "text": self.item.lesson_text}}
        self.registration = {"session_id": self.item.session, "project_id": 7, "registration_project_id": 7,
            "project_resolution": "operator", "location_uuid": self.project["location_uuid"],
            "cwd": self.project["root"], "registered_via": "passive", "wrong_request_projects": 0,
            "pid": None, "boot_id": None, "released_at": None, "reaped_at": None}
        self.lesson = {"id": 20, "project_id": 7, "session_id": self.item.session, "request_id": 10,
            "topic": self.item.lesson_topic, "text": self.item.lesson_text,
            "text_sha256": hashlib.sha256(self.item.lesson_text.lower().encode()).hexdigest(),
            "kind": "convention", "source": "harvester", "stage": "raw", "source_turn_id": None,
            "agent_id": None, "archived_at": None, "expires_at": None, "embedding_absent": True,
            "request_session_matches": True, "request_project_matches": True}

    def test_exact_registration_and_lesson_provenance(self):
        self.assertTrue(workers.validate_registrations([self.registration], [self.item.session], self.project))
        self.assertTrue(workers.validate_lessons([self.lesson], self.expected, self.project))
        self.assertFalse(workers.validate_lessons([], self.expected, self.project))
        self.assertFalse(workers.validate_registrations([], [self.item.session], self.project))

    def test_registration_foreign_paths_pids_or_identity_fail(self):
        for key, value in (("cwd", "/operator/project"), ("registered_via", "hook"), ("pid", 88),
                           ("project_id", 9), ("location_uuid", "wrong"), ("wrong_request_projects", 1),
                           ("released_at", "ended"), ("reaped_at", "reaped")):
            with self.subTest(key=key), self.assertRaises(JourneyError):
                workers.validate_registrations([dict(self.registration, **{key: value})], [self.item.session], self.project)

    def test_lesson_invented_missing_or_duplicate_provenance_fails(self):
        for key, value in (("source", "operator"), ("session_id", None), ("request_id", None),
                           ("project_id", 9), ("text", "fake"), ("text_sha256", "0" * 64),
                           ("topic", "fake"), ("kind", "warning"), ("stage", "daily"),
                           ("request_session_matches", False), ("request_project_matches", False),
                           ("embedding_absent", False), ("source_turn_id", 3)):
            with self.subTest(key=key), self.assertRaises(JourneyError):
                workers.validate_lessons([dict(self.lesson, **{key: value})], self.expected, self.project)
        with self.assertRaises(JourneyError):
            workers.validate_lessons([self.lesson, self.lesson], self.expected, self.project)

    def test_only_owned_database_read_transactions_are_used(self):
        cluster = Mock(created=["owned"])
        cluster.sql.return_value = "[]"
        self.assertEqual(workers.read_rows(cluster, "owned", workers.LESSONS_SQL), [])
        sql = cluster.sql.call_args.args[1]
        self.assertTrue(sql.startswith("BEGIN READ ONLY;"))
        self.assertIn("statement_timeout=5000", sql)
        for forbidden in ("INSERT ", "UPDATE ", "DELETE ", "CREATE "):
            self.assertNotIn(forbidden, sql)
        cluster.sql.reset_mock()
        with self.assertRaises(JourneyError):
            workers.read_rows(cluster, "foreign", workers.LESSONS_SQL)
        cluster.sql.assert_not_called()

    def test_setup_uses_only_project_and_location_apis(self):
        identity = {"id": 7, "project_uuid": self.project["project_uuid"], "project_path": self.project["path"]}
        location = {"project_uuid": self.project["project_uuid"], "project_path": self.project["path"],
                    "location_uuid": self.project["location_uuid"]}
        with patch.object(workers, "setup_post", side_effect=[identity, location]) as post:
            observed = workers.setup_project(12345, self.project, base.Budget(10))
        self.assertEqual([call.args[1] for call in post.call_args_list],
                         ["/api/knowledge/projects", "/api/knowledge/locations"])
        self.assertEqual(observed["sessions_registered_by_setup"], 0)
        self.assertEqual(observed["lessons_written_by_setup"], 0)

    def test_worker_api_rechecks_roster_and_exact_generated_lesson(self):
        roster = {"project_id": 7, "project_path": self.project["path"],
                  "roster": [dict(self.registration, channel_present=False)]}
        with patch.object(workers, "api_json", side_effect=[roster, [self.lesson]]) as get:
            observed = workers.worker_api(12345, [self.item], self.project, [self.item.session], base.Budget(10))
        self.assertTrue(observed["roster_api_matches"])
        self.assertTrue(all(call.args[1].startswith("/api/") for call in get.call_args_list))


class DefaultTests(OfflineTests):
    def settings(self):
        values = {key: 0 for key in defaults.SETTINGS_KEYS}
        values.update({"embedding.enabled": False, "embedding.url": "http://127.0.0.1:7997", "launch_backend": "none"})
        return {"settings": [{"key": key, "default": value, "value": value} for key, value in sorted(values.items())],
                "steerer_openrouter_configured_via_env": False}

    def test_every_setting_is_present_default_and_stable(self):
        value = self.settings()
        self.assertEqual(set(defaults.validate_settings(value)), defaults.SETTINGS_KEYS)
        for index in range(len(value["settings"])):
            bad = copy.deepcopy(value)
            bad["settings"].pop(index)
            with self.assertRaisesRegex(JourneyError, "SETTINGS_KEYS"):
                defaults.validate_settings(bad)
        for index in range(len(value["settings"])):
            bad = copy.deepcopy(value)
            bad["settings"][index]["value"] = "changed-default"
            with self.subTest(key=bad["settings"][index]["key"]), self.assertRaisesRegex(JourneyError, "SETTING_CHANGED"):
                defaults.validate_settings(bad)
        bad = copy.deepcopy(value)
        bad["settings"][0].update(default=1, value=True)
        with self.assertRaisesRegex(JourneyError, "SETTING_CHANGED"):
            defaults.validate_settings(bad)
        bad = copy.deepcopy(value)
        bad["settings"].append({"key": "unknown.setting", "default": 0, "value": 0})
        with self.assertRaisesRegex(JourneyError, "SETTINGS_KEYS"):
            defaults.validate_settings(bad)
        bad = copy.deepcopy(value)
        bad["settings"].append(bad["settings"][0])
        with self.assertRaisesRegex(JourneyError, "SETTINGS_KEYS"):
            defaults.validate_settings(bad)

    def test_settings_inventory_tracks_source_metadata_and_imported_keys(self):
        keys = set()
        for path in (ROOT / "crates/mike/src/settings").glob("meta_*.rs"):
            if "tests" in path.stem:
                continue
            source = path.read_text()
            keys.update(re.findall(r'key:\s*"([^"]+)"', source))
            keys.update(re.findall(r'"((?:[a-z][a-z0-9_]*\.)+[a-z][a-z0-9_]*|launch_[a-z_]+)"', source))
        keys.discard("meta_assistants_tests.rs")
        source = (ROOT / "crates/mike/src/cross_session/settings.rs").read_text()
        keys.update(re.findall(r'const [A-Z_]+_KEY: &str = "([^"]+)"', source))
        self.assertEqual(keys, defaults.SETTINGS_KEYS)

    def test_boot_comparison_rejects_any_unpermitted_field_change(self):
        ports = dict(zip(("proxy", "inspect", "spa", "upstream", "services", "postgres"), range(10001, 10007)))
        database = "recovery_fixture_" + "a" * 16 + "_1"
        config, baseline = Path("/work/a.toml"), Path("/work/b.toml")
        original = {"smoke_run": False, "persist_enabled": True, "db_resync_enabled": True,
                    "db_fallback_enabled": True, "embedding.enabled": False,
                    "embedding.url": "http://127.0.0.1:7997", "untoggled_worker_setting": 33}
        for path, value in ((baseline, original),):
            root = installation_identity(path)[1].parent.parent
            value.update(data_dir=str(root / "data"), log_dir=str(root / "logs"))
        actual = dict(original)
        actual.update(defaults.flatten(tomllib.loads(normal_default_config(ports, database).decode())))
        actual["database_url"] = actual["database_url"].split("?")[0]
        root = installation_identity(config)[1].parent.parent
        actual.update(data_dir=str(root / "data"), log_dir=str(root / "logs"))
        self.assertTrue(defaults.validate_boot_defaults(actual, original, config, baseline, ports, database)["compiled_boot_defaults_preserved"])
        for key, value in (("untoggled_worker_setting", 4), ("smoke_run", True), ("embedding.enabled", True),
                           ("db_resync_enabled", False), ("local_database_url", "postgres://foreign")):
            with self.subTest(key=key), self.assertRaises(JourneyError):
                defaults.validate_boot_defaults(dict(actual, **{key: value}), original, config, baseline, ports, database)

    def test_plain_invocation_fails_before_database_or_filesystem(self):
        cluster = Mock()
        with self.assertRaisesRegex(JourneyError, "VERIFIED_CONTAINER_REQUIRED"):
            normal.run_bound({}, {}, Path("/unused"), {}, cluster, {}, {}, isolation={"verified": True})
        cluster.create.assert_not_called()

    def test_normal_runner_never_calls_synthetic_feature_isolation(self):
        source = (ROOT / "scripts/release/journey_reliability_normal.py").read_text()
        for symbol in ("disable_features(", "isolated_config(", '"--smoke-run"', 'for phase in ("isolation"'):
            self.assertNotIn(symbol, source)
        self.assertIn("postgres=cluster.url.port", source)
        self.assertIn('"fleet_auto_start": "NOT_RUN"', source)
        self.assertIn('"prompt_assistant": "NOT_RUN"', source)


class WorkloadTests(OfflineTests):
    def run_workload(self, *, drift=False, service_failure=False):
        cohort = fixture.NormalCohort(1, project())
        policy = dict(base.reliability_policy(), duration_seconds=0.02, minimum_duration_seconds=0.02,
                      interval_seconds=0.01, max_batches=3, max_requests=3)
        budgets = {"rss_peak_bytes": 100, "fd_peak": 100, "broadcast_backlog_peak": 100,
                   "active_streams_peak": 1, "drain_seconds": 1, "responsiveness_seconds": 1}
        report = base.new_reliability_report(policy, profile="normal_defaults", resource_budgets=budgets)
        services, observations = Mock(), Mock()
        if service_failure:
            services.require_no_default_worker_calls.side_effect = JourneyError("NORMAL_DEFAULT_UNEXPECTED_SERVICE_CALL")
        health = {"instance": {"boot_generation": "same"}, "persistence": {"capture_stalled": False}}
        def batch(_port, _cohort, items, **kwargs):
            kwargs["observe_active"](99)
            return {"completed": len(items), "concurrent_sessions": len(items), "request_bytes": 100, "response_bytes": 200}
        def capture(_port, items, number, seen, _budget, **kwargs):
            self.assertIs(kwargs["response_validator"], fixture.validate_normal_response)
            rows = []
            for item in items:
                request = number + 1
                seen[request] = item.session
                rows.append({"request_id": request, "session_id": item.session, "tool_use_id": item.tool_use_id})
            return rows
        ticks = iter(index * 0.01 for index in range(1000))
        with patch.object(base.time, "monotonic", side_effect=lambda: next(ticks)), \
                patch.object(normal, "wait_owned_ready", return_value=health), \
                patch.object(normal, "observe_defaults", side_effect=JourneyError("DEFAULT_DRIFT") if drift else None), \
                patch.object(normal, "database_counts"), patch.object(normal, "read_rows", return_value=[]), \
                patch("journey_reliability_fixture.run_batch", side_effect=batch) as run, \
                patch.object(normal, "capture_batch", side_effect=capture), \
                patch.object(normal, "worker_database", return_value={"harvested_lessons": 1}) as durable, \
                patch.object(normal, "worker_api", return_value={"roster_api_matches": True}), \
                patch.object(normal, "final_accounting", return_value={}):
            if drift or service_failure:
                with self.assertRaises(JourneyError):
                    normal.workload(Mock(), Path("/work/config"), [1, 2, 3], {"source_commit": "a" * 40},
                                    Mock(), "owned", cohort, project(), policy, report, services, {}, observations)
                run.assert_not_called()
                return
            seen, tools, lessons = normal.workload(Mock(), Path("/work/config"), [1, 2, 3], {"source_commit": "a" * 40},
                                                  Mock(), "owned", cohort, project(), policy, report, services, {}, observations)
        self.assertEqual(report["workload_state"], "PASSED")
        self.assertTrue(report["duration_policy_met"])
        self.assertEqual(len(seen), len(lessons))
        self.assertEqual(set(seen), set(tools))
        durable.assert_called_once()
        observations.active.assert_called_once()
        self.assertEqual(report["worker_exercise"]["fleet_auto_start"], "NOT_RUN")
        self.assertEqual(report["worker_exercise"]["harvester"], "PASSED")

    def test_paced_normal_workload_reaches_real_worker_observation_seams(self):
        self.run_workload()

    def test_default_drift_stops_before_traffic(self):
        self.run_workload(drift=True)

    def test_unexpected_service_use_stops_before_traffic(self):
        self.run_workload(service_failure=True)

    def test_bad_duration_rejected_before_allocating_database(self):
        cluster = Mock()
        policy = dict(base.reliability_policy(), duration_seconds=0.02)
        with patch("journey_reliability_container.require_admission"):
            with self.assertRaisesRegex(JourneyError, "DURATION_POLICY"):
                normal.run_bound({}, {}, None, {}, cluster, {}, policy, isolation=object())
        cluster.create.assert_not_called()


class DefaultsObservationTests(OfflineTests):
    settings = DefaultTests.settings

    def observe(self, overlays, report):
        features = {"features": [{"key": key, "enabled": key in defaults.NORMAL_ENABLED,
                    "scope": meta[0], "applies": meta[1]} for key, meta in defaults.FEATURES.items()]}
        cluster = Mock(created=["owned"])
        cluster.sql.return_value = json.dumps(overlays)
        with patch.object(defaults, "verify_service_prerequisites"), \
                patch.object(defaults, "api_json", side_effect=[features, self.settings()]):
            defaults.observe_defaults(12345, {}, cluster, "owned", base.Budget(10), report)
        self.assertTrue(cluster.sql.call_args.args[1].startswith("BEGIN READ ONLY;"))

    def test_factory_operator_set_rows_are_allowed_but_any_later_change_fails(self):
        report = {"feature_observations": 0}
        overlays = {"settings": 0, "toggles": [{"key": "harvester", "enabled": True, "operator_set": True}]}
        self.observe(overlays, report)
        self.observe(overlays, report)
        self.assertEqual(report["settings_observations"], 2)
        self.assertEqual(report["effective_settings_count"], len(defaults.SETTINGS_KEYS))
        overlays["toggles"][0]["operator_set"] = False
        with self.assertRaisesRegex(JourneyError, "OVERRIDE_CHANGED"):
            self.observe(overlays, report)

    def test_database_override_even_if_hidden_by_fallback_api_fails(self):
        for overlays in ({"settings": 1, "toggles": []},
                         {"settings": 0, "toggles": [{"key": "harvester", "enabled": False, "operator_set": True}]},
                         {"settings": 0, "toggles": [{"key": "unknown", "enabled": True, "operator_set": False}]}):
            with self.subTest(overlays=overlays), self.assertRaises(JourneyError):
                self.observe(overlays, {"feature_observations": 0})


class LifecycleTests(OfflineTests):
    def test_single_boot_cleanup_exact_binding_and_post_stop_durability(self):
        from contextlib import contextmanager
        @contextmanager
        def context(value):
            yield value
        budgets = {"rss_peak_bytes": 100, "fd_peak": 100, "broadcast_backlog_peak": 100,
                   "active_streams_peak": 1, "drain_seconds": 1, "responsiveness_seconds": 1}
        policy = base.reliability_policy()
        report = base.new_reliability_report(policy, profile="normal_defaults", resource_budgets=budgets)
        binding = {"manifest_sha256": "a" * 64}
        result = {"reliability": report, "binding": binding, "checks": []}
        cluster = Mock(created=["recovery_fixture_" + "b" * 16 + "_1"])
        cluster.create.return_value = cluster.created[0]
        cluster.sql.return_value = "0"
        cluster.url.port = 55439
        paths = {"mike": Path("/work/mike"), "mike-mcp": Path("/work/mike-mcp")}
        cohort, child, services, observations = Mock(), Mock(), Mock(), Mock()
        cohort.sessions = ["synthetic-session"]
        def measured(*args):
            report["baseline_comparison"] = {}
            report["workload_state"] = "PASSED"
            report["worker_observations"] = {"worker_rows_sha256": "c" * 64}
            return {1: "synthetic-session"}, {1: "synthetic-tool"}, {1: {"text": "must-not-be-in-report"}}
        reservations = [Mock(), Mock(), Mock()]
        with tempfile.TemporaryDirectory() as temporary, \
                patch("journey_reliability_container.require_admission") as admitted, \
                patch.object(normal, "reserved_ports", return_value=context(([11001, 11002, 11003], reservations))), \
                patch.object(normal, "normal_services", return_value=context((services, 11004))), \
                patch.object(normal, "normal_fixture", return_value=context((cohort, 11005))), \
                patch.object(normal, "boot_snapshot", return_value={}), \
                patch.object(normal, "validate_boot_defaults", return_value={}), \
                patch.object(normal, "verify_binding", return_value=(paths, binding)), \
                patch.object(normal, "verified_migration_inventory", return_value={}), \
                patch.object(normal, "owned_child", return_value=context(child)) as spawn, \
                patch.object(normal, "wait_owned_ready", return_value={}), \
                patch.object(normal, "observe_defaults"), \
                patch.object(normal, "migration_ledger", return_value={}), \
                patch.object(normal, "setup_project", return_value={}), \
                patch.object(normal, "project_database", return_value={}), \
                patch.object(normal, "observe_workload", return_value=context(observations)), \
                patch.object(normal, "workload", side_effect=measured), \
                patch.object(normal, "worker_database", return_value={"worker_rows_sha256": "c" * 64}) as durable, \
                patch.object(normal, "final_accounting"), \
                patch.object(normal, "graceful_stop", return_value={}) as stopped:
            normal.run_bound(paths, {"source_commit": "a" * 40}, Path(temporary) / "owned", result,
                             cluster, {}, policy, isolation=object())
            config = Path(temporary) / "owned/runtime/config.toml"
            value = tomllib.loads(config.read_text())
            self.assertIn(":55439/", value["database_url"])
            self.assertNotIn("harvester", value)
            self.assertNotIn("lesson_worker", value)
            self.assertNotIn("smoke_run", value)
        admitted.assert_called_once()
        spawn.assert_called_once()
        stopped.assert_called_once_with(child)
        durable.assert_called_once()
        self.assertNotIn("must-not-be-in-report", json.dumps(result, default=str))
        checks = {item["id"]: item for item in result["checks"]}
        self.assertEqual(checks["archive_normal_default_workload"]["observation"], {"requests": 1, "harvested_lessons": 1})
        self.assertEqual(checks["archive_normal_default_process_cleanup"]["state"], "PASSED")
        self.assertEqual(checks["archive_normal_default_fixture_cleanup"]["state"], "PASSED")


class FixtureCleanupTests(OfflineTests):
    def test_thread_construction_and_start_failure_close_listener(self):
        for kind in ("construction", "start"):
            for make in (lambda: fixture.normal_fixture(1, project()), fixture.normal_services):
                with self.subTest(kind=kind, fixture=make):
                    server, thread = Mock(), Mock()
                    thread.is_alive.return_value = False
                    if kind == "start":
                        thread.start.side_effect = RuntimeError("synthetic failure")
                    with patch.object(fixture, "BoundedServer", return_value=server), \
                            patch.object(fixture.threading, "Thread", return_value=thread,
                                         side_effect=RuntimeError("synthetic failure") if kind == "construction" else None):
                        with self.assertRaisesRegex(RuntimeError, "synthetic failure"):
                            with make():
                                self.fail("Startup failure yielded a fixture")
                    server.server_close.assert_called_once()
                    server.close_owned_connections.assert_called_once()
                    server.shutdown.assert_not_called()

    def test_shutdown_failure_still_closes_listener_and_handlers(self):
        server, thread = Mock(), Mock()
        thread.is_alive.return_value = False
        server.shutdown.side_effect = RuntimeError("synthetic shutdown failure")
        with patch.object(fixture.threading, "Thread", return_value=thread):
            with self.assertRaisesRegex(RuntimeError, "synthetic shutdown failure"):
                with fixture.owned_server(server):
                    pass
        server.server_close.assert_called_once()
        server.close_owned_connections.assert_called_once()
        thread.join.assert_called_once()


if __name__ == "__main__":
    unittest.main()
