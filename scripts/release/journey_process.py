"""Owned-process and loopback HTTP helpers for partial archive journeys."""
from contextlib import contextmanager
import hashlib
import http.client
import json
import os
from pathlib import Path
import selectors
import socket
import stat
import subprocess
import time
from common import ReleaseError
from owned_process import OwnedProcess

class JourneyError(ReleaseError):
    """Only static diagnostic codes enter retained evidence."""

def require(condition, code):
    if not condition:
        raise JourneyError(code)

def runtime_environment(home):
    return {"HOME": str(home), "TMPDIR": str(home), "PATH": "", "LC_ALL": "C", "LANG": "C"}

def private_write(path, data):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())

def stop_owned(child):
    child.close(grace=5)

@contextmanager
def owned_child(args, cwd, environment, protocol=False):
    child = OwnedProcess(args, cwd=cwd, env=environment,
                         stdin=subprocess.PIPE if protocol else subprocess.DEVNULL,
                         stdout=subprocess.PIPE if protocol else subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, bufsize=0)
    try:
        yield child
    finally:
        stop_owned(child)

def captured_run(args, cwd, environment, timeout=10):
    with owned_child(args, cwd, environment, protocol=True) as child:
        child.stdin.close()
        output = bytearray()
        deadline = time.monotonic() + timeout
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                require(remaining > 0 and selector.select(remaining), "CHILD_TIMEOUT")
                data = os.read(child.stdout.fileno(), 65536)
                if not data:
                    break
                output.extend(data)
                require(len(output) <= 8 * 1024 * 1024, "CHILD_OUTPUT_LIMIT")
        try:
            child.wait(timeout=max(0.01, deadline - time.monotonic()))
        except subprocess.TimeoutExpired as error:
            raise JourneyError("CHILD_TIMEOUT") from error
        require(child.returncode == 0, "CHILD_NONZERO")
        return bytes(output)

@contextmanager
def reserved_ports(count=3):
    listeners = []
    try:
        for _ in range(count):
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            listener.bind(("127.0.0.1", 0))
            listeners.append(listener)
        yield [listener.getsockname()[1] for listener in listeners], listeners
    finally:
        for listener in listeners:
            listener.close()

def http_bytes(port, path, timeout=5):
    require(isinstance(port, int) and 0 < port < 65536, "INVALID_LOOPBACK_PORT")
    require(path.startswith("/") and not path.startswith("//"), "INVALID_LOCAL_PATH")
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        body = response.read(8 * 1024 * 1024 + 1)
        require(len(body) <= 8 * 1024 * 1024, "HTTP_OUTPUT_LIMIT")
        return response.status, dict(response.getheaders()), body
    finally:
        connection.close()

def http_json(port, path):
    status, _, body = http_bytes(port, path)
    require(status == 200, "HTTP_STATUS")
    return json.loads(body)

def installation_identity(config_path):
    absolute = Path(os.path.abspath(config_path))
    identity = hashlib.sha256(os.fsencode(absolute)).hexdigest()[:32]
    return identity, absolute.parent / (".mike-" + identity) / "state" / "instance.json"

def validate_identity(record, health, expected_id, pid, source_commit, *, persistent=False):
    require(record.get("pid") == pid and record.get("instance_id") == expected_id, "PRIVATE_IDENTITY_MISMATCH")
    instance = health.get("instance", {})
    require(instance.get("instance_id") == expected_id
            and instance.get("boot_generation") == record.get("generation")
            and isinstance(record.get("generation"), str), "PUBLIC_IDENTITY_MISMATCH")
    require(health.get("build", {}).get("sha") == source_commit, "BUILD_IDENTITY_MISMATCH")
    if persistent:
        require(health.get("persistence", {}).get("enabled") is True
                and health["persistence"].get("reachable") is True
                and health.get("status") == "ok", "PERSISTENT_READINESS_MISMATCH")
    else:
        require(health.get("persistence", {}).get("enabled") is False, "TRIAL_MODE_MISMATCH")

def wait_owned_ready(child, config, inspect_port, source_commit, timeout=20, *, persistent=False):
    identity, record_path = installation_identity(config)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        require(child.poll() is None, "DAEMON_EXITED_BEFORE_READY_RUNTIME_OR_LOCAL_ENVIRONMENT")
        try:
            metadata = record_path.lstat()
            require(stat.S_ISREG(metadata.st_mode) and metadata.st_mode & 0o077 == 0,
                    "PRIVATE_IDENTITY_PERMISSIONS")
            record = json.loads(record_path.read_bytes())
            health = http_json(inspect_port, "/api/health")
            validate_identity(record, health, identity, child.pid, source_commit, persistent=persistent)
            return health
        except (FileNotFoundError, ConnectionError, TimeoutError, OSError, http.client.HTTPException):
            time.sleep(0.05)
    raise JourneyError("DAEMON_READINESS_TIMEOUT")
