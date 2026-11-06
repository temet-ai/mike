"""Private installed-daemon control for freshly allocated release fixtures only.

Setup intentionally detaches its daemon. Never turn an on-disk PID into a signal:
verify the held lease, record, Linux peer credentials and generation, then request
cooperative stop over the product protocol. This is not a general service manager.
"""
import fcntl
import http.client
import json
import os
from pathlib import Path
import socket
import stat
import struct
import time
import uuid

from journey_process import (JourneyError, http_json, installation_identity, require,
                             validate_identity)


def private_metadata(path, kind):
    info = Path(path).lstat()
    require(kind(info.st_mode) and info.st_uid == os.getuid() and info.st_mode & 0o077 == 0,
            "SELECTED_PRIVATE_PATH")
    return info


def private_read(path):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as source:
        info = os.fstat(source.fileno())
        require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                and info.st_mode & 0o077 == 0 and info.st_size <= 65536, "SELECTED_PRIVATE_FILE")
        data = source.read(65537)
        require(len(data) <= 65536, "SELECTED_FILE_LIMIT")
        return data


class SelectedInstallation:
    def __init__(self, config, inspect_port, source_commit):
        require(hasattr(socket, "SO_PEERCRED"), "SELECTED_LINUX_PEER_CREDENTIALS_REQUIRED")
        self.config = Path(config).absolute()
        require(self.config == self.config.resolve(strict=True), "SELECTED_CONFIG_SYMLINK")
        private_metadata(self.config.parent, stat.S_ISDIR)
        private_read(self.config)
        self.instance_id, self.record = installation_identity(self.config)
        self.state = self.record.parent
        self.root = self.state.parent
        require(not self.root.exists() and not self.root.is_symlink(), "SELECTED_FIXTURE_NOT_FRESH")
        self.socket = self.state / "control.sock"
        require(len(os.fsencode(self.socket)) <= 103, "SELECTED_SOCKET_PATH_LIMIT")
        self.inspect_port = inspect_port
        self.source_commit = source_commit
        self.launch_pending = False

    def begin_launch(self):
        self.launch_pending = True

    def check_paths(self):
        require(self.config.parent == self.config.parent.resolve(strict=True), "SELECTED_DIRECTORY_CHANGED")
        for path in (self.config.parent, self.root, self.state):
            private_metadata(path, stat.S_ISDIR)

    def held(self):
        return self.lease_held("daemon.lock")

    def starting(self):
        return self.lease_held("start.lock")

    def lease_held(self, name):
        require(name in ("daemon.lock", "start.lock"), "SELECTED_LEASE_NAME")
        if not self.root.exists() and not self.root.is_symlink():
            return False
        private_metadata(self.config.parent, stat.S_ISDIR)
        private_metadata(self.root, stat.S_ISDIR)
        if not self.state.exists() and not self.state.is_symlink():
            return False
        self.check_paths()
        try:
            descriptor = os.open(self.state / name, os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
        except FileNotFoundError:
            return False
        with os.fdopen(descriptor, "r+b") as lease:
            info = os.fstat(lease.fileno())
            require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                    and info.st_mode & 0o077 == 0, "SELECTED_PRIVATE_LEASE")
            try:
                fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return True
            fcntl.flock(lease, fcntl.LOCK_UN)
            return False

    def identity(self):
        self.check_paths()
        value = json.loads(private_read(self.record))
        require(isinstance(value, dict) and set(value) == {
            "instance_id", "generation", "pid", "started_unix_secs"}, "SELECTED_IDENTITY_SCHEMA")
        require(value["instance_id"] == self.instance_id and type(value["pid"]) is int
                and value["pid"] > 0 and type(value["started_unix_secs"]) is int
                and value["started_unix_secs"] > 0 and isinstance(value["generation"], str),
                "SELECTED_IDENTITY_MISMATCH")
        try:
            require(str(uuid.UUID(value["generation"])) == value["generation"], "SELECTED_GENERATION")
        except ValueError as error:
            raise JourneyError("SELECTED_GENERATION") from error
        require(self.held(), "SELECTED_LEASE_NOT_HELD")
        return value

    def exchange(self, action, identity):
        require(action in ("status", "stop"), "SELECTED_CONTROL_ACTION")
        require(hasattr(socket, "SO_PEERCRED"), "SELECTED_LINUX_PEER_CREDENTIALS_REQUIRED")
        require(self.identity() == identity, "SELECTED_GENERATION_CHANGED")
        info = private_metadata(self.socket, stat.S_ISSOCK)
        request = {"protocol": 1, "instance_id": self.instance_id,
                   "generation": identity["generation"] if action == "stop" else None,
                   "boot_fingerprint": None, "action": action}
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
            deadline = time.monotonic() + 2
            stream.settimeout(2)
            stream.connect(str(self.socket))
            pid, uid, _ = struct.unpack("3i", stream.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            require(pid == identity["pid"] and uid == os.getuid(), "SELECTED_CONTROL_PEER_MISMATCH")
            current = private_metadata(self.socket, stat.S_ISSOCK)
            require((info.st_dev, info.st_ino) == (current.st_dev, current.st_ino)
                    and self.identity() == identity, "SELECTED_CONTROL_CHANGED")
            stream.sendall(json.dumps(request).encode())
            stream.shutdown(socket.SHUT_WR)
            raw = bytearray()
            while True:
                remaining = deadline - time.monotonic()
                require(remaining > 0, "SELECTED_CONTROL_TIMEOUT")
                stream.settimeout(remaining)
                data = stream.recv(8193 - len(raw))
                if not data:
                    break
                raw.extend(data)
                require(len(raw) <= 8192, "SELECTED_CONTROL_OUTPUT_LIMIT")
        value = json.loads(raw)
        require(isinstance(value, dict) and value.get("rejection") is None
                and isinstance(value.get("status"), dict), "SELECTED_CONTROL_REJECTED")
        status = value["status"]
        require(status.get("identity") == identity, "SELECTED_CONTROL_IDENTITY")
        return status

    def status(self):
        identity = self.identity()
        status = self.exchange("status", identity)
        require(self.identity() == identity, "SELECTED_GENERATION_CHANGED")
        return status

    def ready(self, persistent, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                status = self.status()
                require(status.get("persist_enabled") is persistent, "SELECTED_RUNTIME_MODE")
                if status.get("ready") is True and status.get("stopping") is False:
                    health = http_json(self.inspect_port, "/api/health")
                    validate_identity(status["identity"], health, self.instance_id,
                                      status["identity"]["pid"], self.source_commit, persistent=persistent)
                    self.launch_pending = False
                    return health
            except (FileNotFoundError, ConnectionError, TimeoutError, http.client.HTTPException):
                pass
            time.sleep(0.05)
        raise JourneyError("SELECTED_READINESS_TIMEOUT")

    def stop(self, timeout=70):
        deadline = time.monotonic() + timeout
        identity = None
        while identity is None:
            # A setup process owns start.lock across spawn/readiness/handoff.
            # A killed setup can release it before its detached child publishes
            # daemon.lock, so an unresolved launch must never mean "no daemon".
            if not self.starting():
                if self.held():
                    try:
                        candidate = self.identity()
                        status = self.exchange("stop", candidate)
                        require(status.get("stopping") is True, "SELECTED_STOP_NOT_ACKNOWLEDGED")
                        identity = candidate
                    except (FileNotFoundError, ConnectionRefusedError):
                        pass
                elif not self.launch_pending:
                    return {"owned_private_control_only": True, "already_stopped": True}
            if identity is None:
                require(time.monotonic() < deadline, "SELECTED_STARTUP_CLEANUP_UNRESOLVED")
                time.sleep(0.05)
        while self.held() or self.starting():
            require(time.monotonic() < deadline, "SELECTED_COOPERATIVE_STOP_TIMEOUT")
            try:
                current = json.loads(private_read(self.record))
            except FileNotFoundError:
                current = None
            require(current is None or current == identity, "SELECTED_STOP_GENERATION_CHANGED")
            time.sleep(0.05)
        self.launch_pending = False
        return {"owned_private_control_only": True, "exact_generation_acknowledged": True,
                "lease_released": True, "pid_signalled": False}
