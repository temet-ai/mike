"""Checkout-free shipped maintenance contracts, using only disposable fixtures."""

import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import unittest
from unittest.mock import patch
import zipfile

from archive import SUPPORT_FILES, checked_inventory, extract_checked
from common import ReleaseError, sha256
from install import install
from install_state import clean_pending, initialize, validate_generation, write_owner
from maintenance_bundle import MODULES, payload
import test_install
import test_recovery
from test_recovery_tools import fixture_tools


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.fixture = test_install.InstallTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.base, self.candidate, self.prefix = self.fixture.base, self.fixture.candidate, self.fixture.prefix
        self.manifest = json.loads((self.candidate / "manifest.json").read_text())
        self.artifact = next(a for a in self.manifest["artifacts"] if a["binary"] == "mike")
        self.archive = self.candidate / self.artifact["archive"]
        self.cwd, self.home = self.base / "unrelated-cwd", self.base / "empty-home"
        self.cwd.mkdir(mode=0o700); self.home.mkdir(mode=0o700)
        self.poison = self.base / "unexpected-import"
        for name in (*MODULES, "json", "sitecustomize"):
            (self.cwd / (name + ".py")).write_text(
                f"from pathlib import Path; Path({str(self.poison)!r}).touch(); raise RuntimeError('wrong import')")
        self.env = {"HOME": str(self.home), "TMPDIR": str(self.home),
                    "PATH": str(Path(sys.executable).parent) + ":/usr/bin:/bin",
                    "PYTHONPATH": str(self.cwd), "PYTHONHOME": "/missing-python-home"}

    def run_helper(self, helper, *args):
        return subprocess.run([str(helper), *map(str, args)], cwd=self.cwd, env=self.env,
                              text=True, capture_output=True, timeout=30)

    def extract(self):
        destination = self.base / "extracted"
        destination.mkdir(mode=0o700)
        extract_checked(self.archive, destination, "mike", self.artifact["contents"])
        return destination

    def rewrite_archive(self, change):
        with tarfile.open(self.archive, "r:gz") as tar:
            members = [(item, tar.extractfile(item).read()) for item in tar.getmembers()]
        with tarfile.open(self.archive, "w:gz", format=tarfile.USTAR_FORMAT) as tar:
            for item, data in members:
                item, data = change(item, data)
                tar.addfile(item, io.BytesIO(data))
        digest = sha256(self.archive)
        (self.candidate / (self.archive.name + ".sha256")).write_text(f"{digest}  {self.archive.name}\n")
        self.fixture.rewrite_manifest(lambda m: m["artifacts"][0].update(sha256=digest))

    def test_payload_is_deterministic_and_contains_only_dependency_closure(self):
        first, second = payload(), payload()
        self.assertEqual(first, second)
        self.assertEqual({name: mode for name, _, mode in first}, SUPPORT_FILES)
        with zipfile.ZipFile(io.BytesIO(first[1][1])) as bundle:
            self.assertEqual(bundle.namelist(), ["__main__.py", *(name + ".py" for name in MODULES)])
            for name in MODULES:
                self.assertEqual(bundle.read(name + ".py"),
                                 (test_install.ROOT / "scripts/release" / (name + ".py")).read_bytes())
            self.assertTrue(all(i.date_time == (1980, 1, 1, 0, 0, 0) for i in bundle.infolist()))

    def test_helper_runs_after_its_producing_source_tree_is_deleted(self):
        producer = self.base / "producer"
        producer.mkdir()
        for name in (*MODULES, "maintenance_bundle"):
            shutil.copyfile(test_install.ROOT / "scripts/release" / (name + ".py"), producer / (name + ".py"))
        binary, license_path = self.base / "mike", self.base / "LICENSE"
        binary.write_text("fixture binary is not executed by this test\n")
        license_path.write_text("MIT fixture\n")
        archive = self.base / "checkout-free.tar.gz"
        code = ("import sys,json; from pathlib import Path; sys.path.insert(0,sys.argv[1]); "
                "from archive import pack; print(json.dumps(pack(Path(sys.argv[2]),Path(sys.argv[3]),"
                "Path(sys.argv[4]),b'{}',0)))")
        output = subprocess.check_output([sys.executable, "-I", "-B", "-c", code,
                                          producer, archive, binary, license_path], text=True)
        shutil.rmtree(producer)
        destination = self.base / "no-checkout"
        destination.mkdir(mode=0o700)
        extract_checked(archive, destination, "mike", json.loads(output))
        before = {p.name: sha256(p) for p in destination.iterdir()}
        for args in (("--help",), *((name, "--help") for name in
                                    ("install", "backup", "restore", "upgrade-check", "uninstall", "verify"))):
            result = self.run_helper(destination / "mike-maintenance", *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("usage: mike-maintenance", result.stdout)
            if args[0] in ("backup", "restore", "upgrade-check"):
                self.assertIn("--tool-directory", result.stdout)
        invalid = self.run_helper(destination / "mike-maintenance", "verify", "--backup", self.base / "missing")
        self.assertEqual(invalid.returncode, 1, invalid.stderr)
        self.assertIn("private values omitted", invalid.stderr)
        installed = self.run_helper(destination / "mike-maintenance", "install",
                                    "--candidate", self.candidate, "--prefix", self.prefix)
        self.assertEqual(installed.returncode, 0, installed.stderr)
        self.assertEqual(json.loads(installed.stdout)["status"], "installed")
        repeated = self.run_helper(destination / "mike-maintenance", "install",
                                   "--candidate", self.candidate, "--prefix", self.prefix)
        self.assertEqual(repeated.returncode, 0, repeated.stderr)
        self.assertEqual(json.loads(repeated.stdout)["status"], "unchanged")
        for name in ("mike", "mike-mcp"):
            result = subprocess.run([self.prefix / "bin" / name, "--version"], cwd=self.cwd,
                                    env={"PATH": "", "HOME": str(self.home)}, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), name + " 0.1.0")
        foreign = self.base / "foreign-prefix"
        (foreign / "bin").mkdir(parents=True)
        (foreign / "bin/mike").write_text("keep this executable")
        refused = self.run_helper(destination / "mike-maintenance", "install",
                                  "--candidate", self.candidate, "--prefix", foreign)
        self.assertNotEqual(refused.returncode, 0)
        self.assertEqual((foreign / "bin/mike").read_text(), "keep this executable")
        self.assertFalse((foreign / "lib").exists())
        generation = self.fixture.current().resolve()
        self.archive.write_bytes(b"corrupt fixture candidate")
        refused = self.run_helper(destination / "mike-maintenance", "install",
                                  "--candidate", self.candidate, "--prefix", self.prefix)
        self.assertNotEqual(refused.returncode, 0)
        self.assertEqual(self.fixture.current().resolve(), generation)
        self.assertFalse(producer.exists())
        self.assertFalse(self.poison.exists())
        self.assertEqual(before, {p.name: sha256(p) for p in destination.iterdir()})
        self.assertEqual(list(self.home.iterdir()), [])

    def test_extracted_and_installed_helpers_verify_private_synthetic_backup(self):
        recovery = test_recovery.RecoveryTests()
        recovery.setUp()
        self.addCleanup(recovery.doCleanups)
        recovery.backup()  # FakeDatabase only: no PostgreSQL or real user content.
        extracted = self.extract()
        install(self.candidate, self.prefix)
        identity = sha256(self.candidate / "manifest.json")
        for root in (extracted, self.fixture.current()):
            result = self.run_helper(root / "mike-maintenance", "verify", "--backup", recovery.bundle)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "verified-checksums")
            self.assertFalse(json.loads(result.stdout)["qualified"])
            self.assertNotIn("FAKE_SECRET", result.stdout + result.stderr)
        validate_generation(self.fixture.current().resolve(), identity)
        self.assertEqual({p.name for p in (self.prefix / "bin").iterdir()}, {"mike", "mike-mcp"})
        self.assertFalse(self.poison.exists())

    def test_source_deleted_helper_uses_explicit_postgres_and_docker_tools(self):
        recovery = test_recovery.RecoveryTests()
        recovery.setUp()
        self.addCleanup(recovery.doCleanups)
        recovery.backup()  # Synthetic dump and ledger; never a database connection.
        producer, shipped = self.base / "tool-producer", self.base / "tool-shipped"
        producer.mkdir(); shipped.mkdir()
        for name in (*MODULES, "maintenance_bundle"):
            shutil.copyfile(test_install.ROOT / "scripts/release" / (name + ".py"), producer / (name + ".py"))
        code = '''import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from maintenance_bundle import payload
for name, contents, mode in payload():
    path = Path(sys.argv[2]) / name
    path.write_bytes(contents)
    path.chmod(mode)
'''
        subprocess.run([sys.executable, "-I", "-B", "-c", code, producer, shipped], check=True,
                       cwd=self.cwd, env=self.env, capture_output=True, timeout=30)
        shutil.rmtree(producer)
        record = self.base / "selected-tool-commands"
        tools = fixture_tools(self.base / "selected tools", record)
        service = self.base / "private-service"
        service.write_text("[fixture]\nhost=localhost\ndbname=db\nuser=mike\npassword=FAKE_SECRET\n")
        service.chmod(0o600)
        self.env.update(PGPASSWORD="FAKE_SECRET", PGHOST="foreign", DATABASE_URL="postgres://FAKE_SECRET",
                        DOCKER_CONTEXT="synthetic", DOCKER_HOST="tcp://ignored:2375")
        helper = shipped / "mike-maintenance"
        for method, connection in (("postgres", ["--pg-service-file", service, "--pg-service", "fixture"]),
                                   ("docker", ["--docker-identity", "a" * 32])):
            result = self.run_helper(helper, "restore", "--prefix", recovery.prefix, "--backup", recovery.bundle,
                                     "--destination", recovery.private / ("restored-" + method),
                                     "--confirm-stopped-writers", "--tool-directory", tools, *connection)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "restored-offline-review-required")
            self.assertNotIn("FAKE_SECRET", result.stdout + result.stderr)
            self.assertEqual((recovery.private / ("restored-" + method) / "data/private-person/body").read_bytes(),
                             b"fake raw body\x00")
        records = [json.loads(line) for line in record.read_text().splitlines()]
        self.assertEqual({r["name"] for r in records}, {"psql", "pg_restore", "docker"})
        for command in records:
            self.assertNotIn("FAKE_SECRET", json.dumps(command))
            self.assertEqual(command["env"]["PATH"], "/usr/bin:/bin")
        refused = self.run_helper(helper, "restore", "--prefix", recovery.prefix, "--backup", recovery.bundle,
                                  "--destination", recovery.private / "unsafe-restore", "--confirm-stopped-writers",
                                  "--tool-directory", self.base / "SENSITIVE-MISSING-DIRECTORY",
                                  "--pg-service-file", service, "--pg-service", "fixture")
        self.assertNotEqual(refused.returncode, 0)
        self.assertNotIn("SENSITIVE-MISSING-DIRECTORY", refused.stdout + refused.stderr)
        self.assertFalse((recovery.private / "unsafe-restore").exists())
        self.assertFalse(producer.exists())
        self.assertFalse(self.poison.exists())

    def test_zipapp_reenters_candidate_verification_without_module_file_paths(self):
        extracted = self.extract()
        code = '''import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from inputs import verified_inputs, isolated_smoke
with verified_inputs(Path(sys.argv[2])) as (_, manifest):
    assert manifest["version"] == "0.1.0"
result = isolated_smoke(Path(sys.argv[3]), "0.1.0", Path.cwd(), Path(sys.argv[4]))
assert result["version_output"] == "mike 0.1.0"
'''
        result = subprocess.run([sys.executable, "-I", "-B", "-c", code,
                                 extracted / "mike-maintenance.pyz", self.candidate,
                                 extracted / "mike", self.home], cwd=self.cwd, env=self.env,
                                text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.poison.exists())

    def test_partial_duplicated_or_traversing_support_inventory_is_rejected(self):
        inventory = self.artifact["contents"]
        variants = [inventory[:-1], [*inventory, inventory[-1]],
                    [dict(i, path="../escaped") if i["path"] == "mike-maintenance.pyz" else i for i in inventory],
                    [dict(i, size=4 * 1024**2 + 1) if i["path"] == "mike-maintenance.pyz" else i for i in inventory],
                    [dict(i, mode="0o777") if i["path"] == "mike-maintenance" else i for i in inventory]]
        for invalid in variants:
            with self.subTest(invalid=invalid[-1]):
                with self.assertRaises(ReleaseError):
                    checked_inventory("mike", invalid)

    def test_installed_launcher_pins_physical_generation_before_starting_python(self):
        install(self.candidate, self.prefix)
        tools = self.base / "interpreter-probe"; tools.mkdir(mode=0o700)
        record = self.base / "python-arguments"
        probe = tools / "python3"
        probe.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$ARGUMENT_RECORD"\n')
        probe.chmod(0o755)
        environment = dict(self.env, PATH=str(tools) + ":/usr/bin:/bin", ARGUMENT_RECORD=str(record))
        result = subprocess.run([self.fixture.current() / "mike-maintenance", "--help"],
                                cwd=self.cwd, env=environment, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record.read_text().splitlines(),
                         ["-I", "-B", str(self.fixture.current().resolve() / "mike-maintenance.pyz"), "--help"])

    def test_support_hash_mismatch_refused_even_when_outer_checksum_is_updated(self):
        def tamper(item, data):
            if item.name == "mike-maintenance.pyz":
                data = b"x" * len(data)
            return item, data
        self.rewrite_archive(tamper)
        result = self.fixture.cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("archive member hash mismatch", result.stderr)
        self.assertFalse(self.prefix.exists())

    def test_unsafe_support_headers_refused_before_any_extraction(self):
        original = self.archive.read_bytes()
        for kind in ("symlink", "hardlink", "owner", "mode", "traversal"):
            with self.subTest(kind=kind):
                self.archive.write_bytes(original)
                def tamper(item, data):
                    if item.name == "mike-maintenance.pyz":
                        if kind in ("symlink", "hardlink"):
                            item.type = tarfile.SYMTYPE if kind == "symlink" else tarfile.LNKTYPE
                            item.linkname = "../../outside"
                        elif kind == "owner": item.uid = 99999
                        elif kind == "mode": item.mode = 0o666
                        else: item.name = "../escaped"
                    return item, data
                self.rewrite_archive(tamper)
                destination = self.base / kind
                destination.mkdir(mode=0o700)
                with self.assertRaises(ReleaseError):
                    extract_checked(self.archive, destination, "mike", self.artifact["contents"])
                self.assertEqual(list(destination.iterdir()), [])
        self.assertFalse((self.base / "escaped").exists())

    def test_extraction_preserves_foreign_existing_files_and_symlink_destinations(self):
        foreign = self.base / "foreign"
        foreign.mkdir(mode=0o700)
        (foreign / "mike-maintenance.pyz").write_bytes(b"keep")
        link = self.base / "linked"; link.symlink_to(foreign, target_is_directory=True)
        for destination in (foreign, link):
            with self.assertRaises(ReleaseError):
                extract_checked(self.archive, destination, "mike", self.artifact["contents"])
        self.assertEqual((foreign / "mike-maintenance.pyz").read_bytes(), b"keep")
        self.assertEqual(len(list(foreign.iterdir())), 1)

    def test_installed_support_mutation_or_hardlink_blocks_upgrade(self):
        install(self.candidate, self.prefix)
        current = self.fixture.current().resolve()
        support = current / "mike-maintenance.pyz"
        old_bytes = support.read_bytes()
        support.write_bytes(b"changed")
        newer = self.fixture.make_candidate("0.2.0")
        with self.assertRaises(ReleaseError): install(newer, self.prefix)
        self.assertEqual(self.fixture.current().resolve(), current)
        support.write_bytes(old_bytes)
        os.link(support, self.base / "foreign-hardlink")
        with self.assertRaises(ReleaseError): install(newer, self.prefix)
        self.assertEqual(self.fixture.current().resolve(), current)

    def test_installed_support_foreign_ownership_is_rejected(self):
        install(self.candidate, self.prefix)
        current = self.fixture.current().resolve()
        identity = sha256(current / "manifest.json")
        support = current / "mike-maintenance.pyz"
        original = Path.stat
        def foreign_stat(path, *args, **kwargs):
            value = original(path, *args, **kwargs)
            if path == support:
                fields = list(value); fields[4] = os.getuid() + 1
                return os.stat_result(fields)
            return value
        with patch.object(Path, "stat", foreign_stat):
            with self.assertRaisesRegex(ReleaseError, "installation file must be owned"):
                validate_generation(current, identity)

    def test_verified_legacy_generation_can_upgrade_to_complete_maintenance_kit(self):
        # Old three-member candidates remain valid predecessors; new pack()
        # output always includes the complete kit and cannot omit half of it.
        with tarfile.open(self.archive, "r:gz") as tar:
            members = [(item, tar.extractfile(item).read()) for item in tar.getmembers()
                       if item.name not in SUPPORT_FILES]
        with tarfile.open(self.archive, "w:gz", format=tarfile.USTAR_FORMAT) as tar:
            for item, data in members:
                tar.addfile(item, io.BytesIO(data))
        digest = sha256(self.archive)
        (self.candidate / (self.archive.name + ".sha256")).write_text(f"{digest}  {self.archive.name}\n")
        def legacy(manifest):
            artifact = manifest["artifacts"][0]
            artifact["sha256"] = digest
            artifact["contents"] = [i for i in artifact["contents"] if i["path"] not in SUPPORT_FILES]
        self.fixture.rewrite_manifest(legacy)
        install(self.candidate, self.prefix)
        predecessor = self.fixture.current().resolve()
        self.assertFalse((predecessor / "mike-maintenance").exists())
        install(self.fixture.make_candidate("0.2.0"), self.prefix)
        self.assertNotEqual(self.fixture.current().resolve(), predecessor)
        self.assertTrue(predecessor.exists())
        result = self.run_helper(self.fixture.current() / "mike-maintenance", "--help")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_generation_support_symlink_or_shared_mode_is_refused(self):
        install(self.candidate, self.prefix)
        current = self.fixture.current().resolve()
        identity = sha256(current / "manifest.json")
        support = current / "mike-maintenance.pyz"
        support.chmod(0o666)
        with self.assertRaises(ReleaseError): validate_generation(current, identity)
        support.chmod(0o644)
        outside = self.base / "foreign-kit"
        support.rename(outside); support.symlink_to(outside)
        with self.assertRaises(ReleaseError): validate_generation(current, identity)
        self.assertTrue(outside.exists())

    def test_interrupted_support_extraction_cleanup_is_allowlisted(self):
        state = initialize(self.prefix)
        stage = state / "generations" / (".pending-" + "a" * 64)
        stage.mkdir(mode=0o700); write_owner(stage)
        temporary = stage / ".mike"; temporary.mkdir(mode=0o700)
        (temporary / "mike-maintenance.pyz").write_bytes(b"interrupted")
        (stage / "mike-maintenance").write_bytes(b"already relocated")
        clean_pending(stage)
        self.assertFalse(stage.exists())
        stage.mkdir(mode=0o700); write_owner(stage)
        foreign = stage / "mike-maintenance-extra.py"; foreign.write_text("keep")
        with self.assertRaises(ReleaseError): clean_pending(stage)
        self.assertEqual(foreign.read_text(), "keep")


if __name__ == "__main__":
    unittest.main(verbosity=2)
