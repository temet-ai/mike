#!/usr/bin/env python3
"""Stubbed release contract tests. No real Rust/SPA build or publication."""

import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest

from archive import SUPPORT_FILES, extract_checked
from common import ReleaseError

ROOT = Path(__file__).resolve().parents[2]

STUB = r'''#!PYTHON
import datetime, json, os, pathlib, subprocess, sys, tomllib
base = pathlib.Path(__file__).resolve().parent.parent
control = json.loads((base / 'control.json').read_text())
name, args = pathlib.Path(sys.argv[0]).name, sys.argv[1:]
with (base / 'tools.jsonl').open('a') as log:
    log.write(json.dumps({'tool':name, 'args':args, 'env':dict(os.environ)}) + '\n')
target = control.get('target', 'x86_64-unknown-linux-gnu')
if name in ('gh', 'brew', 'rustup'):
    sys.exit('forbidden tool invoked')
if name == 'uname':
    print(('Darwin' if target.endswith('darwin') else 'Linux') if args == ['-s'] else
          ('arm64' if target.endswith('darwin') else 'x86_64'))
elif name == 'rustc':
    print('rustc 1.95.0\nrelease: ' + control.get('rust', '1.95.0') + '\nhost: ' +
          control.get('rust_host', target))
elif name == 'node': print('v25.9.0')
elif name == 'pnpm':
    if args == ['--version']: print(control.get('pnpm', '11.1.3'))
    elif args == ['install', '--offline', '--frozen-lockfile']:
        if control.get('install_fail'): sys.exit('fixture dependency cache miss')
    elif args == ['build']:
        if control.get('spa_missing'): sys.exit(0)
        out = pathlib.Path('build'); out.mkdir(exist_ok=True)
        (out / 'index.html').write_text('<script src="app.js"></script>')
        (out / 'app.js').write_text('console.log("fixture");')
        if control.get('private_spa'): (out / '.env').write_text('secret')
    else: sys.exit('unexpected pnpm arguments: ' + str(args))
elif name == 'cargo':
    if args == ['--version']: print('cargo 1.95.0 (fixture)')
    else:
        assert args[:-2] == ['build','--offline','--locked','--profile','dist','--target',target,'-p','mike','-p','mike-mcp']
        assert args[-2] == '--config'
        flags = tomllib.loads(args[-1])['target']['cfg(all())']['rustflags']
        assert flags and all(flag.startswith('--remap-path-prefix=') for flag in flags)
        assert os.environ.get('SQLX_OFFLINE') == 'true'
        assert not any(k.startswith(('MIKE_', 'VITE_')) for k in os.environ)
        assert 'DATABASE_URL' not in os.environ and 'MIKE_BUILD_CACHE' not in os.environ
        if control.get('cargo_fail'): sys.exit('fixture build failed')
        if control.get('dirty_build'):
            pathlib.Path('homebrew-formulae/mike.rb').write_text('unexpected build edit')
        out = pathlib.Path(os.environ['CARGO_TARGET_DIR']) / target / 'dist'
        out.mkdir(parents=True, exist_ok=True)
        source_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
        epoch = int(os.environ['SOURCE_DATE_EPOCH'])
        for binary in ('mike', 'mike-mcp'):
            if control.get('missing_binary') == binary: continue
            version = '9.9.9' if control.get('mixed_version') and binary == 'mike-mcp' else '0.1.0'
            identity = {'schema_version': 1, 'product': binary, 'version': version,
                        'source_sha': source_sha, 'source_dirty': False,
                        'built_at': datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}
            if control.get('identity_binary', 'mike') == binary:
                identity.update(control.get('identity_override', {}))
            text = '#!/bin/sh\ncase "$1" in\n'
            text += "  --build-info) printf '%s\\n' '" + json.dumps(identity, sort_keys=True) + "' ;;\n"
            text += '  --version) printf "%s\\n" "' + binary + ' ' + version + '" ;;\n'
            text += '  --help) printf "%s\\n" "fixture help" ;;\n'
            text += '  list-tools) printf "%s\\n" \'[{"name":"fixture"}]\' ;;\n'
            text += '  *) exit 77 ;;\nesac\n'
            if control.get('invalid_catalogue'):
                text = text.replace('[{"name":"fixture"}]', '{"tools":[]}')
            if control.get('smoke_fail') and binary == 'mike':
                text = text.replace('--help) printf', '--help) exit 65; printf')
            (out / binary).write_text(text); (out / binary).chmod(0o755)
elif name in ('readelf', 'otool'): print('fixture runtime library report; not real ABI evidence')
else: sys.exit('unknown stub')
'''


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mike-release-contract-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        self.repo.mkdir()
        self.tools = self.base / "tools"
        self.tools.mkdir()
        self.control = self.base / "control.json"
        self.control.write_text("{}")
        self.log = self.base / "tools.jsonl"
        self.env = dict(os.environ, PATH=str(self.tools) + os.pathsep + os.environ["PATH"],
                        DATABASE_URL="must-not-leak", MIKE_SECRET="must-not-leak",
                        VITE_SECRET="must-not-leak", MIKE_BUILD_CACHE="mbx",
                        RUSTFLAGS="--cfg must_not_leak",
                        CARGO_ENCODED_RUSTFLAGS="-A\x1fall",
                        CARGO_BUILD_RUSTFLAGS="-A all",
                        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-A all",
                        CARGO_TARGET_DIR=str(self.base / "unrelated-target"),
                        CARGO_BUILD_JOBS="999", CARGO_INCREMENTAL="1")
        for name in ("cargo", "rustc", "pnpm", "node", "uname", "readelf", "otool", "gh", "brew", "rustup"):
            path = self.tools / name
            path.write_text(STUB.replace("#!PYTHON", "#!" + sys.executable))
            path.chmod(0o755)
        (self.repo / "scripts").mkdir()
        shutil.copy2(ROOT / "scripts/release.sh", self.repo / "scripts/release.sh")
        shutil.copytree(ROOT / "scripts/release", self.repo / "scripts/release",
                        ignore=shutil.ignore_patterns("__pycache__"))
        self.write(".gitignore", "/dist-out/\nfrontend/build/\nfrontend/node_modules/\n.env\n**/__pycache__/\n")
        self.write("Cargo.toml", '[workspace.package]\nversion = "0.1.0"\n')
        self.write("Cargo.lock", '\n'.join('[[package]]\nname = "' + name + '"\nversion = "0.1.0"'
                                            for name in ("mike", "mike-mcp")))
        for name in ("mike", "mike-mcp"):
            self.write(f"crates/{name}/Cargo.toml", f'[package]\nname = "{name}"\nversion.workspace = true\n')
        self.write("crates/mike/src/store/persistence/migrations/1_synthetic_fixture.sql",
                   "-- Synthetic release fixture only\nSELECT 1;\n")
        self.write("rust-toolchain.toml", '[toolchain]\nchannel = "1.95.0"\n')
        self.write(".nvmrc", "25.9.0\n")
        self.write("frontend/package.json", '{"packageManager":"pnpm@11.1.3"}\n')
        self.write("frontend/pnpm-lock.yaml", "lockfileVersion: '9.0'\n")
        self.write("LICENSE", "MIT fixture license\n")
        self.write("THIRD-PARTY-NOTICES.md", "# Third-party notices\n")
        self.write("homebrew-formulae/mike.rb", "reviewed formula\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid",
                 "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
                 "commit", "-qm", "isolated release test fixture")
        self.before = self.tracked_hashes()

    def write(self, path, content):
        destination = self.repo / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content)

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.repo, check=True,
                              capture_output=True, text=True).stdout.strip()

    def tracked_hashes(self):
        return {name: hashlib.sha256((self.repo / name).read_bytes()).hexdigest()
                for name in self.git("ls-files").splitlines()}

    def configure(self, **kwargs):
        self.control.write_text(json.dumps(kwargs))

    def invoke(self, *args, success=True):
        result = subprocess.run(["bash", str(self.repo / "scripts/release.sh"), *map(str, args)],
                                cwd=self.base, env=self.env, capture_output=True, text=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def candidate(self):
        result = self.invoke()
        return Path(next(line.removeprefix("Candidate: ") for line in result.stdout.splitlines()
                         if line.startswith("Candidate: ")))

    def assert_unmodified(self):
        self.assertEqual(self.before, self.tracked_hashes())
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertFalse((self.repo / "dist-out/.prepare.lock").exists())
        self.assertFalse(any(c["tool"] in ("gh", "brew", "rustup") for c in self.calls()))

    def test_prepare_twice_preserves_source_and_exact_candidate(self):
        first = self.candidate()
        manifest_bytes = (first / "manifest.json").read_bytes()
        self.assertEqual(self.candidate(), first)
        self.assertEqual((first / "manifest.json").read_bytes(), manifest_bytes)
        manifest = json.loads(manifest_bytes)
        self.assertEqual(manifest["source_commit"], self.git("rev-parse", "HEAD"))
        self.assertEqual(manifest["build"]["rust_path_remapping"],
                         {"repository": "/mike", "cargo_home": "/cargo"})
        self.assertNotIn(str(self.base), json.dumps(manifest["build"]))
        self.assertFalse(manifest["qualified"])
        self.assertEqual(manifest["checks"]["embedded_spa_runtime"], "not_run")
        migrations = manifest["migration_inventory"]
        self.assertEqual(migrations["source_commit"], manifest["source_commit"])
        self.assertEqual(migrations["checksum_algorithm"], "sha384")
        sql = self.repo / migrations["source_path"] / "1_synthetic_fixture.sql"
        self.assertEqual(migrations["migrations"], [{"version": 1, "filename": sql.name,
                                                    "checksum": hashlib.sha384(sql.read_bytes()).hexdigest()}])
        for artifact in manifest["artifacts"]:
            checksum = (first / (artifact["archive"] + ".sha256")).read_text()
            self.assertEqual(checksum, artifact["sha256"] + "  " + artifact["archive"] + "\n")
            with tarfile.open(first / artifact["archive"]) as tar:
                expected = [artifact["binary"], "LICENSE", "NOTICE.json", "THIRD-PARTY-NOTICES.md"]
                if artifact["binary"] == "mike":
                    expected.extend(SUPPORT_FILES)
                self.assertEqual(tar.getnames(), expected)
        self.assert_unmodified()

    def test_verify_uses_existing_archive_without_build_tools(self):
        candidate = self.candidate()
        self.log.unlink()
        for name in ("cargo", "pnpm", "rustc", "node"):
            (self.tools / name).write_text("#!/bin/sh\nexit 91\n")
        self.invoke("--verify", candidate)
        self.assertEqual({c["tool"] for c in self.calls()}, {"uname"})
        self.assert_unmodified()

    def test_cargo_build_receives_private_root_maps_as_config_not_environment_flags(self):
        cargo_home = self.base / 'private cargo=with spaces "quoted"'
        cargo_home.mkdir()
        self.env["CARGO_HOME"] = str(cargo_home)
        self.candidate()
        builds = [call for call in self.calls() if call["tool"] == "cargo" and call["args"][0] == "build"]
        self.assertEqual(len(builds), 1)
        call = builds[0]
        self.assertEqual(call["args"][-2], "--config")
        config = tomllib.loads(call["args"][-1])
        self.assertEqual(set(config["target"]["cfg(all())"]["rustflags"]), {
            f"--remap-path-prefix={self.repo}=/mike",
            f"--remap-path-prefix={cargo_home}=/cargo",
        })
        self.assertNotIn("RUSTFLAGS", call["env"])
        self.assertNotIn("CARGO_ENCODED_RUSTFLAGS", call["env"])
        self.assert_unmodified()

    def test_root_env_is_not_sourced_and_inherited_build_overrides_are_removed(self):
        poison = self.base / "env-was-sourced"
        self.write(".env", f'touch "{poison}"\nexit 93\n')
        self.candidate()
        self.assertFalse(poison.exists())
        for call in self.calls():
            if call["tool"] in ("cargo", "pnpm", "rustc", "node"):
                self.assertNotIn("DATABASE_URL", call["env"])
                self.assertNotIn("MIKE_SECRET", call["env"])
                self.assertNotIn("VITE_SECRET", call["env"])
                self.assertNotIn("MIKE_BUILD_CACHE", call["env"])
                for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
                            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS"):
                    self.assertNotIn(key, call["env"])
                self.assertEqual(call["env"]["CARGO_BUILD_JOBS"], "1")
                self.assertEqual(call["env"]["CARGO_INCREMENTAL"], "0")
                self.assertEqual(call["env"]["RUSTUP_AUTO_INSTALL"], "0")
                self.assertEqual(call["env"]["COREPACK_ENABLE_NETWORK"], "0")
                self.assertEqual(call["env"]["npm_config_manage_package_manager_versions"], "false")
        self.assertFalse((self.base / "unrelated-target").exists())
        self.assert_unmodified()

    def test_publish_fails_before_any_tool_or_output(self):
        result = self.invoke("--publish", success=False)
        self.assertIn("publication is unavailable", result.stderr)
        self.assertEqual(self.calls(), [])
        self.assertFalse((self.repo / "dist-out").exists())
        self.assert_unmodified()

    def test_version_change_fails_without_mutation(self):
        result = self.invoke("0.2.0", success=False)
        self.assertIn("reviewed workspace version", result.stderr)
        self.assertEqual(self.calls(), [])
        self.assert_unmodified()

    def test_dirty_source_fails_before_build(self):
        self.write("homebrew-formulae/mike.rb", "user edit\n")
        result = self.invoke(success=False)
        self.assertIn("source tree is not clean", result.stderr)
        self.assertEqual(self.calls(), [])
        self.assertEqual((self.repo / "homebrew-formulae/mike.rb").read_text(), "user edit\n")

    def test_cross_target_fails_before_build(self):
        result = self.invoke("--target", "aarch64-apple-darwin", success=False)
        self.assertIn("not native", result.stderr)
        self.assert_unmodified()

    def test_mixed_binary_versions_fail_and_remove_partial_candidate(self):
        self.configure(mixed_version=True)
        result = self.invoke(success=False)
        self.assertIn("version disagrees", result.stderr)
        self.assertFalse(list((self.repo / "dist-out").rglob("manifest.json")))
        self.assert_unmodified()

    def test_exact_source_identity_is_recorded_for_both_archived_executables(self):
        candidate = self.candidate()
        manifest = json.loads((candidate / "manifest.json").read_text())
        self.assertEqual(manifest["checks"]["executable_source_identity"], "passed")
        for artifact in manifest["artifacts"]:
            self.assertEqual(artifact["build_info"]["source_sha"], manifest["source_commit"])
            self.assertIs(artifact["build_info"]["source_dirty"], False)
            self.assertEqual(artifact["build_info"]["product"], artifact["binary"])
        self.assert_unmodified()

    def test_matching_semver_cannot_hide_wrong_binary_identity(self):
        for binary in ("mike", "mike-mcp"):
            for field, value in (("source_sha", "b" * 40), ("source_sha", "unknown"),
                                 ("source_sha", "mbx-build"), ("source_dirty", True),
                                 ("source_dirty", 0), ("schema_version", True),
                                 ("built_at", "1970-01-01T00:00:00Z")):
                with self.subTest(binary=binary, field=field, value=value):
                    self.configure(identity_binary=binary, identity_override={field: value})
                    result = self.invoke(success=False)
                    self.assertIn("build identity disagrees", result.stderr)
                    self.assertFalse(list((self.repo / "dist-out").rglob("manifest.json")))
        self.assert_unmodified()

    def test_verification_checks_source_against_binary_not_only_manifest_evidence(self):
        candidate = self.candidate()
        path = candidate / "manifest.json"
        original = json.loads(path.read_text())
        for field, value in (("source_commit", "b" * 40), ("source_date_epoch", 0)):
            with self.subTest(field=field):
                manifest = json.loads(json.dumps(original))
                manifest[field] = value
                for artifact in manifest["artifacts"]:
                    artifact["build_info"]["source_sha" if field == "source_commit" else "built_at"] = (
                        value if field == "source_commit" else "1970-01-01T00:00:00Z")
                path.write_text(json.dumps(manifest))
                result = self.invoke("--verify", candidate, success=False)
                self.assertIn("build identity disagrees", result.stderr)

    def test_verification_rejects_missing_identity_evidence_and_invalid_epoch(self):
        candidate = self.candidate()
        path = candidate / "manifest.json"
        original = json.loads(path.read_text())
        manifest = json.loads(json.dumps(original))
        del manifest["artifacts"][0]["build_info"]
        path.write_text(json.dumps(manifest))
        self.assertIn("build identity evidence changed", self.invoke("--verify", candidate, success=False).stderr)
        for epoch in (None, True, -1, 4294967296, "0"):
            with self.subTest(epoch=epoch):
                path.write_text(json.dumps(dict(original, source_date_epoch=epoch)))
                self.assertIn("source_date_epoch", self.invoke("--verify", candidate, success=False).stderr)

    def test_invalid_catalogue_is_not_a_passing_smoke(self):
        self.configure(invalid_catalogue=True)
        result = self.invoke(success=False)
        self.assertIn("invalid tool catalogue", result.stderr)
        self.assert_unmodified()

    def test_verification_requires_complete_valid_source_bound_migration_inventory(self):
        candidate = self.candidate()
        path = candidate / "manifest.json"
        original = json.loads(path.read_text())
        changes = [lambda manifest: manifest.pop("migration_inventory"),
                   lambda manifest: manifest["migration_inventory"].update(source_commit="b" * 40),
                   lambda manifest: manifest["migration_inventory"].update(migrations=[]),
                   lambda manifest: manifest["migration_inventory"]["migrations"][0].update(version=True),
                   lambda manifest: manifest["migration_inventory"]["migrations"][0].update(checksum="short")]
        for change in changes:
            manifest = json.loads(json.dumps(original))
            change(manifest)
            path.write_text(json.dumps(manifest))
            with self.subTest(manifest=manifest):
                self.assertIn("migration inventory", self.invoke("--verify", candidate, success=False).stderr)

    def test_failed_archived_cli_smoke_blocks_candidate(self):
        self.configure(smoke_fail=True)
        result = self.invoke(success=False)
        self.assertIn("failed (65)", result.stderr)
        self.assertFalse(list((self.repo / "dist-out").rglob("manifest.json")))
        self.assert_unmodified()

    def test_missing_binary_cannot_reuse_previous_output(self):
        self.candidate()
        self.configure(missing_binary="mike-mcp")
        result = self.invoke(success=False)
        self.assertIn("expected a regular", result.stderr)
        self.assertEqual(len(list((self.repo / "dist-out").rglob("manifest.json"))), 1)
        self.assert_unmodified()

    def test_missing_spa_cannot_reuse_stale_build(self):
        self.write("frontend/build/index.html", "stale")
        self.configure(spa_missing=True)
        result = self.invoke(success=False)
        self.assertIn("expected a regular", result.stderr)
        self.assertFalse(any(c["tool"] == "cargo" and c["args"][0] == "build" for c in self.calls()))
        self.assert_unmodified()

    def test_dependency_cache_miss_never_installs_tools(self):
        self.configure(install_fail=True)
        result = self.invoke(success=False)
        self.assertIn("cache miss", result.stderr)
        self.assert_unmodified()

    def test_cargo_failure_preserves_existing_candidate(self):
        candidate = self.candidate()
        original = (candidate / "manifest.json").read_bytes()
        self.configure(cargo_fail=True)
        result = self.invoke(success=False)
        self.assertIn("fixture build failed", result.stderr)
        self.assertEqual((candidate / "manifest.json").read_bytes(), original)
        self.assert_unmodified()

    def test_toolchain_mismatch_never_builds(self):
        for config in ({"rust": "1.94.0"}, {"pnpm": "10.0.0"}, {"rust_host": "aarch64-apple-darwin"}):
            with self.subTest(config=config):
                self.configure(**config)
                result = self.invoke(success=False)
                self.assertIn("need preinstalled", result.stderr)
        self.assert_unmodified()

    def test_frontend_env_is_refused(self):
        # Ignore it in the fixture so this reaches the dedicated build-input guard.
        self.git("config", "core.excludesFile", str(self.base / "global-ignore"))
        (self.base / "global-ignore").write_text("frontend/.env.production\n")
        self.write("frontend/.env.production", "VITE_SECRET=private\n")
        result = self.invoke(success=False)
        self.assertIn("could inject private", result.stderr)
        self.assert_unmodified()

    def test_private_spa_file_is_refused(self):
        self.configure(private_spa=True)
        result = self.invoke(success=False)
        self.assertIn("private-file candidate", result.stderr)
        self.assert_unmodified()

    def test_build_source_changes_are_reported_without_resetting_them(self):
        self.configure(dirty_build=True)
        result = self.invoke(success=False)
        self.assertIn("source tree is not clean", result.stderr)
        self.assertEqual((self.repo / "homebrew-formulae/mike.rb").read_text(), "unexpected build edit")
        self.assertFalse(list((self.repo / "dist-out").rglob("manifest.json")))

    def test_existing_prepare_lock_blocks_build(self):
        (self.repo / "dist-out/.prepare.lock").mkdir(parents=True)
        result = self.invoke(success=False)
        self.assertIn("another preparation may be running", result.stderr)
        self.assertFalse(any(c["tool"] == "pnpm" and c["args"][0] == "install" for c in self.calls()))

    def test_symlink_staging_is_refused(self):
        outside = self.base / "outside"
        outside.mkdir()
        (self.repo / "dist-out").symlink_to(outside, target_is_directory=True)
        result = self.invoke(success=False)
        self.assertRegex(result.stderr, "source tree is not clean|non-symlink directory")
        self.assertEqual(list(outside.iterdir()), [])

    def test_changed_archive_and_absolute_checksum_are_refused(self):
        candidate = self.candidate()
        checksum = next(candidate.glob("*.sha256"))
        checksum.write_text(checksum.read_text().replace("  ", "  /tmp/"))
        self.assertIn("archive/checksum mismatch", self.invoke("--verify", candidate, success=False).stderr)
        archive = next(candidate.glob("*.tar.gz"))
        archive.write_bytes(archive.read_bytes() + b"tampered")
        self.assertIn("archive/checksum mismatch", self.invoke("--verify", candidate, success=False).stderr)

    def test_mac_preparation_contract_uses_native_target_and_otool(self):
        self.configure(target="aarch64-apple-darwin")
        candidate = self.candidate()
        manifest = json.loads((candidate / "manifest.json").read_text())
        self.assertEqual(manifest["target"], "aarch64-apple-darwin")
        self.assertEqual(manifest["artifacts"][0]["runtime_requirements"]["method"], "otool")
        self.assert_unmodified()

    def test_archive_traversal_and_symlinks_are_refused(self):
        destination = self.base / "extract"
        destination.mkdir()
        inventory = [{"path": name, "size": 1, "sha256": "unused"}
                     for name in ("mike", "LICENSE", "NOTICE.json")]
        for unsafe in ("../mike", "mike"):
            with self.subTest(unsafe=unsafe):
                archive = self.base / "unsafe.tar.gz"
                with tarfile.open(archive, "w:gz") as tar:
                    for name in (unsafe, "LICENSE", "NOTICE.json"):
                        member = tarfile.TarInfo(name)
                        member.mode = 0o755 if name == unsafe else 0o644
                        member.size = 1
                        if unsafe == "mike" and name == unsafe:
                            member.type = tarfile.SYMTYPE
                            member.linkname = "/tmp/elsewhere"
                        tar.addfile(member, io.BytesIO(b"x"))
                with self.assertRaises(ReleaseError):
                    extract_checked(archive, destination, "mike", inventory)
        self.assertEqual(list(destination.iterdir()), [])


if __name__ == "__main__":
    print("Contract fixtures only: no real Mike artifacts, ABI, embedded UI, or clean-account qualification.", flush=True)
    unittest.main(verbosity=2)
