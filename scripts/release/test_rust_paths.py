"""Tiny offline compiler probes; never build Mike or reuse its output directories."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from common import ReleaseError, build_environment
from rust_paths import cargo_path_config, remap_flags

ROOT = Path(__file__).resolve().parents[2]


class RustPathTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-private-build-root-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / 'private source=with spaces "quoted"'
        self.root.mkdir()
        self.cargo_home = self.base / "private cargo=with spaces"
        self.cargo_home.mkdir()
        self.env = dict(build_environment(), CARGO_HOME=str(self.cargo_home))

    def test_toml_keeps_each_mapping_as_one_flag(self):
        config = tomllib.loads(cargo_path_config(self.root, self.env))
        self.assertEqual(config, {"target": {"cfg(all())": {"rustflags":
                         remap_flags(self.root, self.env)}}})
        self.assertEqual(set(config["target"]["cfg(all())"]["rustflags"]), {
            f"--remap-path-prefix={self.root}=/mike",
            f"--remap-path-prefix={self.cargo_home}=/cargo",
        })

    def test_default_and_relative_cargo_home_follow_build_cwd(self):
        for value in (None, "", ".private cargo"):
            with self.subTest(cargo_home=value):
                env = {"HOME": str(self.base / "private home")}
                if value is not None:
                    env["CARGO_HOME"] = value
                expected = self.root / value if value else Path(env["HOME"]) / ".cargo"
                self.assertIn(f"--remap-path-prefix={expected}=/cargo", remap_flags(self.root, env))

    def test_aliases_and_nested_prefixes_remain_covered(self):
        alias = self.root / "cargo cache link"
        alias.symlink_to(self.cargo_home, target_is_directory=True)
        flags = remap_flags(self.root, dict(self.env, CARGO_HOME=str(alias)))
        self.assertIn(f"--remap-path-prefix={alias}=/cargo", flags)
        self.assertIn(f"--remap-path-prefix={self.cargo_home}=/cargo", flags)
        self.assertLess(flags.index(f"--remap-path-prefix={self.root}=/mike"),
                        flags.index(f"--remap-path-prefix={alias}=/cargo"))

    def test_ambiguous_source_and_cache_roots_fail(self):
        with self.assertRaisesRegex(ReleaseError, "distinct source path prefixes"):
            remap_flags(self.root, dict(self.env, CARGO_HOME=str(self.root)))

    def test_ambient_compiler_overrides_are_not_reintroduced(self):
        overrides = {"RUSTFLAGS": "-A all", "CARGO_ENCODED_RUSTFLAGS": "-A\x1fall",
                     "CARGO_BUILD_RUSTFLAGS": "-A all", "RUSTC_WRAPPER": "/private/wrapper",
                     "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS": "-A all"}
        with patch.dict(os.environ, overrides):
            env = build_environment()
            self.assertTrue(set(overrides).isdisjoint(env))
            self.assertNotIn("-A all", cargo_path_config(self.root, env))


class CompilerPathTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.env = build_environment()
        cls.rustc = shutil.which("rustc", path=cls.env.get("PATH"))
        cls.cargo = shutil.which("cargo", path=cls.env.get("PATH"))
        if cls.rustc is None or cls.cargo is None:
            raise RuntimeError("preinstalled rustc and cargo required for standalone path regressions")
        version = subprocess.check_output([cls.rustc, "-vV"], cwd=ROOT, env=cls.env,
                                          text=True, timeout=15)
        cls.target = next(line.removeprefix("host: ") for line in version.splitlines()
                          if line.startswith("host: "))

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-private-compiler-root-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()

    def command(self, args, root, env, success=True):
        result = subprocess.run(args, cwd=root, env=env, text=True,
                                capture_output=True, timeout=60)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def layout(self, name):
        root = self.base / name / 'source=with spaces "quoted"'
        cargo_home = root.parent / "cargo=with spaces"
        root.mkdir(parents=True)
        cargo_home.mkdir()
        # Preserve the exact workspace toolchain even outside the checkout.
        shutil.copyfile(ROOT / "rust-toolchain.toml", root / "rust-toolchain.toml")
        return root, cargo_home, dict(self.env, CARGO_HOME=str(cargo_home))

    def test_optimized_file_and_panic_locations_hide_both_private_roots(self):
        observations = []
        for build_name in ("first private host", "second private host"):
            root, cargo_home, env = self.layout(build_name)
            dependency = cargo_home / "registry/src/fixture-1.0.0/location.rs"
            dependency.parent.mkdir(parents=True)
            dependency.write_text('pub fn source() -> &\'static str { file!() }\n'
                                  'pub fn fail() { panic!("synthetic dependency panic"); }\n')
            source = root / "location.rs"
            source.write_text(f'#[path = {json.dumps(str(dependency))}] mod dependency;\n'
                              'fn main() {\n'
                              '    println!("{}|{}", file!(), dependency::source());\n'
                              '    if std::env::args().nth(1).as_deref() == Some("dependency") {\n'
                              '        dependency::fail();\n'
                              '    }\n'
                              '    panic!("synthetic repository panic");\n'
                              '}\n')
            binary = root / "probe"
            command = [self.rustc, "--edition=2024", "-O", "-C", "strip=symbols",
                       str(source), "-o", str(binary)]
            if build_name == "first private host":
                self.command(command, root, env)
                self.assertIn(str(root).encode(), binary.read_bytes())
                self.assertIn(str(cargo_home).encode(), binary.read_bytes())
            self.command([*command, *remap_flags(root, env)], root, env)
            self.assertNotIn(str(self.base).encode(), binary.read_bytes())
            results = [self.command([str(binary), mode], root, env, success=False)
                       for mode in ("repository", "dependency")]
            self.assertIn("/mike/location.rs", results[0].stderr)
            self.assertIn("/cargo/registry/src/fixture-1.0.0/location.rs", results[1].stderr)
            for result in results:
                self.assertNotIn(str(self.base), result.stdout + result.stderr)
                self.assertEqual(result.stdout.strip(),
                                 "/mike/location.rs|/cargo/registry/src/fixture-1.0.0/location.rs")
            observations.append([(result.stdout, result.stderr) for result in results])
        # Panic headers contain process IDs, so compare the location/message tails.
        for left, right in zip(*observations):
            self.assertEqual(left[0], right[0])
            self.assertEqual(left[1].split("panicked at ", 1)[1],
                             right[1].split("panicked at ", 1)[1])

    def test_cargo_remapping_preserves_committed_deny_lints(self):
        root, _, env = self.layout("cargo lint inheritance")
        (root / "src").mkdir()
        (root / ".cargo").mkdir()
        shutil.copyfile(ROOT / ".cargo/config.toml", root / ".cargo/config.toml")
        (root / "Cargo.toml").write_text('[package]\nname = "release-path-probe"\n'
                                         'version = "0.0.0"\nedition = "2024"\n')
        (root / "src/main.rs").write_text(
            'fn main() {\n'
            '    let value: Option<u8> = std::env::args().nth(1).and_then(|v| v.parse().ok());\n'
            '    println!("{}", value.unwrap());\n'
            '}\n')
        self.command([self.cargo, "generate-lockfile", "--offline"], root, env)
        result = self.command([self.cargo, "clippy", "--offline", "--locked", "-v",
                               "--target", self.target, "--config", cargo_path_config(root, env)],
                              root, env, success=False)
        self.assertIn("clippy::unwrap_used", result.stderr)
        self.assertIn("error: used `unwrap()`", result.stderr)
        committed = tomllib.loads((ROOT / ".cargo/config.toml").read_text())
        flags = committed["target"]["cfg(all())"]["rustflags"]
        for flag in flags:
            self.assertIn(flag, result.stderr)
        self.assertIn("--remap-path-prefix=", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
