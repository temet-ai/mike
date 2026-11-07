"""Snapshot explicit local inputs before verifying or installing candidate bytes."""

from contextlib import contextmanager
import json
import os
import re
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import zipimport

if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent))

from archive import BINARIES, SUPPORT_FILES, extract_checked, smoke, verify_candidate
from common import ReleaseError, TARGETS, sha256
from owned_process import OwnedProcess

MAX_ARCHIVE_BYTES = 2 * 1024 * 1024 * 1024


def clean_environment(home):
    return {"HOME": str(home), "TMPDIR": str(home), "PATH": "/usr/bin:/bin",
            "LC_ALL": "C", "LANG": "C", "PYTHONDONTWRITEBYTECODE": "1"}


def verification_command():
    # A module's __file__ inside a zipapp is not an executable filesystem path.
    # Re-enter the same immutable kit, not the developer checkout or caller CWD.
    if isinstance(__loader__, zipimport.zipimporter):
        return [sys.executable, "-I", "-B", __loader__.archive, "--_candidate-inputs"]
    return [sys.executable, "-I", "-B", str(Path(__file__).absolute())]


def owned_run(args, cwd, env, timeout=180):
    """Bound a child process group created by this invocation, never a PID file."""
    try:
        child = OwnedProcess(args, cwd=cwd, env=env, text=True,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    except OSError as exc:
        raise ReleaseError(f"could not execute local verification: {exc}") from exc
    try:
        stdout, stderr = child.communicate(timeout=timeout)
        if child.returncode:
            raise ReleaseError(f"local verification failed ({child.returncode}): {stderr.strip()}")
        return stdout
    except subprocess.TimeoutExpired as exc:
        raise ReleaseError("local verification timed out; owned process group stopped") from exc
    finally:
        child.close()


@contextmanager
def interruption_cleanup():
    def stop(signum, _frame):
        signal.signal(signum, signal.SIG_IGN)
        raise ReleaseError("interrupted; owned temporary work will be cleaned up")
    previous = {sig: signal.signal(sig, stop) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        yield
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def copy_regular(source, destination, limit):
    descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > limit:
            raise ReleaseError(f"not a bounded regular input file: {source.name}")
        with destination.open("xb") as output:
            remaining = metadata.st_size
            while remaining:
                block = stream.read(min(1024 * 1024, remaining))
                if not block:
                    raise ReleaseError(f"input changed while copying: {source.name}")
                output.write(block)
                remaining -= len(block)
            if stream.read(1):
                raise ReleaseError(f"input grew while copying: {source.name}")
        if destination.stat().st_size != metadata.st_size:
            raise ReleaseError(f"input changed while copying: {source.name}")
    destination.chmod(0o600)


def snapshot_inputs(candidate, destination, *, max_total_bytes=None):
    if max_total_bytes is not None and (type(max_total_bytes) is not int or max_total_bytes <= 0):
        raise ReleaseError("invalid aggregate candidate input limit")
    remaining = max_total_bytes
    def selected_copy(source, target, limit):
        nonlocal remaining
        copy_regular(source, target, limit if remaining is None else min(limit, remaining))
        if remaining is not None:
            remaining -= target.stat().st_size
    if candidate.is_symlink() or not candidate.is_dir():
        raise ReleaseError("--candidate must name a local, non-symlink directory")
    selected_copy(candidate / "manifest.json", destination / "manifest.json", 8 * 1024 * 1024)
    manifest = json.loads((destination / "manifest.json").read_text())
    version, target = manifest["version"], manifest["target"]
    # Do not construct a path from unchecked manifest strings.
    if not isinstance(version, str) or not re.fullmatch(r"[0-9A-Za-z.+-]+", version):
        raise ReleaseError("invalid candidate version")
    if not isinstance(target, str) or target not in TARGETS:
        raise ReleaseError("unsupported candidate platform")
    expected = {"manifest.json"}
    for binary in BINARIES:
        filename = f"{binary}-v{version}-{target}.tar.gz"
        for name, limit in ((filename, MAX_ARCHIVE_BYTES), (filename + ".sha256", 1024)):
            expected.add(name)
            selected_copy(candidate / name, destination / name, limit)
    if {path.name for path in candidate.iterdir()} != expected:
        raise ReleaseError("candidate directory contains unexpected files")
    return manifest


@contextmanager
def verified_inputs(candidate):
    """Verification executes explicitly selected trusted binaries, not a sandbox."""
    with tempfile.TemporaryDirectory(prefix="mike-local-candidate-") as temporary:
        root = Path(temporary).resolve()
        snapshot, home, cwd = (root / name for name in ("candidate", "home", "cwd"))
        for path in (snapshot, home, cwd):
            path.mkdir(mode=0o700)
        manifest = snapshot_inputs(candidate.absolute(), snapshot)
        owned_run([*verification_command(), str(snapshot)],
                  cwd, clean_environment(home))
        # All following extraction uses the private verified copy, not input paths
        # another process can replace between verification and installation.
        yield snapshot, manifest


def extract_pair(snapshot, manifest, destination):
    shutil.copyfile(snapshot / "manifest.json", destination / "manifest.json")
    for artifact in manifest["artifacts"]:
        binary = artifact["binary"]
        unpacked = destination / ("." + binary)
        unpacked.mkdir(mode=0o700)
        extract_checked(snapshot / artifact["archive"], unpacked, binary, artifact["contents"])
        (unpacked / binary).rename(destination / binary)
        if binary == "mike":
            for name in SUPPORT_FILES:
                if (unpacked / name).exists():
                    (unpacked / name).rename(destination / name)
            if (unpacked / "share").exists():
                (unpacked / "share").rename(destination / "share")
        # Keep licenses per binary; these are useful installed provenance.
        unpacked.rename(destination / (binary + "-notices"))
    shutil.copyfile(snapshot / "manifest.json", destination / "manifest.json")
    return sha256(destination / "manifest.json")


def isolated_smoke(binary, version, cwd, home):
    output = owned_run([*verification_command(), "--smoke",
                        str(binary), version, str(cwd), str(home)],
                       cwd, clean_environment(home), timeout=75)
    return json.loads(output)


if __name__ == "__main__":
    try:
        if sys.argv[1] == "--smoke":
            print(json.dumps(smoke(Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4]), Path(sys.argv[5]))))
        else:
            verify_candidate(Path(sys.argv[1]))
    except Exception as exc:
        print(f"candidate verification: {exc}", file=sys.stderr)
        sys.exit(1)
