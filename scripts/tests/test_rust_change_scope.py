"""Exercise gate scope against real Cargo metadata without compiling Rust."""

import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


REPO = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("rust_change_scope", REPO / "scripts/lib/rust_change_scope.py")
scope = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scope)


class RustChangeScopeTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name).resolve()
        self.names = ["mike", "mike-codex", "mike-mcp", "proxy-forward", "unrelated"]
        self.write("Cargo.toml", '[workspace]\nresolver = "2"\nmembers = ["crates/*"]\n')
        for name in self.names:
            deps = ""
            if name == "mike":
                deps = '[dependencies]\nadapter = { package = "mike-codex", path = "../mike-codex" }\nproxy-forward = { path = "../proxy-forward" }\n'
            elif name == "mike-mcp":
                deps = '[dev-dependencies]\nmike = { path = "../mike" }\n'
            self.write(f"crates/{name}/Cargo.toml", f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n{deps}')
            self.write(f"crates/{name}/src/lib.rs", "")
            self.write(f"crates/{name}/tests/contract.rs", "")
        self.packages = scope.metadata(self.root)
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Scope test")
        self.git("add", ".")
        self.git("commit", "-qm", "base")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")

    def write(self, relative, content):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True, text=True, capture_output=True).stdout.strip()

    def selected(self, *paths):
        return scope.affected_args(self.root, paths, self.packages)

    def commands(self, *paths):
        return scope.area_commands(self.root, paths, self.packages)

    def install_runner(self):
        for file in ["scripts/test-area", "scripts/pre-push-check", "scripts/lib/rust_change_scope.py"]:
            target = self.root / file
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / file, target)
        for name in ["cx", "lint-rust", "crate-plan", "schema-plan"]:
            target = self.write(f"scripts/{name}", '#!/usr/bin/env python3\nimport json, pathlib, sys\nwith (pathlib.Path(__file__).parent / "calls.jsonl").open("a") as out:\n    out.write(json.dumps([pathlib.Path(__file__).name, *sys.argv[1:]]) + "\\n")\n')
            target.chmod(0o755)
        self.git("add", ".")
        self.git("commit", "-qm", "runner")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")

    def push(self):
        self.git("add", ".")
        self.git("commit", "-qm", "change")
        head = self.git("rev-parse", "HEAD")
        base = self.git("rev-parse", "origin/main")
        for name in ["gitleaks", "semgrep"]:
            target = self.write(f"tools/{name}", "#!/bin/sh\nexit 0\n")
            target.chmod(0o755)
        env = dict(os.environ, PATH=f"{self.root / 'tools'}:{os.environ['PATH']}")
        env.pop("MIKE_SEAT", None)
        return subprocess.run(
            [str(self.root / "scripts/pre-push-check")], cwd=self.root,
            input=f"refs/heads/change {head} refs/heads/change {base}\n",
            env=env, check=True, capture_output=True, text=True,
        )

    def calls(self):
        return [json.loads(line) for line in (self.root / "scripts/calls.jsonl").read_text().splitlines()]

    def test_new_crate_selects_real_transitive_and_dev_consumers(self):
        self.assertEqual(self.selected("crates/mike-codex/src/client.rs"), ["-p", "mike", "-p", "mike-codex", "-p", "mike-mcp"])

    def test_existing_crates_keep_consumer_coverage(self):
        self.assertEqual(self.selected("crates/mike/src/app.rs"), ["-p", "mike", "-p", "mike-mcp"])
        self.assertEqual(self.selected("crates/mike-mcp/src/tools.rs"), ["-p", "mike-mcp"])
        self.assertEqual(self.selected("crates/proxy-forward/src/lib.rs"), ["-p", "mike", "-p", "mike-mcp", "-p", "proxy-forward"])

    def test_build_dependencies_and_renamed_dependencies_are_edges(self):
        manifest = self.root / "crates/unrelated/Cargo.toml"
        manifest.write_text(manifest.read_text() + '[build-dependencies]\nclient = { package = "mike-codex", path = "../mike-codex" }\n')
        self.packages = scope.metadata(self.root)
        self.assertIn("unrelated", self.selected("crates/mike-codex/src/client.rs"))

    def test_workspace_and_unknown_inputs_fall_back(self):
        for path in ["Cargo.lock", "Cargo.toml", ".cargo/config.toml", "rust-toolchain.toml", "crates/removed/src/lib.rs", "shared/outside.rs"]:
            with self.subTest(path=path):
                self.assertEqual(self.selected(path), ["--workspace"])
                self.assertIn("--workspace", self.commands(path)[0])

    def test_sqlx_selects_mike_and_consumers(self):
        self.assertEqual(self.selected(".sqlx/query-abc.json"), ["-p", "mike", "-p", "mike-mcp"])
        command = self.commands(".sqlx/query-abc.json")[0]
        self.assertEqual(command[3:5], ["-p", "mike"])
        self.assertIn("mike-mcp", command)
        self.assertNotIn("--ignore-default-filter", command)

    def test_crate_manifest_selects_that_crate_and_consumers(self):
        self.assertEqual(self.selected("crates/mike-codex/Cargo.toml"), ["-p", "mike", "-p", "mike-codex", "-p", "mike-mcp"])

    def test_library_change_includes_its_integration_targets_only(self):
        command = self.commands("crates/mike-codex/src/client.rs")[0]
        self.assertEqual(command[3:7], ["-p", "mike-codex", "--tests", "--ignore-default-filter"])
        self.assertNotIn("--all", command)
        self.assertNotIn("--workspace", command)
        self.assertEqual(command[-1], "(package(=mike-codex))")

    def test_mike_source_keeps_module_filter_and_all_tiers(self):
        command = self.commands("crates/mike/src/launch/codex/model.rs")[0]
        self.assertEqual(command[3:5], ["-p", "mike"])
        self.assertIn("--tests", command)
        self.assertIn("--ignore-default-filter", command)
        self.assertEqual(command[-1], "(package(=mike) & test(/^launch::codex(::|$)/))")

    def test_workspace_and_query_metadata_do_not_expand_changed_module_db_scope(self):
        source = "crates/mike/src/launch/codex/model.rs"
        self.assertEqual(self.commands(source, "Cargo.toml", "Cargo.lock", "crates/mike/Cargo.toml", ".sqlx/query.json"), self.commands(source))
        for path in ["Cargo.toml", "Cargo.lock", "crates/mike/Cargo.toml", ".sqlx/query.json"]:
            with self.subTest(path=path):
                command = self.commands(path)[0]
                self.assertNotIn("--ignore-default-filter", command)
                self.assertNotIn("-E", command)

    def test_nested_integration_change_selects_harness(self):
        command = self.commands("crates/mike/tests/contract/codex.rs")[0]
        self.assertEqual(command[3:7], ["-p", "mike", "--test", "contract"])
        self.assertNotIn("--tests", command)
        self.assertEqual(command[-1], "(binary(=contract))")

    def test_multiple_packages_get_distinct_target_commands(self):
        commands = self.commands("crates/mike/tests/contract/codex.rs", "crates/mike-codex/src/lib.rs")
        self.assertEqual(len(commands), 2)
        self.assertEqual([command[4] for command in commands], ["mike", "mike-codex"])
        self.assertNotIn("--tests", commands[0])
        self.assertIn("--tests", commands[1])

    def test_inspect_manifest_change_runs_mcp_manifest_coverage(self):
        commands = self.commands("crates/mike/src/api/manifest/workspace.rs")
        coverage = [command for command in commands if command[4] == "mike-mcp"]
        self.assertEqual(len(coverage), 1)
        self.assertIn(["--test", "manifest_coverage"], [coverage[0][5:7]])
        self.assertEqual(coverage[0][-1], "(binary(=manifest_coverage))")

    def test_docs_do_not_broaden_source_filter(self):
        self.assertEqual(self.commands("crates/mike/README.md"), [])
        self.assertEqual(self.commands("crates/mike/src/launch/codex/model.rs", "crates/mike/README.md"), self.commands("crates/mike/src/launch/codex/model.rs"))

    def test_deleted_and_renamed_files_keep_both_package_owners(self):
        self.git("mv", "crates/mike-codex/src/lib.rs", "crates/unrelated/src/renamed.rs")
        paths = scope.git_paths(self.root, "origin/main")
        self.assertIn("crates/mike-codex/src/lib.rs", paths)
        self.assertIn("crates/unrelated/src/renamed.rs", paths)
        selected = self.selected(*paths)
        self.assertIn("mike-codex", selected)
        self.assertIn("unrelated", selected)

    def test_runner_executes_explicit_package_and_targets(self):
        self.install_runner()
        self.write("crates/mike-codex/src/lib.rs", "pub fn changed() {}\n")
        subprocess.run([str(self.root / "scripts/test-area")], cwd=self.root, check=True, capture_output=True)
        calls = self.calls()
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][1:6], ["nextest", "run", "-p", "mike-codex", "--tests"])

    def test_runner_print_command_does_not_execute(self):
        self.install_runner()
        self.write("crates/mike-codex/src/lib.rs", "pub fn changed() {}\n")
        result = subprocess.run([str(self.root / "scripts/test-area"), "--print-command"], cwd=self.root, check=True, capture_output=True, text=True)
        self.assertIn("-p mike-codex --tests", result.stdout)
        self.assertFalse((self.root / "scripts/calls.jsonl").exists())

    def test_metadata_failure_is_conservative(self):
        with patch.object(sys := scope.sys, "argv", ["rust_change_scope.py", "packages", "--root", str(self.root)]), \
             patch.object(sys, "stdin", io.StringIO("crates/mike-codex/src/lib.rs\n")), \
             patch.object(scope, "metadata", side_effect=ValueError("invalid metadata")), \
             patch.object(sys, "stdout", io.StringIO()) as output, \
             patch.object(sys, "stderr", io.StringIO()):
            self.assertEqual(scope.main(), 0)
            self.assertEqual(output.getvalue().strip(), "--workspace")

    def test_pre_push_checks_consumers_then_only_changed_area(self):
        self.install_runner()
        self.write("crates/mike-codex/src/lib.rs", "pub fn changed() {}\n")
        self.push()
        calls = self.calls()
        packages = ["-p", "mike", "-p", "mike-codex", "-p", "mike-mcp"]
        self.assertIn(["lint-rust", *packages], calls)
        self.assertNotIn(["cx", "nextest", "run", *packages, "--no-fail-fast"], calls)
        focused = calls[-1]
        self.assertEqual(focused[1:6], ["nextest", "run", "-p", "mike-codex", "--tests"])
        self.assertNotIn("--all", focused)

    def test_sqlx_only_push_runs_rust_gates(self):
        self.install_runner()
        self.write(".sqlx/query-example.json", "{}")
        self.push()
        self.assertIn(["lint-rust", "-p", "mike", "-p", "mike-mcp"], self.calls())
        self.assertEqual(len([call for call in self.calls() if call[1:3] == ["nextest", "run"]]), 1)

    def test_script_only_push_runs_scope_regressions(self):
        self.install_runner()
        self.write("scripts/tests/test_rust_change_scope.py", 'import pathlib, unittest\nclass Check(unittest.TestCase):\n    def test_marker(self):\n        pathlib.Path("regression-ran").write_text("yes")\n')
        self.push()
        self.assertTrue((self.root / "regression-ran").exists())
        self.assertFalse((self.root / "scripts/calls.jsonl").exists())


if __name__ == "__main__":
    unittest.main()
