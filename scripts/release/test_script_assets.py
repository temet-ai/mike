"""The scripts payload: only tracked, non-test files in the ship list, required by the verifier."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from archive import check_scripts, checked_inventory, pack
from common import ReleaseError
import script_assets
from script_assets import SCRIPTS, check_inventory, payload, shipped


def stub_paths():
    """One stub file per ship-list entry; a directory entry gets one module, and a test beside it."""
    paths = []
    for entry in SCRIPTS:
        paths.append(entry + "module.py" if entry.endswith("/") else entry)
        if entry.endswith("/"):
            paths.append(entry + "test_module.py")
    return paths


def write_script_stubs(root):
    """Write the stubs under a repository root; the caller stages them. Returns the paths."""
    paths = stub_paths()
    for path in paths:
        target = Path(root) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("#!/bin/sh\nexit 0\n")
        target.chmod(0o644 if path.endswith(".py") else 0o755)
    return paths


class ScriptAssetsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-script-assets-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.repo = self.root / "repo"
        self.repo.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        self.paths = write_script_stubs(self.repo)
        (self.repo / "scripts/lessons-commit").write_text("#!/bin/sh\nexit 0\n")
        subprocess.run(["git", "-C", str(self.repo), "add", "scripts"], check=True)
        (self.repo / "scripts/untracked").write_text("untracked")
        (self.repo / "LICENSE").write_text("fixture")

    def test_only_tracked_listed_non_test_files_ship_with_their_modes(self):
        files = {name: (data, mode) for name, data, mode in payload(self.repo)}
        expected = {"share/mike/" + path for path in self.paths if shipped(path)}
        self.assertEqual(set(files), expected)
        self.assertEqual(files["share/mike/scripts/learning-promote"][1], 0o755)
        self.assertEqual(files["share/mike/scripts/learning_audit/module.py"][1], 0o644)
        self.assertNotIn("share/mike/scripts/lessons-commit", files)
        self.assertFalse(any("test_" in name or "untracked" in name for name in files))

    def test_a_listed_script_missing_from_the_source_stops_the_payload(self):
        subprocess.run(["git", "-C", str(self.repo), "rm", "-q", "-f", "scripts/mike-knowledge"], check=True)
        with self.assertRaisesRegex(ReleaseError, "mike-knowledge"):
            payload(self.repo)

    def test_a_root_without_scripts_ships_nothing(self):
        self.assertEqual(payload(self.root), [])

    def test_the_mike_archive_carries_the_payload_and_mcp_does_not(self):
        for name, shipped_scripts in (("mike", True), ("mike-mcp", False)):
            binary = self.repo / name
            binary.write_text("fixture binary")
            archive = self.root / (name + ".tar.gz")
            contents = pack(archive, binary, self.repo / "LICENSE", b"{}", 1)
            checked_inventory(name, contents)
            names = [item["path"] for item in contents if item["path"].startswith("share/mike/scripts/")]
            self.assertEqual(bool(names), shipped_scripts)
            if shipped_scripts:
                check_scripts({"script_assets": list(SCRIPTS)}, name, contents)

    def test_verifier_requires_every_recorded_entry_and_refuses_extras(self):
        paths = ["share/mike/" + path for path in self.paths if shipped(path)]
        self.assertEqual(check_inventory(paths, list(SCRIPTS)), [])
        short = [path for path in paths if not path.endswith("scripts/tool-reuse")]
        self.assertEqual(check_inventory(short, list(SCRIPTS)), ["missing scripts/tool-reuse"])
        extra = paths + ["share/mike/scripts/lessons-commit"]
        self.assertEqual(check_inventory(extra, list(SCRIPTS)), ["unexpected share/mike/scripts/lessons-commit"])
        self.assertEqual(check_inventory(paths, []), ["invalid script_assets list"])
        self.assertEqual(check_inventory(paths, ["/etc/passwd"]), ["invalid script_assets list"])

    def test_check_scripts_is_strict_for_new_manifests_and_legacy_for_old_ones(self):
        inventory = [dict(path="share/mike/scripts/learning-promote")]
        with self.assertRaisesRegex(ReleaseError, "unrecorded"):
            check_scripts({}, "mike", inventory)
        with self.assertRaisesRegex(ReleaseError, "unrecorded"):
            check_scripts({"script_assets": list(SCRIPTS)}, "mike-mcp", inventory)
        check_scripts({}, "mike", [dict(path="mike")])
        with self.assertRaisesRegex(ReleaseError, "do not match"):
            check_scripts({"script_assets": list(SCRIPTS)}, "mike", [dict(path="mike")])

    def test_unsafe_script_paths_are_refused_by_the_inventory_check(self):
        for path in ("share/mike/scripts/../../escape", "/share/mike/scripts/x", "share/mike/scripts//x"):
            inventory = [dict(path=path, size=1, sha256="a" * 64, mode="0o644")]
            with self.assertRaises(ReleaseError):
                checked_inventory("mike", inventory)

    @unittest.skipUnless(Path("/bin/sh").exists(), "needs a POSIX shell")
    def test_lesson_relations_finds_the_mike_beside_an_installed_package(self):
        scripts = self.root / "generation/share/mike/scripts"
        scripts.mkdir(parents=True)
        installed = scripts / "lesson-relations"
        installed.write_bytes((Path(__file__).resolve().parents[2] / "scripts/lesson-relations").read_bytes())
        installed.chmod(0o755)
        binary = self.root / "generation/mike"
        binary.write_text('#!/bin/sh\necho "beside $*"\n')
        binary.chmod(0o755)
        environment = {"PATH": "/usr/bin:/bin", "HOME": str(self.root / "home")}
        output = subprocess.run(["/bin/sh", str(installed), "--once"], env=environment,
                                capture_output=True, text=True, check=True).stdout
        self.assertEqual(output.strip(), "beside detect-lesson-relations --once")

    def test_the_manifest_key_is_json_serialisable(self):
        self.assertEqual(json.loads(json.dumps(list(script_assets.SCRIPTS))), list(script_assets.SCRIPTS))


if __name__ == "__main__":
    unittest.main()
