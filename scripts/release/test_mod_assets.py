"""Mods survive exact archive extraction and immutable generation validation."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from archive import checked_inventory, extract_checked, pack
from common import ReleaseError
from inputs import extract_pair
from install_state import clean_pending, validate_generation, write_owner
from mod_assets import payload


class ModAssetsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-mod-assets-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.repo = self.root / "repo"
        self.repo.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        self.plugin = "mods/mike/.claude-plugin/plugin.json"
        for name, data in ((self.plugin, b'{"name":"mike"}\n'),
                           ("mods/mike/hooks/run.sh", b"#!/bin/sh\nexit 0\n"),
                           ("mods/mike/tests/test.py", b"test only")):
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        (self.repo / "mods/mike/hooks/run.sh").chmod(0o755)
        subprocess.run(["git", "-C", str(self.repo), "add", "mods"], check=True)
        (self.repo / "LICENSE").write_text("fixture")

    def candidate(self):
        snapshot = self.root / "candidate"
        snapshot.mkdir()
        artifacts = []
        for name in ("mike", "mike-mcp"):
            binary = self.repo / name
            binary.write_text("fixture binary")
            archive = snapshot / (name + ".tar.gz")
            contents = pack(archive, binary, self.repo / "LICENSE", b"{}", 1)
            artifacts.append(dict(binary=name, archive=archive.name, contents=contents))
        manifest = dict(artifacts=artifacts)
        (snapshot / "manifest.json").write_text(json.dumps(manifest))
        return snapshot, manifest

    def test_only_tracked_runtime_assets_ship_with_exact_modes_and_bytes(self):
        (self.repo / self.plugin).write_text("uncommitted replacement")
        (self.repo / "mods/mike/untracked").write_text("untracked")
        files = {name: (data, mode) for name, data, mode in payload(self.repo)}
        self.assertEqual(files["share/mike/" + self.plugin], (b'{"name":"mike"}\n', 0o644))
        self.assertEqual(files["share/mike/mods/mike/hooks/run.sh"][1], 0o755)
        self.assertEqual(len(files), 2)

    def test_installed_assets_are_verified_and_partial_extraction_can_be_cleaned(self):
        snapshot, manifest = self.candidate()
        stage = self.root / "stage"
        stage.mkdir(mode=0o700)
        write_owner(stage)
        identity = extract_pair(snapshot, manifest, stage)
        validate_generation(stage, identity)
        asset = stage / ("share/mike/" + self.plugin)
        original = asset.read_bytes()
        asset.write_text("modified")
        with self.assertRaisesRegex(ReleaseError, "changed"):
            validate_generation(stage, identity)
        asset.write_bytes(original)
        (stage / ".mike").mkdir(mode=0o700)
        (stage / "share").rename(stage / ".mike/share")
        clean_pending(stage)
        self.assertFalse(stage.exists())

    def test_unsafe_asset_paths_and_symlinked_extraction_parents_are_refused(self):
        snapshot, manifest = self.candidate()
        artifact = manifest["artifacts"][0]
        for path in ("share/mike/mods/../../escape", "/share/mike/mods/escape", "share/mike/mods//escape"):
            inventory = artifact["contents"] + [dict(path=path, size=1, sha256="a" * 64, mode="0o644")]
            with self.assertRaises(ReleaseError):
                checked_inventory("mike", inventory)
        target = self.root / "extract"
        target.mkdir(mode=0o700)
        (target / "share").symlink_to(self.repo, target_is_directory=True)
        with self.assertRaises(ReleaseError):
            extract_checked(snapshot / artifact["archive"], target, "mike", artifact["contents"])
