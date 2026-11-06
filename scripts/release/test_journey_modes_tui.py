"""Offline prompt/key contracts; no installed daemon or private socket executes."""
from contextlib import contextmanager
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

from journey_modes_tui import PROMPTS, TerminalGrid, TerminalSession, installed_terminal, tui_setup
from journey_process import JourneyError

ROOT = Path(__file__).resolve().parents[2]


class ModeTuiTests(unittest.TestCase):
    def test_prompt_markers_match_current_installed_source_and_database_order(self):
        source = (ROOT / "crates/mike/src/tui/screens/setup/steps/database.rs").read_text()
        for stage in ("storage", "url", "verified"):
            self.assertIn(PROMPTS[stage].decode(), source)
        self.assertLess(source.index("DbAction::Existing,\n"), source.index("DbAction::Memory,"))
        keys = (ROOT / "crates/mike/src/tui/screens/setup/panel.rs").read_text()
        self.assertIn("KeyCode::End", keys)

    def test_trial_and_existing_url_drive_reviewed_keys_and_stop_before_registration(self):
        for url in (None, "postgres://postgres@127.0.0.1:6432/owned?sslmode=disable"):
            terminal = Mock()
            terminal.finish.return_value = {"owned_pty": True}
            @contextmanager
            def session(*_):
                yield terminal
            with self.subTest(url=url), patch("journey_modes_tui.installed_terminal", session):
                result = tui_setup("/installed/mike", "/private/config", "/private/home", "/private/cwd", url)
            self.assertTrue(result["owned_pty"])
            prompts = [call.args[0] for call in terminal.expect.call_args_list]
            expected = ["storage"]
            self.assertEqual(prompts, expected + (["url"] if url else []) + ["verified"])
            keys = [call.args[0] for call in terminal.send.call_args_list]
            expected_keys = [b"\x1b[F"]
            if url:
                expected_keys.extend([b"\x1b[A\r", url.encode() + b"\r"])
            self.assertEqual(keys, expected_keys + [b"\r"])
            terminal.finish.assert_called_once_with()

    def test_prompt_timeout_and_dead_child_cannot_pass(self):
        child = Mock(poll=Mock(return_value=None))
        terminal = TerminalSession(-1, child, timeout=0)
        with self.assertRaisesRegex(JourneyError, "TIMEOUT"):
            terminal.expect("home")
        child.poll.return_value = 1
        with self.assertRaisesRegex(JourneyError, "EXITED_BEFORE_PROMPT"):
            TerminalSession(-1, child).expect("home")

    def test_ansi_sequences_are_ignored_and_raw_transcript_is_not_reported(self):
        terminal = TerminalSession(-1, Mock(poll=Mock(return_value=None)))
        with patch.object(terminal, "read", side_effect=[b"\x1b[2Jserv", b"ices\x1b[0m"]):
            terminal.expect("home")
        self.assertEqual(terminal.stages, ["home"])
        self.assertIn(b"services", terminal.screen.text())

    def test_differential_cursor_updates_reconstruct_prompts(self):
        screen = TerminalGrid()
        screen.feed(b"\x1b[2J\x1b[3;4Hstack running")
        self.assertIn(b"stack running", screen.text())
        screen.feed(b"\x1b[3;10Hstopp")
        screen.feed(b"ed")
        self.assertIn(b"stack stopped", screen.text())
        screen.feed(b"\x1b[3;4H\x1b[K")
        self.assertNotIn(b"stack stopped", screen.text())
        screen.feed(b"\x1b[4;")
        screen.feed(b"5Hmcp \x1b[4;9Hregistration")
        self.assertIn(b"mcp registration", screen.text())

    def test_synthetic_pty_exercises_complete_trial_and_database_key_protocols(self):
        # The executable is a strict key/protocol fixture, not product evidence.
        for url in (None, "postgres://postgres@127.0.0.1:6432/owned?sslmode=disable"):
            expected = [(b"\x1b[F", PROMPTS["storage"])]
            if url:
                expected.extend([(b"\x1b[A\r", PROMPTS["url"]),
                                 (url.encode() + b"\r", PROMPTS["url"])])
            expected.append((b"\r", PROMPTS["verified"]))
            expected.append((b"q", b""))
            with self.subTest(url=url), tempfile.TemporaryDirectory(prefix="mt-") as temporary:
                root = Path(temporary)
                script = root / "terminal-fixture"
                script.write_text(f"#!{sys.executable}\n" + "import os,tty\ntty.setraw(0)\n"
                    + "os.write(1," + repr(PROMPTS["storage"]) + ")\n" + "for expected,prompt in " + repr(expected) + ":\n"
                    + " actual=b''\n while len(actual)<len(expected): actual+=os.read(0,len(expected)-len(actual))\n"
                    + " assert actual==expected\n os.write(1,b'\\x1b[2J\\x1b[H'+prompt)\n")
                script.chmod(0o700)
                result = tui_setup(script, root / "config", root, root, url)
            self.assertEqual(result["exit_status"], 0)
            self.assertEqual(result["prompt_stages"][-1], "verified")
            self.assertNotIn("postgres://", str(result))

    def test_synthetic_owned_pty_runs_without_daemon_or_sockets(self):
        # Explicitly a Python terminal fixture, never installed mike evidence.
        with tempfile.TemporaryDirectory(prefix="mt-") as temporary:
            root = Path(temporary)
            script = root / "terminal-fixture"
            script.write_text(f"#!{sys.executable}\n" + '''import os,sys,tty
assert os.isatty(0) and os.isatty(1)
tty.setraw(0)
os.write(1, b"services\\r\\n")
assert os.read(0,1)==b'q'
''')
            script.chmod(0o700)
            with installed_terminal(script, root / "config", root, root) as terminal:
                terminal.expect("home")
                result = terminal.finish()
            self.assertTrue(result["owned_pty"])
            self.assertEqual(result["exit_status"], 0)
            self.assertFalse(result["raw_transcript_retained"])
            self.assertRegex(result["terminal_output_sha256"], r"^[a-f0-9]{64}$")


if __name__ == "__main__":
    unittest.main()
