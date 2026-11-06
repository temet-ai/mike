#!/usr/bin/env python3
"""Mise journey helper tests against stubs. They never qualify a candidate archive."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from common import sha256
import journey_mise
from journey import check, new_result
from journey_process import JourneyError


class MiseJourneyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="mike-mise-journey-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()

    def manifest(self):
        snapshot = self.root / "snapshot"
        snapshot.mkdir()
        artifacts = []
        for name in journey_mise.TOOLS:
            archive = f"{name}-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
            (snapshot / archive).write_bytes(b"archive bytes for " + name.encode())
            artifacts.append({"binary": name, "archive": archive, "sha256": sha256(snapshot / archive),
                              "contents": [{"path": name, "sha256": "0" * 64}]})
        return snapshot, {"version": "1.2.3", "source_commit": "a" * 40,
                          "target": "x86_64-unknown-linux-gnu", "artifacts": artifacts}

    def test_layout_is_private_and_environment_is_clean(self):
        layout = journey_mise.make_layout(self.root / "out")
        environment = journey_mise.mise_environment(layout, Path("/usr/bin"))
        self.assertEqual(environment["PATH"], "/usr/bin")
        self.assertTrue(environment["MISE_DATA_DIR"].startswith(str(layout["mise"])))
        self.assertNotIn("SSH_AUTH_SOCK", environment)
        self.assertEqual(environment["HOME"], str(layout["home"]))
        self.assertEqual(layout["home"].stat().st_mode & 0o077, 0)

    def test_default_config_reads_the_instance_record_from_the_xdg_state_directory(self):
        home = self.root / "home"
        default = home / ".config/mike/config.toml"
        self.assertEqual(journey_mise.record_path_for(default), home / ".local/state/mike/instance.json")
        other = home / "elsewhere/config.toml"
        self.assertEqual(journey_mise.record_path_for(other).parent.name, "state")
        self.assertIn(".mike-", str(journey_mise.record_path_for(other)))

    def test_real_claude_phase_requires_token_and_executable(self):
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(JourneyError) as raised:
                journey_mise.real_claude_phase({}, Path("mike"), Path("c"), {}, [1, 2, 3], {}, self.root / "claude", {})
        self.assertEqual(str(raised.exception), "REAL_CLAUDE_INPUT_MISSING")

    def test_registry_holds_the_same_bytes_under_two_versions(self):
        layout = journey_mise.make_layout(self.root / "out")
        snapshot, manifest = self.manifest()
        versions, digests = journey_mise.stage_registry(layout, snapshot, manifest, "f" * 64)
        self.assertEqual(versions, ("1.2.3", "1.2.3.1"))
        for artifact in manifest["artifacts"]:
            for version in versions:
                self.assertEqual(sha256(layout["registry"] / version / artifact["archive"]), digests[artifact["binary"]])

    def test_registry_refuses_changed_archive(self):
        layout = journey_mise.make_layout(self.root / "out")
        snapshot, manifest = self.manifest()
        (snapshot / manifest["artifacts"][0]["archive"]).write_bytes(b"changed")
        with self.assertRaisesRegex(JourneyError, "REGISTRY_ARCHIVE_HASH_MISMATCH"):
            journey_mise.stage_registry(layout, snapshot, manifest, "f" * 64)

    def test_project_file_names_both_tools_and_is_replaced_for_the_upgrade(self):
        layout = journey_mise.make_layout(self.root / "out")
        _, manifest = self.manifest()
        journey_mise.write_project(layout, manifest, "1.2.3")
        journey_mise.write_project(layout, manifest, "1.2.3.1")
        text = (layout["project"] / "mise.toml").read_text()
        self.assertIn('[tools."http:mike"]', text)
        self.assertIn('[tools."http:mike-mcp"]', text)
        self.assertIn('version = "1.2.3.1"', text)
        self.assertNotIn('version = "1.2.3"\n', text)
        self.assertIn("file://", text)
        self.assertNotIn("http://", text)

    def stub_mise(self, installs):
        stub = self.root / "bin" / "mise"
        stub.parent.mkdir(exist_ok=True)
        stub.write_text("#!/bin/sh\nif [ \"$1\" = bin-paths ]; then printf '%s\\n' " + " ".join(f"'{p}'" for p in installs) + "; fi\n")
        stub.chmod(0o700)
        return stub

    def test_installed_binaries_must_match_manifest_hashes(self):
        layout = journey_mise.make_layout(self.root / "out")
        _, manifest = self.manifest()
        install = self.root / "install"
        install.mkdir()
        for artifact in manifest["artifacts"]:
            binary = install / artifact["binary"]
            binary.write_bytes(b"binary " + artifact["binary"].encode())
            binary.chmod(0o700)
            artifact["contents"][0]["sha256"] = sha256(binary)
        stub = self.stub_mise([install])
        paths = journey_mise.installed_binaries(layout, stub, manifest)
        self.assertEqual(set(paths), set(journey_mise.TOOLS))
        (install / "mike").write_bytes(b"tampered")
        with self.assertRaisesRegex(JourneyError, "MISE_INSTALLED_BINARY_HASH_MISMATCH"):
            journey_mise.installed_binaries(layout, stub, manifest)

    def test_missing_install_is_a_failure(self):
        layout = journey_mise.make_layout(self.root / "out")
        _, manifest = self.manifest()
        with self.assertRaisesRegex(JourneyError, "MISE_INSTALLED_BINARY_MISSING"):
            journey_mise.installed_binaries(layout, self.stub_mise([]), manifest)

    def test_output_must_be_new_and_outside_checkout_and_candidate(self):
        candidate = self.root / "candidate"
        candidate.mkdir()
        with self.assertRaisesRegex(JourneyError, "MISE_OUTPUT_INSIDE_CHECKOUT_OR_CANDIDATE"):
            journey_mise.validate_output(candidate / "out", candidate)
        existing = self.root / "existing"
        existing.mkdir()
        with self.assertRaisesRegex(JourneyError, "MISE_OUTPUT_NOT_NEW"):
            journey_mise.validate_output(existing, candidate)
        output = journey_mise.validate_output(self.root / "new", candidate)
        self.assertTrue(output.is_dir())

    def test_stop_refuses_a_process_that_is_not_the_named_binary(self):
        record = {"pid": os.getpid(), "generation": "g"}
        with patch.object(journey_mise, "identity_record", return_value=record):
            with self.assertRaisesRegex(JourneyError, "MISE_STOP_NOT_OWNED"):
                journey_mise.stop_recorded(self.root / "config.toml", self.root / "not-python")

    def test_binding_and_summary_never_qualify(self):
        _, manifest = self.manifest()
        binding = journey_mise.binding_for(manifest, "e" * 64)
        self.assertEqual(binding["source_commit"], "a" * 40)
        result = new_result(binding)
        check(result, "step", lambda: {"ok": True})
        with self.assertRaises(RuntimeError):
            check(result, "broken", lambda: (_ for _ in ()).throw(RuntimeError("x")))
        self.assertEqual(result["state"], "FAILED")
        self.assertFalse(result["qualified"])
        self.assertTrue(all(c["state"] == "NOT_RUN" for c in result["full_cases"]))
        self.assertIn("FAILED", journey_mise.summary(result))

    def test_missing_mise_is_reported(self):
        with patch.object(journey_mise.shutil, "which", return_value=None):
            with self.assertRaisesRegex(JourneyError, "MISE_NOT_ON_PATH"):
                journey_mise.run_journey(self.root / "candidate", self.root / "out")

    def test_failed_candidate_writes_failed_evidence_and_exit_one(self):
        with patch.object(journey_mise.shutil, "which", return_value="/usr/bin/true"):
            code = journey_mise.main(["--candidate", str(self.root / "absent"), "--output", str(self.root / "out")])
        self.assertEqual(code, 1)
        value = json.loads((self.root / "out" / "result.json").read_text())
        self.assertEqual(value["state"], "FAILED")
        self.assertFalse(value["qualified"])
        self.assertEqual((self.root / "out" / "result.json").stat().st_mode & 0o077, 0)


if __name__ == "__main__":
    unittest.main()
