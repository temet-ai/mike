"""Offline ownership contracts. All AF_UNIX exchanges are mocked, never bound."""
import fcntl
import json
import os
from pathlib import Path
import socket
import stat
import struct
import tempfile
import unittest
from unittest.mock import Mock, patch

from journey_process import JourneyError, private_write
from journey_selected import SelectedInstallation, private_read


@unittest.skipUnless(hasattr(socket, "SO_PEERCRED"), "the selected installation needs Linux peer credentials")
class SelectedInstallationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ms-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.config = self.root / "config.toml"
        private_write(self.config, b"persist_enabled = false\n")
        self.selected = SelectedInstallation(self.config, 11001, "b" * 40)

    def runtime(self):
        selected = self.selected
        selected.root.mkdir(mode=0o700)
        selected.state.mkdir(mode=0o700)
        self.identity = {"instance_id": selected.instance_id, "generation": "00000000-0000-4000-8000-000000000001",
                         "pid": 12345, "started_unix_secs": 1}
        private_write(selected.record, json.dumps(self.identity).encode())
        private_write(selected.state / "daemon.lock", b"")
        lease = open(selected.state / "daemon.lock", "r+b")
        self.addCleanup(lease.close)
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return lease

    def test_refuses_existing_runtime_and_symlink_config_before_network(self):
        with patch("journey_selected.socket.socket") as network:
            self.runtime()
            with self.assertRaisesRegex(JourneyError, "NOT_FRESH"):
                SelectedInstallation(self.config, 1, "b" * 40)
            link = self.root / "link.toml"
            link.symlink_to(self.config)
            with self.assertRaisesRegex(JourneyError, "SYMLINK"):
                SelectedInstallation(link, 1, "b" * 40)
            network.assert_not_called()

    def test_identity_requires_held_private_lease_and_matching_selected_path(self):
        lease = self.runtime()
        self.assertEqual(self.selected.identity(), self.identity)
        fcntl.flock(lease, fcntl.LOCK_UN)
        with self.assertRaisesRegex(JourneyError, "LEASE_NOT_HELD"):
            self.selected.identity()
        fcntl.flock(lease, fcntl.LOCK_EX)
        self.selected.record.chmod(0o644)
        with self.assertRaisesRegex(JourneyError, "PRIVATE_FILE"):
            self.selected.identity()

    def test_private_reads_refuse_symlinks_and_oversize(self):
        link = self.root / "link"
        link.symlink_to(self.config)
        with self.assertRaises(OSError):
            private_read(link)
        private_write(self.root / "large", b"a" * 65537)
        with self.assertRaisesRegex(JourneyError, "PRIVATE_FILE"):
            private_read(self.root / "large")

    def exchange(self, *, peer_pid=12345, response=None):
        self.runtime()
        if response is None:
            response = {"status": {"identity": self.identity, "stopping": True}, "rejection": None}
        stream = Mock()
        stream.__enter__ = Mock(return_value=stream)
        stream.__exit__ = Mock(return_value=False)
        stream.getsockopt.return_value = struct.pack("3i", peer_pid, os.getuid(), os.getgid())
        stream.recv.side_effect = [json.dumps(response).encode(), b""]
        metadata = Mock(st_dev=1, st_ino=2)
        return stream, metadata

    def test_stop_uses_generation_not_pid_signal_and_rejects_foreign_peer(self):
        stream, metadata = self.exchange(peer_pid=12346)
        with patch("journey_selected.socket.socket", return_value=stream), \
                patch("journey_selected.private_metadata", return_value=metadata), \
                patch("journey_selected.os.kill") as signal:
            with self.assertRaisesRegex(JourneyError, "PEER_MISMATCH"):
                self.selected.exchange("stop", self.identity)
            stream.sendall.assert_not_called()
            signal.assert_not_called()

    def test_exact_generation_protocol_and_bounded_output(self):
        stream, metadata = self.exchange()
        with patch("journey_selected.socket.socket", return_value=stream), \
                patch("journey_selected.private_metadata", return_value=metadata):
            result = self.selected.exchange("stop", self.identity)
        request = json.loads(stream.sendall.call_args.args[0])
        self.assertEqual(request["generation"], self.identity["generation"])
        self.assertEqual(request["action"], "stop")
        self.assertEqual(request["instance_id"], self.selected.instance_id)
        self.assertTrue(result["stopping"])
        stream.settimeout.assert_called()
        stream.recv.side_effect = [b"x" * 8193]
        with patch("journey_selected.socket.socket", return_value=stream), \
                patch("journey_selected.private_metadata", return_value=metadata):
            with self.assertRaisesRegex(JourneyError, "OUTPUT_LIMIT"):
                self.selected.exchange("status", self.identity)

    def test_control_rejection_never_acknowledges_stop(self):
        stream, metadata = self.exchange(response={"status": None, "rejection": "generation_mismatch"})
        with patch("journey_selected.socket.socket", return_value=stream), \
                patch("journey_selected.private_metadata", return_value=metadata):
            with self.assertRaisesRegex(JourneyError, "CONTROL_REJECTED"):
                self.selected.exchange("stop", self.identity)

    def test_stop_timeout_does_not_force_signal_or_delete_state(self):
        self.runtime()
        with patch.object(self.selected, "exchange", return_value={"stopping": True}), \
                patch("journey_selected.os.kill") as signal:
            with self.assertRaisesRegex(JourneyError, "STOP_TIMEOUT"):
                self.selected.stop(timeout=0)
        signal.assert_not_called()
        self.assertTrue(self.selected.record.is_file())

    def test_unresolved_detached_launch_never_means_no_daemon_when_lease_is_absent(self):
        self.selected.begin_launch()
        with patch("journey_selected.socket.socket") as network, patch("journey_selected.os.kill") as signal:
            with self.assertRaisesRegex(JourneyError, "STARTUP_CLEANUP_UNRESOLVED"):
                self.selected.stop(timeout=0)
        network.assert_not_called()
        signal.assert_not_called()
        self.assertTrue(self.config.is_file())
        self.assertTrue(self.selected.launch_pending)

    def test_start_mutex_handoff_and_missing_record_refuse_cleanup(self):
        lease = self.runtime()
        private_write(self.selected.state / "start.lock", b"")
        with open(self.selected.state / "start.lock", "r+b") as starting:
            fcntl.flock(starting, fcntl.LOCK_EX)
            with patch.object(self.selected, "exchange") as control:
                with self.assertRaisesRegex(JourneyError, "STARTUP_CLEANUP_UNRESOLVED"):
                    self.selected.stop(timeout=0)
            control.assert_not_called()
        self.selected.record.unlink()
        with self.assertRaisesRegex(JourneyError, "STARTUP_CLEANUP_UNRESOLVED"):
            self.selected.stop(timeout=0)
        self.assertTrue(self.selected.root.exists())
        fcntl.flock(lease, fcntl.LOCK_UN)

    def test_pending_launch_is_cleared_only_after_verified_readiness(self):
        self.runtime()
        self.selected.begin_launch()
        status = {"identity": self.identity, "persist_enabled": False, "ready": True, "stopping": False}
        health = {"instance": {"instance_id": self.selected.instance_id,
                               "boot_generation": self.identity["generation"]},
                  "build": {"sha": "b" * 40}, "persistence": {"enabled": False}}
        with patch.object(self.selected, "status", return_value=status), \
                patch("journey_selected.http_json", return_value=health):
            self.selected.ready(False)
        self.assertFalse(self.selected.launch_pending)

    def test_stopped_fresh_fixture_needs_no_network(self):
        with patch("journey_selected.socket.socket") as network:
            self.assertTrue(self.selected.stop()["already_stopped"])
        network.assert_not_called()

    def test_wrong_live_mode_or_health_identity_is_refused(self):
        self.runtime()
        health = {"instance": {"instance_id": self.selected.instance_id, "boot_generation": self.identity["generation"]},
                  "build": {"sha": "b" * 40}, "persistence": {"enabled": False}}
        status = {"identity": self.identity, "persist_enabled": False, "ready": True, "stopping": False}
        with patch.object(self.selected, "status", return_value=status), \
                patch("journey_selected.http_json", return_value=health):
            self.assertEqual(self.selected.ready(False), health)
            with self.assertRaisesRegex(JourneyError, "RUNTIME_MODE"):
                self.selected.ready(True)
            health["instance"]["boot_generation"] = "foreign"
            with self.assertRaisesRegex(JourneyError, "IDENTITY"):
                self.selected.ready(False)


@unittest.skipIf(hasattr(socket, "SO_PEERCRED"), "covers systems without Linux peer credentials")
class SelectedWithoutLinuxCredentialsTests(unittest.TestCase):
    def test_selected_installation_refuses_before_reading_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / "config.toml"
            with self.assertRaisesRegex(JourneyError, "SELECTED_LINUX_PEER_CREDENTIALS_REQUIRED"):
                SelectedInstallation(config, 11001, "b" * 40)


if __name__ == "__main__":
    unittest.main()
