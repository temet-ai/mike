"""Offline maintenance-journey contracts: no binaries, sockets or DB processes.

Mocks verify refusal, lifecycle ordering and evidence labels, not actual installed
backup/restore. These tests cannot qualify AC22/AC23/AC24 or any other full case.
"""
from contextlib import ExitStack, contextmanager
import json
import os
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import Mock, patch

from common import sha256
from journey import create_config, new_result
from journey_fixture import Fixture
from journey_maintenance import (InstalledKit, rebind_restored_fixture, restore_and_compare,
                                  run_maintenance_roundtrip, runtime_roots, source_contract,
                                  verify_offline_copy)
from journey_process import JourneyError, installation_identity, private_write
from recovery_files import inventory


class MaintenanceFixtureTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mm-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.sandbox = Path(temporary.name).resolve()
        self.root = self.sandbox / "maintenance"
        self.url = "postgres://postgres@127.0.0.1:6432/owned_source?sslmode=disable"
        self.target_url = self.url.replace("owned_source", "owned_target")
        config, home, cwd = create_config(self.sandbox / "persistent journey", [10001, 10002, 10003],
                                           10004, database_url=self.url)
        self.fixture = Fixture()
        self.prefix = self.sandbox / "prefix"
        self.source = {"prefix": self.prefix, "config": config, "home": home, "cwd": cwd,
                       "database": "owned_source", "fixture": self.fixture, "ports": [10001, 10002, 10003],
                       "generations": ["initial", "restarted"],
                       "capture": {"request_id": 7, "session_id": self.fixture.session, "request_count": 1,
                                   "request_body_sha256": "c" * 64, "response_body_sha256": "d" * 64},
                       "ledger": {"migration_count": 2, "full_ledger_sha256": "b" * 64},
                       "mcp": {"protocol": "2024-11-05", "profile": "core", "tool_count": 7,
                               "read_tool": "get_request", "matching_capture": True, "protocol_frames": 3}}
        self.binding = {"manifest_sha256": "a" * 64}
        self.result = new_result(self.binding)
        for phase in ("initial", "restarted"):
            self.result["checks"].append({"id": "archive_persistent_" + phase + "_owned_stop", "state": "PASSED"})
        self.cluster = Mock(created=["owned_source"])
        self.cluster.config_url.side_effect = lambda name: {"owned_source": self.url, "owned_target": self.target_url}[name]
        self.cluster.sql.return_value = "0"
        def create(*, seeded):
            self.assertIs(seeded, False)
            self.cluster.created.append("owned_target")
            return "owned_target"
        self.cluster.create.side_effect = create
        self.cluster.service.side_effect = lambda path, _database: path
        self.manifest = {"version": "0.1.0", "source_commit": "b" * 40}
        self.candidate_inventory = {"offline_mock": True}
        self.paths = {name: self.sandbox / name for name in ("mike", "mike-mcp")}
        self.roots = self.make_roots(config)
        self.output = self.sandbox / "restored"
        self.output.mkdir(mode=0o700)
        self.copy_to_output()
        self.commands = self.enterContext(patch("journey_maintenance.captured_run",
                                                side_effect=AssertionError("unexpected process execution")))
        self.launch = self.enterContext(patch("journey_maintenance.owned_child",
                                              side_effect=AssertionError("unexpected daemon execution")))
        self.enterContext(patch.dict(os.environ, {}, clear=True))

    def make_roots(self, config):
        _, record = installation_identity(config)
        record.parent.parent.mkdir(mode=0o700)
        roots = {"config": config, "data": record.parent.parent / "data", "state": record.parent}
        for key in ("data", "state"):
            roots[key].mkdir(mode=0o700)
        private_write(roots["data"] / "body", b"synthetic private capture")
        private_write(roots["state"] / "retained", b"synthetic state")
        return roots

    def copy_to_output(self):
        private_write(self.output / "config.toml", self.source["config"].read_bytes())
        for key in ("data", "state"):
            (self.output / key).mkdir(mode=0o700)
            for path in self.roots[key].iterdir():
                private_write(self.output / key / path.name, path.read_bytes())
        private_write(self.output / "RESTORED.json", b"{}")

    def kit(self):
        kit = Mock(paths=self.paths, home=self.source["home"], cwd=self.source["cwd"])
        kit.recheck.return_value = {"offline_mock": True}
        return kit

    def test_source_contract_requires_owned_database_and_completed_owned_stops(self):
        self.assertTrue(source_contract(self.source, self.result, self.cluster, self.root)["owned_source_only"])
        for change, code in ((lambda: self.cluster.created.clear(), "SOURCE_NOT_OWNED"),
                             (lambda: self.result["checks"].pop(), "SOURCE_NOT_STOPPED"),
                             (lambda: self.source.update(generations=["same", "same"]), "RESTART_REQUIRED")):
            with self.subTest(code=code):
                self.cluster.created = ["owned_source"]
                self.result["checks"] = [{"id": "archive_persistent_" + p + "_owned_stop", "state": "PASSED"}
                                         for p in ("initial", "restarted")]
                self.source["generations"] = ["initial", "restarted"]
                change()
                with self.assertRaisesRegex(JourneyError, code):
                    source_contract(self.source, self.result, self.cluster, self.root)
        self.commands.assert_not_called()
        self.launch.assert_not_called()

    def test_source_contract_rejects_foreign_config_and_unverified_mcp(self):
        original = self.source["config"].read_bytes()
        for replacement in (original.replace(b"owned_source", b"foreign_database"), original + b"data_dir='/outside'\n"):
            self.source["config"].write_bytes(replacement)
            with self.assertRaisesRegex(JourneyError, "SOURCE_CONFIG_INVALID"):
                source_contract(self.source, self.result, self.cluster, self.root)
        self.source["config"].write_bytes(original)
        self.source["mcp"] = {"matching_capture": True}
        with self.assertRaisesRegex(JourneyError, "SOURCE_MCP_INVALID"):
            source_contract(self.source, self.result, self.cluster, self.root)

    def test_remote_upstream_or_changed_listeners_cannot_launch_restored_fixture(self):
        original = self.source["config"].read_bytes()
        for old, new, code in ((b"http://127.0.0.1:10004", b"https://provider.invalid", "UPSTREAM_INVALID"),
                              (b"127.0.0.1:10002", b"0.0.0.0:10002", "LISTENERS_INVALID")):
            self.source["config"].write_bytes(original.replace(old, new))
            with self.subTest(code=code), self.assertRaisesRegex(JourneyError, code):
                self.run_roundtrip()
        self.cluster.create.assert_not_called()
        self.launch.assert_not_called()

    def test_authoritative_runtime_paths_must_remain_inside_selected_fixture(self):
        kit = self.kit()
        identity, record = installation_identity(self.source["config"])
        paths = {"config_file": str(self.source["config"]), "installation_id": identity,
                 "data_dir": str(self.roots["data"]), "state_dir": str(record.parent),
                 "log_dir": str(record.parent.parent / "logs")}
        self.commands.side_effect = None
        self.commands.return_value = json.dumps(paths).encode()
        self.assertEqual(runtime_roots(kit, self.source["config"], self.source["config"].parent), self.roots)
        self.assertEqual(self.commands.call_args.args[2]["PATH"], "")
        for key, value, code in (("data_dir", "/foreign/data", "ROOT_OUTSIDE_FIXTURE"),
                                  ("state_dir", "/foreign/state", "RUNTIME_PATHS_INVALID"),
                                  ("installation_id", "foreign", "RUNTIME_PATHS_INVALID")):
            self.commands.return_value = json.dumps(paths | {key: value}).encode()
            with self.subTest(key=key), self.assertRaisesRegex(JourneyError, code):
                runtime_roots(kit, self.source["config"], self.source["config"].parent)

    def verify_copy(self):
        saved = {"installed_manifest_sha256": self.binding["manifest_sha256"], **self.manifest}
        with patch("journey_maintenance.verify", return_value=(saved, {"files": inventory(self.roots)})):
            return verify_offline_copy(self.sandbox / "offline-bundle", self.output, self.roots,
                                       self.manifest, self.binding)

    def test_complete_offline_copy_is_checked_before_narrow_fixture_rebinding(self):
        before = inventory(self.roots)
        self.assertTrue(self.verify_copy()["restored_offline_before_rebinding"])
        config, roots = rebind_restored_fixture(self.output, self.target_url, self.source["config"])
        value = tomllib.loads(config.read_text())
        expected = tomllib.loads(self.source["config"].read_text())
        expected.update(database_url=self.target_url, data_dir=str(self.output / "data"))
        self.assertEqual(value, expected)
        self.assertEqual(roots["state"], installation_identity(config)[1].parent)
        self.assertEqual((roots["state"] / "retained").read_bytes(), b"synthetic state")
        self.assertFalse((self.output / "state").exists())
        self.assertEqual(inventory(self.roots), before)
        self.assertEqual(config.stat().st_mode & 0o777, 0o600)

    def test_corrupt_extra_missing_or_ephemeral_restored_files_fail_closed(self):
        body = self.output / "data/body"
        original = body.read_bytes()
        body.write_bytes(b"corrupt")
        with self.assertRaisesRegex(JourneyError, "RESTORED_FILES_MISMATCH"):
            self.verify_copy()
        body.write_bytes(original)
        foreign = self.output / "state/foreign"
        private_write(foreign, b"not from snapshot")
        with self.assertRaisesRegex(JourneyError, "RESTORED_FILES_MISMATCH"):
            self.verify_copy()
        foreign.unlink()
        ephemeral = self.output / "state/instance.json"
        private_write(ephemeral, b"{}")
        with self.assertRaisesRegex(JourneyError, "RESTORED_EPHEMERAL_STATE"):
            self.verify_copy()
        ephemeral.unlink()
        private_write(self.output / "unexpected", b"extra")
        with self.assertRaisesRegex(JourneyError, "RESTORED_OUTPUT_SET"):
            self.verify_copy()
        self.launch.assert_not_called()

    def test_rebind_refuses_a_changed_config_and_never_rewrites_source(self):
        before = self.source["config"].read_bytes()
        (self.output / "config.toml").write_bytes(before + b"# changed\n")
        with self.assertRaisesRegex(JourneyError, "RESTORED_CONFIG_CHANGED"):
            rebind_restored_fixture(self.output, self.target_url, self.source["config"])
        self.assertEqual(self.source["config"].read_bytes(), before)
        self.assertTrue((self.output / "state").exists())

    @contextmanager
    def orchestration(self, *, offline_failure=False, target_nonempty=False):
        events, kit = [], self.kit()
        kit.command.side_effect = lambda operation, _args: events.append(operation) or {"offline_mock": True}
        if target_nonempty:
            self.cluster.sql.return_value = "1"
        def offline(*_args):
            events.append("offline-verification")
            if offline_failure:
                raise JourneyError("MAINTENANCE_RESTORED_FILES_MISMATCH")
            return {"offline_mock": True}
        def runtime(*_args):
            events.append("restored-runtime")
        with ExitStack() as stack:
            stack.enter_context(patch("journey_maintenance.InstalledKit", return_value=kit))
            stack.enter_context(patch("journey_maintenance.runtime_roots", return_value=self.roots))
            stack.enter_context(patch("journey_maintenance.migration_ledger", return_value=self.source["ledger"]))
            stack.enter_context(patch("journey_maintenance.verify_offline_copy", side_effect=offline))
            compare = stack.enter_context(patch("journey_maintenance.restore_and_compare", side_effect=runtime))
            yield events, kit, compare

    def run_roundtrip(self):
        return run_maintenance_roundtrip(self.paths, self.manifest, self.root, self.result,
                                         self.cluster, self.candidate_inventory, self.source)

    def test_roundtrip_orders_backup_then_fresh_restore_then_integrity_then_runtime(self):
        with self.orchestration() as (events, kit, compare):
            observation = self.run_roundtrip()
        self.assertEqual(events, ["backup", "verify", "restore", "offline-verification", "restored-runtime"])
        self.cluster.create.assert_called_once_with(seeded=False)
        self.assertEqual(self.cluster.created, ["owned_source", "owned_target"])
        self.assertEqual(self.cluster.service.call_args_list[0].args[1], "owned_source")
        self.assertEqual(self.cluster.service.call_args_list[1].args[1], "owned_target")
        self.assertEqual(compare.call_args.args[-1], "owned_target")
        self.assertEqual(observation["database_cleanup"], "caller_owned_cluster_finally")
        self.cluster.cleanup.assert_not_called()  # Caller must clean both DBs after the callback.
        self.assertFalse(observation["qualified"])
        self.assertEqual(self.result["state"], "INCOMPLETE")
        self.assertTrue(all(case["state"] == "NOT_RUN" for case in self.result["full_cases"]))

    def test_bad_copy_prevents_config_rebinding_or_restored_start(self):
        with self.orchestration(offline_failure=True) as (events, _kit, compare):
            with self.assertRaisesRegex(JourneyError, "RESTORED_FILES_MISMATCH"):
                self.run_roundtrip()
        compare.assert_not_called()
        self.assertEqual(events, ["backup", "verify", "restore", "offline-verification"])
        self.assertEqual(self.result["state"], "FAILED")
        self.assertIn("owned_target", self.cluster.created)  # Retained for caller's finally cleanup.

    def test_nonempty_target_cannot_receive_restore_or_start(self):
        with self.orchestration(target_nonempty=True) as (events, _kit, compare):
            with self.assertRaisesRegex(JourneyError, "TARGET_NOT_EMPTY"):
                self.run_roundtrip()
        compare.assert_not_called()
        self.assertEqual(events, ["backup", "verify"])
        self.assertEqual(self.result["state"], "FAILED")

    def test_previously_owned_database_is_not_a_fresh_restore_target(self):
        self.cluster.created.append("owned_target")
        self.cluster.create.side_effect = None
        self.cluster.create.return_value = "owned_target"
        with self.orchestration() as (events, _kit, compare):
            with self.assertRaisesRegex(JourneyError, "TARGET_NOT_NEW_OWNED_DATABASE"):
                self.run_roundtrip()
        compare.assert_not_called()
        self.assertEqual(events, ["backup", "verify"])

    def test_overlong_restored_control_socket_fails_before_kit_or_database_use(self):
        self.root = self.sandbox / ("maintenance" * 20)
        with patch("journey_maintenance.InstalledKit") as kit:
            with self.assertRaisesRegex(JourneyError, "RESTORE_PATH_TOO_LONG"):
                self.run_roundtrip()
        kit.assert_not_called()
        self.cluster.create.assert_not_called()

    def test_foreign_source_is_refused_before_kit_database_allocation_or_launch(self):
        self.cluster.created.clear()
        with patch("journey_maintenance.InstalledKit") as kit:
            with self.assertRaisesRegex(JourneyError, "SOURCE_NOT_OWNED"):
                self.run_roundtrip()
        kit.assert_not_called()
        self.cluster.create.assert_not_called()
        self.assertFalse(self.root.exists())

    @contextmanager
    def restored_runtime(self, *, capture_changed=False, ledger_changed=False, mcp_changed=False, generation="restored"):
        events, kit = [], self.kit()
        @contextmanager
        def child(args, cwd, environment):
            events.append("start")
            self.assertEqual(environment["PATH"], "")
            self.assertEqual(args[-1], "serve")
            try:
                yield Mock()
            finally:
                events.append("owned-context-cleanup")
        _, record = installation_identity(self.output / "config.toml")
        roots = {"config": self.output / "config.toml", "data": self.output / "data", "state": record.parent}
        with ExitStack() as stack:
            stack.enter_context(patch("journey_maintenance.runtime_roots", return_value=roots))
            stack.enter_context(patch("journey_maintenance.owned_child", child))
            stack.enter_context(patch("journey_maintenance.wait_owned_ready", return_value={"instance": {"boot_generation": generation}}))
            stack.enter_context(patch("journey_maintenance.stored_capture", return_value=self.source["capture"] |
                                      ({"request_body_sha256": "changed"} if capture_changed else {})))
            stack.enter_context(patch("journey_maintenance.migration_ledger", return_value=self.source["ledger"] |
                                      ({"full_ledger_sha256": "changed"} if ledger_changed else {})))
            stack.enter_context(patch("journey_maintenance.run_mcp", return_value=self.source["mcp"] |
                                      {"protocol_frames": 19} | ({"tool_count": 99} if mcp_changed else {})))
            stop = stack.enter_context(patch("journey_maintenance.graceful_stop", side_effect=lambda _c: events.append("graceful-stop")))
            yield events, kit, stop

    def test_restored_runtime_compares_capture_ledger_mcp_and_new_owned_generation(self):
        with self.restored_runtime() as (events, kit, stop):
            restore_and_compare(kit, self.manifest, self.result, self.cluster, self.candidate_inventory,
                                 self.source, self.output, "owned_target")
        self.assertEqual(events, ["start", "graceful-stop", "owned-context-cleanup"])
        stop.assert_called_once()
        self.assertTrue(all(entry["state"] == "PASSED" for entry in self.result["checks"]))

    def test_changed_capture_still_cleans_only_owned_context_and_cannot_pass(self):
        with self.restored_runtime(capture_changed=True) as (events, kit, stop):
            with self.assertRaisesRegex(JourneyError, "RESTORED_CAPTURE_CHANGED"):
                restore_and_compare(kit, self.manifest, self.result, self.cluster, self.candidate_inventory,
                                     self.source, self.output, "owned_target")
        self.assertEqual(events, ["start", "owned-context-cleanup"])
        self.assertEqual(self.result["state"], "FAILED")
        stop.assert_not_called()

    def assert_runtime_refused(self, code, **options):
        with self.restored_runtime(**options) as (events, kit, stop):
            with self.assertRaisesRegex(JourneyError, code):
                restore_and_compare(kit, self.manifest, self.result, self.cluster, self.candidate_inventory,
                                     self.source, self.output, "owned_target")
        self.assertEqual(events, ["start", "owned-context-cleanup"])
        self.assertEqual(self.result["state"], "FAILED")
        stop.assert_not_called()

    def test_restored_ledger_change_cannot_pass(self):
        self.assert_runtime_refused("RESTORED_LEDGER_CHANGED", ledger_changed=True)

    def test_restored_mcp_change_cannot_pass(self):
        self.assert_runtime_refused("RESTORED_MCP_CHANGED", mcp_changed=True)

    def test_source_boot_generation_cannot_be_reused(self):
        self.assert_runtime_refused("BOOT_GENERATION_REUSED", generation="initial")


class InstalledKitTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-kit-offline-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.prefix = self.root / "prefix"
        self.state = self.prefix / "lib/mike-archive"
        self.identity = "a" * 64
        self.generation = self.state / "generations" / self.identity
        self.generation.mkdir(parents=True, mode=0o700)
        self.paths = {name: self.generation / name for name in ("mike", "mike-mcp")}
        self.binding = {"manifest_sha256": self.identity}
        self.manifest = {"version": "0.1.0", "source_commit": "b" * 40,
                         "artifacts": [{"binary": "mike", "contents": []}]}
        for name, mode in (("mike-maintenance", 0o755), ("mike-maintenance.pyz", 0o644)):
            path = self.generation / name
            path.write_bytes(b"inert file: this offline test never executes it")
            path.chmod(mode)
            self.manifest["artifacts"][0]["contents"].append({"path": name, "sha256": sha256(path), "size": path.stat().st_size})
        self.inventory = {"offline_inventory": True}
        self.enterContext(patch.dict(os.environ, {}, clear=True))
        self.installed = self.enterContext(patch("journey_maintenance.installed", return_value=(
            self.prefix, self.state, self.identity, self.manifest)))
        self.enterContext(patch("journey_maintenance.verify_binding", return_value=(self.paths, self.binding)))
        self.identity_check = self.enterContext(patch("journey_maintenance.verified_migration_inventory", return_value=self.inventory))
        self.commands = self.enterContext(patch("journey_maintenance.captured_run"))
        self.kit = InstalledKit(self.paths, self.manifest, self.binding, self.prefix, self.inventory, self.root, self.root)

    def receipt(self, status="backup-complete"):
        return {"created_utc": "2026-10-13T00:00:00Z", "version": "0.1.0", "source_commit": "b" * 40,
                "postgres_major": 19, "files": 3, "bytes": 10, "status": status, "qualified": False}

    def test_kit_is_manifest_hash_bound_and_rechecked_before_every_command(self):
        self.commands.return_value = json.dumps(self.receipt()).encode()
        observation = self.kit.command("backup", ["--destination", "/offline-target"])
        self.assertEqual(set(self.kit.recheck()["kit_sha256"]), {"mike-maintenance", "mike-maintenance.pyz"})
        self.assertEqual(observation["status"], "backup-complete")
        args, cwd, env = self.commands.call_args.args
        self.assertEqual(args[0], str(self.generation / "mike-maintenance"))
        self.assertIn("--confirm-stopped-writers", args)
        self.assertEqual(env["HOME"], str(self.root))
        self.assertEqual(set(env), {"HOME", "TMPDIR", "PATH", "LANG", "LC_ALL"})
        self.assertEqual(self.commands.call_args.kwargs["timeout"], 360)
        (self.generation / "mike-maintenance.pyz").write_bytes(b"tampered")
        with self.assertRaisesRegex(JourneyError, "KIT_HASH_MISMATCH"):
            self.kit.command("backup", [])
        self.assertEqual(self.commands.call_count, 1)

    def test_missing_manifest_kit_wrong_generation_or_inventory_prevents_execution(self):
        self.identity_check.return_value = {"different": True}
        with self.assertRaisesRegex(JourneyError, "CANDIDATE_INVENTORY_CHANGED"):
            self.kit.command("backup", [])
        self.identity_check.return_value = self.inventory
        self.manifest["artifacts"][0]["contents"].clear()
        with self.assertRaisesRegex(JourneyError, "KIT_NOT_IN_MANIFEST"):
            self.kit.command("backup", [])
        self.commands.assert_not_called()

    def test_receipt_rejects_qualification_identity_changes_and_extra_private_fields(self):
        for changes in ({"qualified": True}, {"source_commit": "c" * 40}, {"private": "/secret"},
                         {"postgres_major": True}, {"files": True}, {"status": "wrong"}):
            self.commands.return_value = json.dumps(self.receipt() | changes).encode()
            with self.subTest(changes=changes), self.assertRaisesRegex(JourneyError, "RECEIPT_INVALID"):
                self.kit.command("backup", [])

    def test_explicit_host_tools_are_validated_and_ambient_credentials_are_never_inherited(self):
        self.commands.return_value = json.dumps(self.receipt()).encode()
        with patch.dict(os.environ, {"MIKE_RECOVERY_TEST_TOOL_DIR": "/explicit/tools", "PGPASSWORD": "private",
                                    "LD_LIBRARY_PATH": "/verifier-only", "PYTHONPATH": "/poison"}), \
                patch("journey_maintenance.selected_tools") as selected:
            kit = InstalledKit(self.paths, self.manifest, self.binding, self.prefix, self.inventory, self.root, self.root)
            kit.command("backup", [])
        selected.assert_called_once_with("/explicit/tools", ("psql", "pg_dump", "pg_restore"))
        args, _, env = self.commands.call_args.args
        self.assertEqual(args[args.index("--tool-directory") + 1], "/explicit/tools")
        self.assertNotIn("PGPASSWORD", env)
        self.assertNotIn("LD_LIBRARY_PATH", env)
        self.assertNotIn("PYTHONPATH", env)


if __name__ == "__main__":
    unittest.main()
