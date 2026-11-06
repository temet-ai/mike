"""Offline installed-removal contracts; mocked CLI output is never qualification."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from common import sha256
from journey_process import JourneyError, installation_identity, private_write
from journey_reinstall import registration_roundtrip, retained_reinstall


class ReinstallTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="m-reinstall-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.home, self.cwd = self.root / "home", self.root / "cwd"
        self.home.mkdir(mode=0o700)
        self.cwd.mkdir(mode=0o700)
        self.config = self.root / "config.toml"
        private_write(self.config, b"persist_enabled = true\n")
        self.prefix = self.root / "prefix"
        self.generation = self.prefix / "lib/mike-archive/generations" / ("a" * 64)
        self.generation.mkdir(parents=True, mode=0o700)
        (self.prefix / "bin").mkdir(mode=0o700)
        for name in ("mike", "mike-mcp", "mike-maintenance", "mike-maintenance.pyz"):
            private_write(self.generation / name, b"inert: this test executes nothing\n")
        self.restore_links()
        self.candidate = Mock(prefix=self.prefix, snapshot=self.root / "snapshot", identity="a" * 64,
                              paths={name: self.generation / name for name in ("mike", "mike-mcp")},
                              manifest={"version": "0.2.0", "source_commit": "b" * 40})
        self.kit = Mock(launcher=self.generation / "mike-maintenance")
        self.kit.recheck.return_value = {"kit_sha256": {name: sha256(self.generation / name)
            for name in ("mike-maintenance", "mike-maintenance.pyz")}}
        self.runtime = Mock(config=self.config, home=self.home, cwd=self.cwd, ports=[12001, 12002, 12003])
        self.runtime.kit.return_value = self.kit
        self.roots = {"state": self.root / "state"}
        self.roots["state"].mkdir(mode=0o700)
        self.bundle = self.root / "backup"
        self.bundle.mkdir(mode=0o700)
        private_write(self.bundle / "inert-backup", b"private synthetic backup bytes\n")
        self.snapshot = self.enterContext(patch("journey_reinstall.durable_snapshot", return_value={"all": "unchanged"}))
        self.commands = self.enterContext(patch("journey_reinstall.captured_run"))
        self.process = self.enterContext(patch("journey_reinstall.OwnedProcess"))
        self.child = self.process.return_value
        self.child.returncode = 1
        self.child.communicate.return_value = (b"", b"changed or foreign mike registration")

    def restore_links(self):
        for name in ("mike", "mike-mcp"):
            (self.prefix / "bin" / name).symlink_to(self.generation / name)

    def uninstall_command(self, args, *_args, **_kwargs):
        if args[1] == "uninstall":
            for name in ("mike", "mike-mcp"):
                (self.prefix / "bin" / name).unlink()
            return json.dumps({"status": "owned-commands-unlinked", "data_retained": True,
                "configuration_retained": True, "database_resources_retained": True, "backups_retained": True,
                "recovery_generations_retained": True, "integrations_changed": False}).encode()
        self.assertEqual(args[1], "install")
        self.restore_links()
        return json.dumps({"version": "0.2.0", "source_commit": "b" * 40,
            "manifest_sha256": "a" * 64, "prefix": str(self.prefix), "status": "unchanged", "qualified": False}).encode()

    def reinstall(self):
        return retained_reinstall(self.runtime, self.candidate, self.roots, self.root / "service", self.bundle)

    def test_exact_installed_kit_unlinks_only_pair_then_restores_with_retained_bytes(self):
        self.commands.side_effect = self.uninstall_command
        result = self.reinstall()
        self.assertTrue(result["stable_pair_restored"])
        self.assertFalse(result["qualified"])
        calls = self.commands.call_args_list
        self.assertEqual([call.args[0][1] for call in calls], ["uninstall", "install"])
        self.assertTrue(all(call.args[0][0] == str(self.kit.launcher) for call in calls))
        self.assertIn("--confirm-stopped-writers", calls[0].args[0])
        self.assertTrue((self.prefix / "bin/unrelated-synthetic-command").is_file())
        self.assertTrue(all((self.prefix / "bin" / name).is_symlink() for name in ("mike", "mike-mcp")))
        self.candidate.bind.assert_called_once_with(self.prefix)
        self.process.assert_not_called()

    def test_changed_state_backup_generation_or_kit_prevents_reinstall(self):
        self.commands.side_effect = self.uninstall_command
        self.snapshot.side_effect = [{"all": "unchanged"}, {"all": "mutated"}]
        with self.assertRaisesRegex(JourneyError, "UNINSTALL_CHANGED_RETAINED_STATE"):
            self.reinstall()
        self.assertEqual(self.commands.call_count, 1)
        self.candidate.bind.assert_not_called()

    def test_wrong_receipt_does_not_continue_to_install(self):
        self.commands.return_value = b'{"status":"owned-commands-unlinked","qualified":true}'
        with self.assertRaisesRegex(JourneyError, "UNINSTALL_RECEIPT"):
            self.reinstall()
        self.assertEqual(self.commands.call_count, 1)

    def test_restore_receipt_cannot_claim_a_different_candidate(self):
        def changed(args, *positional, **kwargs):
            raw = self.uninstall_command(args, *positional, **kwargs)
            if args[1] == "install":
                value = json.loads(raw)
                value["manifest_sha256"] = "f" * 64
                return json.dumps(value).encode()
            return raw
        self.commands.side_effect = changed
        with self.assertRaisesRegex(JourneyError, "EXACT_CANDIDATE_RECEIPT"):
            self.reinstall()
        self.candidate.bind.assert_not_called()

    def registration_command(self, args, *_args, **_kwargs):
        operation = args[4]
        client = Path(args[args.index("--claude-config") + 1])
        before = client.read_bytes()
        document = json.loads(before)
        backup = client.with_name(client.name + ".mike-backup-" + operation)
        private_write(backup, before)
        value = {"client_config": str(client), "changed": True, "backup": str(backup)}
        if operation == "register":
            identity, _ = installation_identity(self.config)
            document["mcpServers"]["mike"] = {"command": str(self.prefix / "bin/mike-mcp"), "args": ["--core"],
                "env": {"MIKE_INSPECT_URL": "http://127.0.0.1:12002", "MIKE_REGISTRATION_INSTANCE_ID": identity}}
            value["probe"] = {"handshake": True, "backend_read": True, "instance_id": identity, "version": "0.2.0"}
        else:
            self.assertEqual(operation, "unregister")
            document["mcpServers"].pop("mike")
        client.write_text(json.dumps(document))
        return json.dumps(value).encode()

    def test_registration_uses_stable_pair_exact_owned_probe_and_preserves_foreign_entry(self):
        self.commands.side_effect = self.registration_command
        result = registration_roundtrip(self.runtime, self.candidate)
        self.assertTrue(result["unrelated_entry_and_settings_preserved"])
        self.assertTrue(result["changed_registration_refused"])
        client = json.loads((self.home / "synthetic-claude.json").read_bytes())
        self.assertEqual(client, {"theme": "synthetic-preserve", "mcpServers": {"other": {"command": "never-executed"}}})
        self.assertEqual(self.commands.call_count, 2)
        self.assertEqual(self.process.call_args.args[0][4], "unregister")
        self.child.close.assert_called_once()

    def test_registration_probe_for_foreign_instance_is_not_accepted(self):
        def foreign(args, *positional, **kwargs):
            value = json.loads(self.registration_command(args, *positional, **kwargs))
            value["probe"]["instance_id"] = "foreign"
            return json.dumps(value).encode()
        self.commands.side_effect = foreign
        with self.assertRaisesRegex(JourneyError, "NOT_CONNECTED_TO_OWNED_INSTANCE"):
            registration_roundtrip(self.runtime, self.candidate)
        self.assertEqual(self.commands.call_count, 1)
        self.process.assert_not_called()

    def test_changed_registration_refusal_checks_exact_client_bytes(self):
        self.commands.side_effect = self.registration_command
        def mutate(*_args, **_kwargs):
            changed = self.home / "changed-synthetic-claude.json"
            changed.write_bytes(changed.read_bytes() + b"\n")
            return b"", b"changed or foreign mike registration"
        self.child.communicate.side_effect = mutate
        with self.assertRaisesRegex(JourneyError, "CHANGED_REGISTRATION_MUTATED"):
            registration_roundtrip(self.runtime, self.candidate)
        self.child.close.assert_called_once()

    def test_unrelated_environment_error_is_not_safe_registration_refusal(self):
        self.commands.side_effect = self.registration_command
        self.child.communicate.return_value = (b"", b"permission denied")
        with self.assertRaisesRegex(JourneyError, "CHANGED_REGISTRATION_NOT_REFUSED"):
            registration_roundtrip(self.runtime, self.candidate)
        self.child.close.assert_called_once()


if __name__ == "__main__":
    unittest.main()
