#!/usr/bin/env python3
"""Exercise installation and rollback in a temporary home without launchd writes."""

import contextlib
import importlib.machinery
import importlib.util
import io
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).with_name("install-cargo-sweep")
LOADER = importlib.machinery.SourceFileLoader("install_cargo_sweep", str(SOURCE))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
INSTALL = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(INSTALL)


class Installation(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cargo-cleaner-install-")
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name) / "home with spaces"
        self.launcher = self.home / ".local/bin/cargo-target-sweep.sh"
        self.plist = self.home / "Library/LaunchAgents/com.dawiddutoit.cargo-sweep.plist"
        self.launcher.parent.mkdir(parents=True)
        self.plist.parent.mkdir(parents=True)
        self.launcher.write_text("#!/bin/sh\necho old-cleaner\n")
        self.original_plist = plistlib.dumps({
            "Label": INSTALL.LABEL,
            "StartInterval": 21600,
            "ProgramArguments": [str(self.launcher)],
            "EnvironmentVariables": {"CARGO_TARGET_DIR": "obsolete-target",
                                     "CARGO_SWEEP_MAXSIZE": "25GB"},
        })
        self.plist.write_bytes(self.original_plist)
        self.commands = []
        self.fail_next_bootstrap = False

    def command(self, args, **kwargs):
        self.commands.append(tuple(args))
        if args[:2] == ("plutil", "-lint"):
            plistlib.loads(Path(args[2]).read_bytes())
        if args[0:2] == ("launchctl", "bootstrap") and self.fail_next_bootstrap:
            self.fail_next_bootstrap = False
            raise subprocess.CalledProcessError(5, args, stderr="fixture failure")
        return subprocess.CompletedProcess(args, 0, stdout="state = not running\n", stderr="")

    def install(self, *args):
        with patch.object(INSTALL.Path, "home", return_value=self.home), \
                patch.object(INSTALL.platform, "system", return_value="Darwin"), \
                patch.object(INSTALL.subprocess, "run", side_effect=self.command), \
                patch("sys.argv", [str(SOURCE), *args]), \
                contextlib.redirect_stdout(io.StringIO()):
            INSTALL.main()

    def test_migrates_live_job_to_complete_branch_independent_runtime(self):
        self.install()
        config = plistlib.loads(self.plist.read_bytes())
        self.assertEqual(config["StartInterval"], 21600)
        env = config["EnvironmentVariables"]
        self.assertEqual(env["CARGO_SWEEP_MAXSIZE_GB"], "25")
        self.assertNotIn("CARGO_TARGET_DIR", env)
        self.assertNotIn("CARGO_SWEEP_MAXSIZE", env)
        self.assertEqual(env["CARGO_SWEEP_ROOTS"], f"{self.home}/projects:{self.home}/.cache")
        runtime = self.home / ".local/share/mike/cargo-sweep/current"
        self.assertTrue(runtime.is_symlink())
        for name in INSTALL.FILES:
            self.assertFalse((runtime / name).is_symlink())
            self.assertEqual((runtime / name).read_bytes(), (SOURCE.parent.parent / name).read_bytes())
        subprocess.run(["/bin/sh", "-n", str(self.launcher)], check=True)
        self.assertTrue(any(c[:2] == ("launchctl", "bootstrap") for c in self.commands))

    def test_bootstrap_failure_restores_original_job_and_launcher(self):
        self.fail_next_bootstrap = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.install()
        self.assertEqual(self.plist.read_bytes(), self.original_plist)
        self.assertEqual(self.launcher.read_text(), "#!/bin/sh\necho old-cleaner\n")
        self.assertFalse((self.home / ".local/share/mike/cargo-sweep/current").is_symlink())
        self.assertEqual(sum(c[:2] == ("launchctl", "bootstrap") for c in self.commands), 2)

    def test_preview_does_not_change_job_or_launch_service(self):
        self.install("--dry-run")
        self.assertEqual(self.plist.read_bytes(), self.original_plist)
        self.assertFalse((self.home / ".local/share").exists())
        self.assertEqual(self.commands, [])


if __name__ == "__main__":
    unittest.main()
