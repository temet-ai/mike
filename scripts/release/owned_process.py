"""POSIX subprocess groups pinned by an unreaped, private supervisor.

The target may exit and be reaped normally. Its session leader stays alive until
cleanup, so the group number cannot be recycled before the final group signal.
This supervises cooperating descendants, not children that deliberately setsid.
"""
import json
import os
from pathlib import Path
import select
import selectors
import signal
import subprocess
import sys
import time
import zipimport

MAX_OUTPUT_BYTES = 8 * 1024 * 1024
START_TIMEOUT = 5


def supervisor_command():
    if isinstance(__loader__, zipimport.zipimporter):
        return [sys.executable, "-I", "-B", __loader__.archive, "--_owned-process"]
    return [sys.executable, "-I", "-B", str(Path(__file__).absolute())]


class OwnedProcess:
    """Popen-like target status and pipes, with separate retained group ownership."""

    def __init__(self, args, *, cwd=None, env=None, stdin=None, stdout=None,
                 stderr=None, text=False, bufsize=0):
        if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
            raise OSError("owned subprocess requires default child reaping")
        self.args = [os.fspath(value) for value in args]
        self.pid = None
        self.returncode = None
        self.text = text
        self._buffer = bytearray()
        self._can_signal = False
        self._closed = False
        self._forced_returncode = None
        self._anchor = None
        self.stdin = self.stdout = self.stderr = None
        control_read, self._control = os.pipe()
        self._status, status_write = os.pipe()
        try:
            self._anchor = subprocess.Popen(
                [*supervisor_command(), str(control_read), str(status_write), *self.args],
                cwd=cwd, env=env, stdin=stdin, stdout=stdout, stderr=stderr,
                start_new_session=True, pass_fds=(control_read, status_write), bufsize=bufsize,
            )
            self._can_signal = True
            self.stdin, self.stdout, self.stderr = (
                self._anchor.stdin, self._anchor.stdout, self._anchor.stderr)
        finally:
            os.close(control_read)
            os.close(status_write)
            if self._anchor is None:
                os.close(self._control)
                os.close(self._status)
        try:
            os.set_blocking(self._status, False)
            deadline = time.monotonic() + START_TIMEOUT
            while self.pid is None:
                self._read_status()
                if self.pid is None:
                    self._await_status(deadline)
        except subprocess.TimeoutExpired as error:
            self.close()
            raise OSError("owned subprocess launch timed out") from error
        except BaseException:
            self.close()
            raise

    def _read_status(self):
        while True:
            try:
                data = os.read(self._status, 4096)
            except BlockingIOError:
                return
            if not data:
                if self.returncode is None:
                    if self._forced_returncode is None:
                        raise OSError("owned subprocess supervisor stopped unexpectedly")
                    self.returncode = self._forced_returncode
                return
            self._buffer.extend(data)
            if len(self._buffer) > 4096:
                raise OSError("invalid owned subprocess status")
            while b"\n" in self._buffer:
                line, _, remainder = self._buffer.partition(b"\n")
                self._buffer = bytearray(remainder)
                record = json.loads(line)
                if "error" in record:
                    raise OSError("could not start owned subprocess")
                if "pid" in record:
                    self.pid = record["pid"]
                if "returncode" in record:
                    self.returncode = record["returncode"]

    def _await_status(self, deadline):
        remaining = None if deadline is None else deadline - time.monotonic()
        if remaining is not None and remaining <= 0:
            raise subprocess.TimeoutExpired(self.args, 0)
        select.select([self._status], [], [], remaining)

    def poll(self):
        if not self._closed:
            self._read_status()
        return self.returncode

    def wait(self, timeout=None):
        deadline = None if timeout is None else time.monotonic() + timeout
        while self.poll() is None:
            self._await_status(deadline)
        return self.returncode

    def communicate(self, timeout=None):
        if self.stdin is not None and not self.stdin.closed:
            self.stdin.close()
        deadline = None if timeout is None else time.monotonic() + timeout
        streams = [(self.stdout, bytearray()), (self.stderr, bytearray())]
        with selectors.DefaultSelector() as selector:
            selector.register(self._status, selectors.EVENT_READ)
            for stream, output in streams:
                if stream is not None and not stream.closed:
                    selector.register(stream, selectors.EVENT_READ, output)
            while len(selector.get_map()) > 1 or self.poll() is None:
                remaining = None if deadline is None else deadline - time.monotonic()
                if remaining is not None and remaining <= 0:
                    raise subprocess.TimeoutExpired(self.args, timeout)
                for key, _ in selector.select(remaining):
                    if key.fd == self._status:
                        self.poll()
                        continue
                    data = os.read(key.fd, 65536)
                    if not data:
                        selector.unregister(key.fileobj)
                    else:
                        key.data.extend(data)
                        if len(key.data) > MAX_OUTPUT_BYTES:
                            raise OSError("owned subprocess output exceeds limit")
        values = [None if stream is None else bytes(output) for stream, output in streams]
        if self.text:
            return tuple(value.decode("utf-8", errors="replace") if value is not None else None
                         for value in values)
        return tuple(values)

    def signal_group(self, signum):
        # Never poll/wait the anchor here: even an exited anchor must remain
        # unreaped until the last signal, reserving its numeric process group.
        if not self._can_signal:
            return
        if signum == signal.SIGKILL:
            self._forced_returncode = -signal.SIGKILL
        try:
            os.killpg(self._anchor.pid, signum)
        except ProcessLookupError:
            pass

    def close(self, grace=0):
        if self._closed:
            return
        try:
            if grace:
                self.signal_group(signal.SIGTERM)
                try:
                    self.wait(timeout=grace)
                except (OSError, subprocess.TimeoutExpired):
                    pass
        finally:
            try:
                self.signal_group(signal.SIGKILL)
            finally:
                # Disable all future signals BEFORE reaping, including a signal
                # handler that interrupts wait(). Repeated close is harmless.
                self._can_signal = False
                self._closed = True
                try:
                    self._anchor.wait(timeout=5)
                finally:
                    os.close(self._control)
                    os.close(self._status)
                    for stream in (self.stdin, self.stdout, self.stderr):
                        if stream is not None:
                            stream.close()


def _send_status(descriptor, record):
    os.write(descriptor, json.dumps(record, separators=(",", ":")).encode() + b"\n")


def supervise(args):
    """Zipapp entry: two inherited descriptors, followed by the target argv."""
    control, status = map(int, args[:2])
    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        # Caught handlers reset to defaults at exec; SIG_IGN would leak into
        # the target and make ordinary graceful shutdown ineffective.
        signal.signal(signum, lambda _signum, _frame: None)
    try:
        try:
            target = subprocess.Popen(args[2:])
        except (OSError, ValueError):
            _send_status(status, {"error": "spawn"})
            target = None
        else:
            _send_status(status, {"pid": target.pid})
        for descriptor in (0, 1, 2):
            try:
                os.close(descriptor)
            except OSError:
                pass
        reported = target is None
        while True:
            if not reported:
                code = target.poll()
                if code is not None:
                    _send_status(status, {"returncode": code})
                    reported = True
            readable, _, _ = select.select([control], [], [], 0.05 if not reported else None)
            if readable and not os.read(control, 1):
                break
    finally:
        # The controller disappeared, or supervision failed. We are still the
        # live session leader; this can only address our own retained group.
        os.killpg(os.getpgrp(), signal.SIGKILL)


if __name__ == "__main__":
    supervise(sys.argv[1:])
