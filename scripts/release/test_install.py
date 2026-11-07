#!/usr/bin/env python3
"""Disposable fixture installs and failure injection; no real release qualification."""

import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from archive import BINARIES, binary_identity, pack, smoke
from notice_inputs import BASE as NOTICE_BASE, encoded as encoded_notice
from common import ReleaseError, TARGETS, sha256, write_json
from inputs import clean_environment, owned_run, verified_inputs
from install import install, install_verified, semver
from install_state import OWNER, STATE_NAME, initialize, link_target, validate_generation
from migration_inventory import SOURCE_PATH

ROOT = Path(__file__).resolve().parents[2]
TARGET = next(target for target, host in TARGETS.items()
              if host[:2] == (os.uname().sysname, os.uname().machine))


def await_stopped_child(proc, timeout=2):
    import time
    deadline = time.monotonic() + timeout
    state = "unobserved"
    while time.monotonic() < deadline:
        try:
            state = proc.read_text().rsplit(")", 1)[1].split()[0]
        except FileNotFoundError:
            return None
        if state == "Z":
            return state
        time.sleep(0.01)
    return state


class ChildExitTests(unittest.TestCase):
    def test_exit_wait_never_accepts_a_live_child_at_the_deadline(self):
        from unittest.mock import Mock
        proc = Mock()
        proc.read_text.return_value = "123 (fixture with spaces) R 0 0"
        with patch("time.monotonic", side_effect=[0, 0, 3]), patch("time.sleep"):
            self.assertEqual(await_stopped_child(proc), "R")
        with patch("time.monotonic", side_effect=[0, 3]):
            self.assertEqual(await_stopped_child(proc), "unobserved")

    def test_exit_wait_accepts_only_disappearance_or_zombie(self):
        from unittest.mock import Mock
        proc = Mock()
        proc.read_text.side_effect = ["123 (fixture) S 0 0", "123 (fixture) Z 0 0"]
        with patch("time.sleep"):
            self.assertEqual(await_stopped_child(proc), "Z")
        proc.read_text.side_effect = FileNotFoundError
        self.assertIsNone(await_stopped_child(proc))


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mike-install-test-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.prefix = self.base / "prefix with spaces"
        self.candidate = self.make_candidate("0.1.0")

    def make_candidate(self, version, script_extra="", target=TARGET):
        root = self.base / ("candidate-" + str(len(list(self.base.iterdir()))))
        root.mkdir()
        build = self.base / ("build-" + root.name)
        build.mkdir()
        (build / "LICENSE").write_text("fixture MIT license\n")
        artifacts = []
        for name in BINARIES:
            binary = build / name
            identity = json.dumps({"schema_version": 1, "product": name, "version": version,
                                   "source_sha": "a" * 40, "source_dirty": False,
                                   "built_at": "1970-01-01T00:00:00Z"}, sort_keys=True)
            binary.write_text('#!/bin/sh\n' + script_extra + '\ncase "$1" in\n'
                              f'--version) printf "%s\\n" "{name} {version}" ;;\n'
                              f"--build-info) printf '%s\\n' '{identity}' ;;\n"
                              '--help) printf "%s\\n" "fixture help" ;;\n'
                              'list-tools) printf "%s\\n" \'[{"name":"fixture"}]\' ;;\n'
                              '*) exit 77 ;;\nesac\n')
            binary.chmod(0o755)
            archive = root / f"{name}-v{version}-{target}.tar.gz"
            inventory = pack(archive, binary, build / "LICENSE", encoded_notice(NOTICE_BASE), 0)
            digest = sha256(archive)
            (root / (archive.name + ".sha256")).write_text(f"{digest}  {archive.name}\n")
            home, cwd = build / (name + "-home"), build / (name + "-cwd")
            home.mkdir(); cwd.mkdir()
            artifacts.append({"binary": name, "archive": archive.name, "sha256": digest,
                              "contents": inventory, "smoke": smoke(binary, version, cwd, home),
                              "build_info": binary_identity(binary, version, "a" * 40, 0, cwd, home)})
        synthetic_inventory = {"schema_version": 1, "source_commit": "a" * 40,
                               "source_path": SOURCE_PATH, "checksum_algorithm": "sha384",
                               "migrations": [{"version": 1, "filename": "1_synthetic_fixture.sql",
                                   "checksum": hashlib.sha384(b"-- Synthetic fixture only\nSELECT 1;\n").hexdigest()}]}
        write_json(root / "manifest.json", {"schema_version": 1, "state": "prepared", "qualified": False,
                   "source_commit": "a" * 40, "source_date_epoch": 0, "version": version, "target": target,
                   "migration_inventory": synthetic_inventory, "artifacts": artifacts})
        return root

    def cli(self, candidate=None, prefix=None, env=None):
        cwd = self.base / "caller cwd"
        cwd.mkdir(exist_ok=True)
        return subprocess.run([str(ROOT / "scripts/install-release"), "--candidate", str(candidate or self.candidate),
                               "--prefix", str(prefix or self.prefix), "--json"], cwd=cwd,
                              env=env, text=True, capture_output=True, timeout=30)

    def current(self):
        return self.prefix / "lib" / STATE_NAME / "current"

    def rewrite_manifest(self, modify, candidate=None):
        path = (candidate or self.candidate) / "manifest.json"
        manifest = json.loads(path.read_text())
        modify(manifest)
        write_json(path, manifest)

    def test_fresh_and_repeated_install_keep_pair_stable(self):
        first = self.cli()
        self.assertEqual(first.returncode, 0, first.stderr)
        receipt = json.loads(first.stdout)
        self.assertFalse(receipt["qualified"])
        current = os.readlink(self.current())
        for name in BINARIES:
            self.assertEqual(os.readlink(self.prefix / "bin" / name), link_target(name))
            self.assertIn(f"{name} 0.1.0", subprocess.check_output([self.prefix / "bin" / name, "--version"], text=True))
        again = self.cli()
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertEqual(json.loads(again.stdout)["status"], "unchanged")
        self.assertEqual(os.readlink(self.current()), current)
        generations = self.current().parent / "generations"
        self.assertEqual(len(list(generations.iterdir())), 1)

    def test_verifier_replaces_environment_and_never_uses_developer_path(self):
        candidate = self.make_candidate("0.2.0", '[ -z "$PATH" ] || exit 41\n'
            '[ -z "$MIKE_SECRET$DATABASE_URL$VITE_SECRET$PYTHONPATH" ] || exit 42\n'
            '[ ! -e "$HOME/.env" ] || exit 43\n[ ! -e .env ] || exit 44\n')
        home = self.base / "operator-home"; home.mkdir()
        (home / ".env").write_text("private")
        result = self.cli(candidate, env=dict(os.environ, HOME=str(home), MIKE_SECRET="private",
                         DATABASE_URL="live-db-must-not-be-read", VITE_SECRET="private", PYTHONPATH="/bad"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((home / ".env").read_text(), "private")
        self.assertEqual(list(home.iterdir()), [home / ".env"])

    def test_checksum_mismatch_does_not_create_prefix(self):
        archive = next(self.candidate.glob("*.tar.gz"))
        archive.write_bytes(archive.read_bytes() + b"corrupt")
        result = self.cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_absolute_checksum_rejected(self):
        checksum = next(self.candidate.glob("*.sha256"))
        checksum.write_text(checksum.read_text().replace("  mike", "  /tmp/mike"))
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_platform_mismatch_rejected_before_install(self):
        target = "aarch64-apple-darwin" if TARGET.endswith("gnu") else "x86_64-unknown-linux-gnu"
        candidate = self.make_candidate("0.1.0", target=target)
        self.assertNotEqual(self.cli(candidate).returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_mixed_executable_versions_rejected(self):
        self.rewrite_manifest(lambda m: m.update(version="0.2.0"))
        for path in list(self.candidate.iterdir()):
            if "0.1.0" in path.name:
                changed = path.with_name(path.name.replace("0.1.0", "0.2.0"))
                path.rename(changed)
                if changed.suffix == ".sha256":
                    changed.write_text(changed.read_text().replace("0.1.0", "0.2.0"))
        self.rewrite_manifest(lambda m: [a.update(archive=a["archive"].replace("0.1.0", "0.2.0")) for a in m["artifacts"]])
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_wrong_observed_smoke_rejected(self):
        self.rewrite_manifest(lambda m: m["artifacts"][0]["smoke"].update(stdout_sha256="0" * 64))
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_traversal_member_with_updated_outer_checksum_rejected(self):
        path = next(self.candidate.glob("mike-v*.tar.gz"))
        with tarfile.open(path, "w:gz") as tar:
            item = tarfile.TarInfo("../escaped"); item.size = 4; item.mode = 0o755
            tar.addfile(item, io.BytesIO(b"evil"))
        digest = sha256(path)
        (self.candidate / (path.name + ".sha256")).write_text(f"{digest}  {path.name}\n")
        self.rewrite_manifest(lambda m: m["artifacts"][0].update(sha256=digest))
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertFalse(self.prefix.exists())
        self.assertFalse((self.base / "escaped").exists())

    def test_symlink_input_rejected(self):
        archive = next(self.candidate.glob("*.tar.gz"))
        original = self.base / "original"; archive.rename(original); archive.symlink_to(original)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_unexpected_candidate_files_rejected(self):
        (self.candidate / ".env").write_text("must-not-copy")
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_candidate_is_copied_before_later_mutations(self):
        with verified_inputs(self.candidate) as (snapshot, manifest):
            for path in self.candidate.glob("*.tar.gz"):
                path.write_bytes(b"changed after validation")
            result = install_verified(snapshot, manifest, self.prefix)
        self.assertEqual(result["status"], "installed")

    def test_unrelated_binary_is_preserved(self):
        (self.prefix / "bin").mkdir(parents=True)
        foreign = self.prefix / "bin" / "mike"; foreign.write_text("unrelated")
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(foreign.read_text(), "unrelated")
        self.assertFalse((self.prefix / "lib").exists())

    def test_brew_symlink_is_preserved(self):
        (self.prefix / "bin").mkdir(parents=True)
        foreign = self.prefix / "bin" / "mike"; foreign.symlink_to("../../Cellar/mike/bin/mike")
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(os.readlink(foreign), "../../Cellar/mike/bin/mike")

    def test_package_manager_prefix_refused(self):
        self.assertNotEqual(self.cli(prefix=self.base / "Cellar" / "mike").returncode, 0)
        self.assertFalse((self.base / "Cellar").exists())

    def test_symlink_prefix_and_bin_refused(self):
        elsewhere = self.base / "elsewhere"; elsewhere.mkdir()
        self.prefix.symlink_to(elsewhere)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(list(elsewhere.iterdir()), [])
        self.prefix.unlink(); self.prefix.mkdir()
        (self.prefix / "bin").symlink_to(elsewhere)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_group_writable_prefix_refused(self):
        self.prefix.mkdir(); self.prefix.chmod(0o777)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(list(self.prefix.iterdir()), [])

    def test_upgrade_switches_both_binaries_and_preserves_old_generation(self):
        install(self.candidate, self.prefix)
        previous = self.current().resolve()
        newer = self.make_candidate("0.2.0")
        install(newer, self.prefix)
        self.assertTrue(previous.is_dir())
        self.assertNotEqual(previous, self.current().resolve())
        for name in BINARIES:
            self.assertIn(f"{name} 0.2.0", subprocess.check_output([self.prefix / "bin" / name, "--version"], text=True))

    def test_downgrade_refused_with_recovery_guidance(self):
        newer = self.make_candidate("0.2.0")
        install(newer, self.prefix)
        previous = self.current().resolve()
        result = self.cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("recovery", result.stderr)
        self.assertEqual(previous, self.current().resolve())

    def test_changed_same_version_refused(self):
        install(self.candidate, self.prefix)
        other = self.make_candidate("0.1.0")
        self.rewrite_manifest(lambda m: m.update(source_commit="b" * 40), other)
        self.assertNotEqual(self.cli(other).returncode, 0)

    def test_modified_installed_binary_refused(self):
        install(self.candidate, self.prefix)
        installed = self.prefix / "bin" / "mike"; installed.write_text("unrelated replacement")
        result = self.cli(self.make_candidate("0.2.0"))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(installed.read_text(), "unrelated replacement")

    def test_modified_install_link_refused(self):
        install(self.candidate, self.prefix)
        link = self.prefix / "bin" / "mike"; link.unlink(); link.symlink_to("/unrelated")
        self.assertNotEqual(self.cli(self.make_candidate("0.2.0")).returncode, 0)
        self.assertEqual(os.readlink(link), "/unrelated")

    def test_concurrent_install_lock_is_bounded(self):
        state = initialize(self.prefix)
        lock = state / "lock"; lock.touch(mode=0o600)
        with lock.open("r+") as stream:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = self.cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("another installer", result.stderr)
        self.assertEqual(self.cli().returncode, 0)

    def test_exception_before_commit_keeps_previous_pair_and_retries(self):
        install(self.candidate, self.prefix)
        previous = self.current().resolve()
        newer = self.make_candidate("0.2.0")
        with patch("install.commit_generation", side_effect=OSError("fixture interruption")):
            with self.assertRaises(OSError):
                install(newer, self.prefix)
        self.assertEqual(self.current().resolve(), previous)
        self.assertEqual(self.cli(newer).returncode, 0)

    def test_killed_install_before_atomic_swap_recovers(self):
        install(self.candidate, self.prefix)
        previous = self.current().resolve()
        newer = self.make_candidate("0.2.0")
        code = '''import os, sys, signal
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import install, install_state
replace = install_state.os.replace
def crash(source, destination):
    if Path(destination).name == 'current': os.kill(os.getpid(), signal.SIGKILL)
    return replace(source, destination)
install_state.os.replace = crash
install.install(Path(sys.argv[2]), Path(sys.argv[3]))
'''
        result = subprocess.run([sys.executable, "-I", "-c", code, str(ROOT / "scripts/release"), str(newer), str(self.prefix)],
                                capture_output=True, timeout=30)
        self.assertEqual(result.returncode, -signal.SIGKILL, result.stderr)
        self.assertEqual(previous, self.current().resolve())
        self.assertTrue((self.current().parent / "next").is_symlink())
        self.assertEqual(self.cli(newer).returncode, 0)
        self.assertNotEqual(previous, self.current().resolve())

    def test_killed_first_install_recovers_stable_command_links(self):
        code = """import os, sys, signal
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import install, install_state
original = install_state.os.replace
def crash(source, destination):
    if Path(destination).name == 'current': os.kill(os.getpid(), signal.SIGKILL)
    return original(source, destination)
install_state.os.replace = crash
install.install(Path(sys.argv[2]), Path(sys.argv[3]))
"""
        result = subprocess.run([sys.executable, "-I", "-c", code, str(ROOT / "scripts/release"),
                                 str(self.candidate), str(self.prefix)], capture_output=True, timeout=30)
        self.assertEqual(result.returncode, -signal.SIGKILL, result.stderr)
        self.assertFalse(self.current().exists())
        self.assertTrue((self.prefix / "bin" / "mike").is_symlink())
        retry = self.cli()
        self.assertEqual(retry.returncode, 0, retry.stderr)
        self.assertTrue(self.current().exists())

    def test_term_during_verification_cleans_owned_child_and_scratch(self):
        import time
        flag, marker = self.base / "block", self.base / "blocked-pid"
        candidate = self.make_candidate("0.2.0", f'[ ! -e "{flag}" ] || {{ printf "%s" "$$" > "{marker}"; while :; do :; done; }}')
        flag.touch()
        before = set(Path("/tmp").glob("mike-local-candidate-*"))
        child = subprocess.Popen([ROOT / "scripts/install-release", "--candidate", candidate,
                                  "--prefix", self.prefix], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 5
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(marker.exists(), "fixture failed to reach verification")
            child.terminate()
            _, stderr = child.communicate(timeout=5)
            self.assertEqual(child.returncode, 1, stderr)
            proc = Path(f"/proc/{int(marker.read_text())}/stat")
            # killpg has delivered SIGKILL, but communicate() only reaps the
            # immediate verification wrapper. Its grandchild can briefly remain
            # runnable until the scheduler processes that fatal signal. Wait
            # for this exact fixture PID to disappear or become a zombie; never
            # accept a live/runnable/sleeping child as successful cleanup.
            self.assertIn(await_stopped_child(proc), (None, "Z"),
                          "owned verification child remained alive after cleanup")
            self.assertFalse(self.prefix.exists())
            self.assertEqual(before, set(Path("/tmp").glob("mike-local-candidate-*")))
        finally:
            if child.poll() is None:
                child.kill(); child.communicate(timeout=5)

    def test_partial_owned_stage_is_cleaned_only_for_selected_candidate(self):
        state = initialize(self.prefix)
        identity = sha256(self.candidate / "manifest.json")
        stage = state / "generations" / (".pending-" + identity); stage.mkdir()
        write_json(stage / "owner.json", OWNER)
        (stage / ".mike").mkdir(); (stage / ".mike" / "mike").write_text("partial")
        unrelated = state / "generations" / ".unrelated"; unrelated.mkdir()
        (unrelated / "keep").write_text("keep")
        self.assertEqual(self.cli().returncode, 0)
        self.assertFalse(stage.exists())
        self.assertEqual((unrelated / "keep").read_text(), "keep")

    def test_interrupted_ownership_receipt_recovers_before_extraction(self):
        state = initialize(self.prefix)
        identity = sha256(self.candidate / "manifest.json")
        stage = state / "generations" / (".pending-" + identity); stage.mkdir()
        (stage / "owner.json.pending").write_text('{"schema_')
        result = self.cli()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(stage.exists())

    def test_unrecognized_pending_content_never_deleted(self):
        state = initialize(self.prefix)
        identity = sha256(self.candidate / "manifest.json")
        stage = state / "generations" / (".pending-" + identity); stage.mkdir()
        write_json(stage / "owner.json", OWNER)
        (stage / "personal.txt").write_text("keep")
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual((stage / "personal.txt").read_text(), "keep")

    def test_pending_symlink_never_followed_or_deleted(self):
        state = initialize(self.prefix)
        identity = sha256(self.candidate / "manifest.json")
        stage = state / "generations" / (".pending-" + identity); stage.mkdir()
        write_json(stage / "owner.json", OWNER)
        outside = self.base / "keep"; outside.write_text("keep")
        (stage / "mike").symlink_to(outside)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(outside.read_text(), "keep")
        self.assertTrue((stage / "mike").is_symlink())

    def test_semver_ordering_is_conservative(self):
        versions = ["0.1.0-alpha.1", "0.1.0-alpha.2", "0.1.0-alpha.10", "0.1.0-beta", "0.1.0", "0.2.0"]
        self.assertEqual(sorted(versions, key=semver), versions)
        with self.assertRaises(ReleaseError): semver("0.1.0-01")


if __name__ == "__main__":
    unittest.main(verbosity=2)
