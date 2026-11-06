"""Bounded, explicit local Docker transport. Importing never touches the daemon."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import platform
import re
import selectors
import stat
import subprocess
import tempfile
import time

from journey_process import JourneyError, require
from owned_process import OwnedProcess
from journey_reliability_seccomp import pinned_profile_bytes, write_profile
from journey_reliability_peer import verify_peer

MAX_RESPONSE = 2 * 1024 * 1024
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
CONTAINER_ID = re.compile(r"[0-9a-f]{64}")
IMAGE_REF = re.compile(r"[a-z0-9][a-z0-9./:_-]*@sha256:[0-9a-f]{64}")


def bounded_command(args, *, cwd, environment, timeout=30, input_file=None, limit=MAX_RESPONSE):
    """Input uses an already bounded file; output cannot grow past its ceiling."""
    child = None
    try:
        child = OwnedProcess(args, cwd=cwd, env=environment,
                             stdin=input_file or subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        output = bytearray()
        deadline = time.monotonic() + timeout
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                require(remaining > 0, "ISOLATED_DOCKER_TIMEOUT")
                require(selector.select(remaining), "ISOLATED_DOCKER_TIMEOUT")
                data = os.read(child.stdout.fileno(), min(65536, limit + 1 - len(output)))
                if not data:
                    break
                output.extend(data)
                require(len(output) <= limit, "ISOLATED_DOCKER_OUTPUT_LIMIT")
        child.wait(timeout=max(0.01, deadline - time.monotonic()))
        require(child.returncode == 0, "ISOLATED_DOCKER_COMMAND_FAILED")
        return bytes(output)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise JourneyError("ISOLATED_DOCKER_UNAVAILABLE_OR_TIMEOUT") from error
    finally:
        if child is not None:
            child.close()


def plain_absolute(path, *, socket=False):
    path = Path(path)
    require(path.is_absolute() and str(path) == os.path.normpath(str(path)), "ISOLATED_LOCAL_PATH_REQUIRED")
    require(not any(item.is_symlink() for item in (path, *path.parents)), "ISOLATED_LOCAL_PATH_SYMLINK")
    value = path.lstat()
    require(stat.S_ISSOCK(value.st_mode) if socket else stat.S_ISREG(value.st_mode),
            "ISOLATED_ENDPOINT_NOT_UNIX_SOCKET" if socket else "ISOLATED_DOCKER_NOT_REGULAR")
    if not socket:
        require(os.access(path, os.X_OK) and not value.st_mode & 0o022
                and value.st_uid in (0, os.getuid()), "ISOLATED_DOCKER_NOT_TRUSTED_LOCAL_EXECUTABLE")
    return path, (value.st_dev, value.st_ino)


class LocalDocker:
    """Never consult inherited DOCKER_HOST, context, credentials, proxy or SSH."""
    def __init__(self, binary, endpoint, private_root):
        require(platform.system() == "Linux" and platform.machine() == "x86_64",
                "ISOLATED_RUNNER_REQUIRES_NATIVE_LINUX_AMD64")
        self.binary, self.binary_identity = plain_absolute(binary)
        self.endpoint, self.endpoint_identity = plain_absolute(endpoint, socket=True)
        self.verified_peer = None
        self.root = private_root
        self.config = private_root / "docker-config"
        self.config.mkdir(mode=0o700)
        (self.config / "config.json").write_text("{}\n")
        (self.config / "config.json").chmod(0o600)
        self.seccomp_profile = self.config / "normal-default-seccomp.json"
        write_profile(self.seccomp_profile)
        self.environment = {"HOME": str(private_root), "TMPDIR": str(private_root),
                            "PATH": "", "LC_ALL": "C", "LANG": "C"}

    def command(self, args, **options):
        descriptor = os.open(self.seccomp_profile, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, "rb") as source:
            metadata = os.fstat(source.fileno())
            require(stat.S_ISREG(metadata.st_mode) and metadata.st_size <= 65536
                    and metadata.st_uid == os.getuid() and not metadata.st_mode & 0o077
                    and source.read(65537) == pinned_profile_bytes(), "ISOLATED_SECCOMP_FILE_CHANGED")
        require(plain_absolute(self.binary)[1] == self.binary_identity
                and plain_absolute(self.endpoint, socket=True)[1] == self.endpoint_identity,
                "ISOLATED_DOCKER_ENDPOINT_CHANGED")
        self.verified_peer = verify_peer(self.endpoint, self.verified_peer)
        return bounded_command([str(self.binary), "--config", str(self.config),
                                "--host", "unix://" + str(self.endpoint), *args],
                               cwd=self.root, environment=self.environment, **options)

    def inspect(self, kind, identity):
        raw = self.command([kind, "inspect", identity])
        try:
            value = json.loads(raw)
        except (ValueError, UnicodeError) as error:
            raise JourneyError("ISOLATED_DOCKER_INSPECT_JSON") from error
        require(isinstance(value, list) and len(value) == 1 and isinstance(value[0], dict),
                "ISOLATED_DOCKER_INSPECT_SHAPE")
        return value[0]


@contextmanager
def local_docker(binary, endpoint):
    with tempfile.TemporaryDirectory(prefix="mike-isolated-docker-") as temporary:
        yield LocalDocker(binary, endpoint, Path(temporary))
