#!/usr/bin/env python3
"""Serialize sb-test and clean only the process group this invocation created."""
from __future__ import annotations

import fcntl
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import time


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
from owned_process import OwnedProcess


class Runner:
    def __init__(self) -> None:
        self.child: OwnedProcess | None = None
        self.interrupted: int | None = None
        self.interrupted_at: float | None = None

    def signal(self, signum: int, _frame: object) -> None:
        if self.interrupted is None:
            self.interrupted = signum
            self.interrupted_at = time.monotonic()
        self.signal_owned_group(signum)

    def signal_owned_group(self, signum: int) -> None:
        if self.child is None:
            return
        self.child.signal_group(signum)

    def acquire(self, descriptor: int) -> bool:
        announced = False
        while self.interrupted is None:
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return True
            except BlockingIOError:
                if not announced:
                    print("sb-test: waiting for the active owned test run", file=sys.stderr, flush=True)
                    announced = True
                time.sleep(0.1)
        return False

    def run(self, args: list[str]) -> int:
        # setsid is supplied by Python on both Linux and macOS, not a host CLI.
        self.child = OwnedProcess(["bash", args[0], "--owned-test-child", *args[1:]])
        try:
            while True:
                try:
                    return self.child.wait(timeout=0.1)
                except subprocess.TimeoutExpired:
                    if self.interrupted_at is not None and time.monotonic() - self.interrupted_at > 2:
                        self.signal_owned_group(signal.SIGKILL)
        finally:
            self.clean_owned_group()

    def clean_owned_group(self) -> None:
        if self.child is not None:
            self.child.close(grace=1)


def open_lock() -> int:
    # A stable location also serializes checkouts with different HOME/TMPDIR/XDG
    # environments. Never unlink this inode: flock waiters must share it.
    root = Path("/tmp")
    flags = os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(root / f"mike-sb-test-{os.getuid()}.lock", flags, 0o600)
    info = os.fstat(descriptor)
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid():
        os.close(descriptor)
        raise RuntimeError("test lock is not an owned regular file")
    os.fchmod(descriptor, 0o600)
    return descriptor


def main(args: list[str]) -> int:
    if not args:
        print("owned-test-runner: expected the sb-test script path", file=sys.stderr)
        return 2
    runner = Runner()
    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(signum, runner.signal)
    descriptor = open_lock()
    try:
        result = runner.run(args) if runner.acquire(descriptor) else 1
    finally:
        os.close(descriptor)
    if runner.interrupted is not None:
        name = signal.Signals(runner.interrupted).name
        print(f"sb-test: FAIL (aborted by {name} before it finished)", flush=True)
        return 128 + runner.interrupted
    return result if result >= 0 else 128 - result


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, RuntimeError) as error:
        print(f"sb-test: FAIL (owned runner could not start: {error})", file=sys.stderr)
        sys.exit(1)
