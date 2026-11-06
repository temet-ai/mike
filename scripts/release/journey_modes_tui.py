"""Bounded PTY path through the installed TUI's existing setup and home keys.

Prompt matching is not visual qualification. Raw terminal output, config paths
and typed database URLs remain ephemeral; only hashes and named stages survive.
"""
from contextlib import contextmanager
import codecs
import errno
import fcntl
import hashlib
import os
import pty
import re
import selectors
import struct
import termios
import time
import unicodedata

from journey_process import JourneyError, require, runtime_environment
from owned_process import OwnedProcess

CSI = re.compile(r"\x1b\[([0-?]*)([ -/]*)([@-~])")
PROMPTS = {"home": b"services", "storage": b"Keep captures in memory only",
           "url": b"connection URL", "verified": b"Storage saved:"}


class TerminalGrid:
    """Fixed 160×50 Crossterm screen, including differential cursor draws."""
    def __init__(self):
        self.rows = [[" "] * 160 for _ in range(50)]
        self.row = self.column = 0
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")

    def text(self):
        return "\n".join("".join(row) for row in self.rows).encode()

    def feed(self, data):
        self.pending += self.decoder.decode(data)
        while self.pending:
            char = self.pending[0]
            if char == "\x1b":
                match = CSI.match(self.pending)
                if match:
                    self.control(match[1], match[3])
                    self.pending = self.pending[match.end():]
                    continue
                if len(self.pending) == 1 or self.pending.startswith("\x1b["):
                    require(len(self.pending) < 65536, "MODE_TUI_ESCAPE_LIMIT")
                    return
                self.pending = self.pending[2:]
                continue
            self.pending = self.pending[1:]
            if char == "\r":
                self.column = 0
            elif char == "\n":
                self.row = min(49, self.row + 1)
            elif char == "\b":
                self.column = max(0, self.column - 1)
            elif char >= " " and char != "\x7f":
                self.rows[self.row][self.column] = char
                width = 0 if unicodedata.combining(char) else 2 if unicodedata.east_asian_width(char) in "WF" else 1
                self.column = min(159, self.column + width)

    def control(self, arguments, code):
        if any(char not in "0123456789;" for char in arguments):
            return
        values = [int(part or "0") for part in arguments.split(";")]
        first = values[0]
        if code in ("H", "f"):
            self.row = max(0, min(49, (first or 1) - 1))
            self.column = max(0, min(159, ((values[1] if len(values) > 1 else 1) or 1) - 1))
        elif code == "G":
            self.column = max(0, min(159, (first or 1) - 1))
        elif code == "A":
            self.row = max(0, self.row - (first or 1))
        elif code == "B":
            self.row = min(49, self.row + (first or 1))
        elif code == "C":
            self.column = min(159, self.column + (first or 1))
        elif code == "D":
            self.column = max(0, self.column - (first or 1))
        elif code == "J" and first in (2, 3):
            self.rows = [[" "] * 160 for _ in range(50)]
        elif code == "K":
            left, right = (0 if first in (1, 2) else self.column), (self.column + 1 if first == 1 else 160)
            self.rows[self.row][left:right] = [" "] * (right - left)


class TerminalSession:
    def __init__(self, descriptor, child, timeout=330):
        self.descriptor = descriptor
        self.child = child
        self.deadline = time.monotonic() + timeout
        self.screen = TerminalGrid()
        self.digest = hashlib.sha256()
        self.bytes_seen = 0
        self.stages = []

    def read(self):
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, "MODE_TUI_TIMEOUT")
        with selectors.DefaultSelector() as selector:
            selector.register(self.descriptor, selectors.EVENT_READ)
            require(selector.select(min(remaining, 1)), "MODE_TUI_READ_WAIT")
        try:
            data = os.read(self.descriptor, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                return b""
            raise
        self.digest.update(data)
        self.bytes_seen += len(data)
        require(self.bytes_seen <= 8 * 1024 * 1024, "MODE_TUI_OUTPUT_LIMIT")
        return data

    def expect(self, stage):
        require(stage in PROMPTS, "MODE_TUI_STAGE")
        while PROMPTS[stage] not in self.screen.text():
            require(self.child.poll() is None, "MODE_TUI_EXITED_BEFORE_PROMPT")
            try:
                data = self.read()
            except JourneyError as error:
                if str(error) == "MODE_TUI_READ_WAIT":
                    continue
                raise
            require(data, "MODE_TUI_EOF_BEFORE_PROMPT")
            self.screen.feed(data)
        self.stages.append(stage)

    def send(self, value):
        require(self.child.poll() is None and time.monotonic() < self.deadline, "MODE_TUI_NOT_RUNNING")
        require(isinstance(value, bytes) and 0 < len(value) <= 65536, "MODE_TUI_INPUT_LIMIT")
        offset = 0
        while offset < len(value):
            remaining = self.deadline - time.monotonic()
            require(remaining > 0, "MODE_TUI_INPUT_TIMEOUT")
            with selectors.DefaultSelector() as selector:
                selector.register(self.descriptor, selectors.EVENT_WRITE)
                require(selector.select(remaining), "MODE_TUI_INPUT_TIMEOUT")
            try:
                offset += os.write(self.descriptor, value[offset:offset + 4096])
            except BlockingIOError:
                continue

    def finish(self):
        self.send(b"q")
        while self.child.poll() is None:
            try:
                self.read()
            except JourneyError as error:
                if str(error) != "MODE_TUI_READ_WAIT":
                    raise
        require(self.child.returncode == 0, "MODE_TUI_NONZERO")
        return {"owned_pty": True, "exit_status": 0, "prompt_stages": self.stages,
                "terminal_output_sha256": self.digest.hexdigest(), "raw_transcript_retained": False}


@contextmanager
def installed_terminal(binary, config, home, cwd, arguments=()):
    master, slave = pty.openpty()
    child = None
    try:
        os.set_blocking(master, False)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
        environment = dict(runtime_environment(home), TERM="xterm-256color")
        child = OwnedProcess([str(binary), "--config", str(config), *arguments], cwd=cwd, env=environment,
                             stdin=slave, stdout=slave, stderr=slave)
        os.close(slave)
        slave = None
        yield TerminalSession(master, child)
    finally:
        if child is not None:
            child.close(grace=1)
        os.close(master)
        if slave is not None:
            os.close(slave)


def tui_setup(binary, config, home, cwd, database_url=None):
    with installed_terminal(binary, config, home, cwd, ("setup", "database")) as terminal:
        terminal.expect("storage")
        terminal.send(b"\x1b[F")
        if database_url is not None:
            terminal.send(b"\x1b[A\r")
            terminal.expect("url")
            terminal.send(database_url.encode("utf-8") + b"\r")
        terminal.send(b"\r")
        terminal.expect("verified")
        return terminal.finish()


def tui_restart(binary, config, home, cwd, selected, persistent):
    with installed_terminal(binary, config, home, cwd) as terminal:
        terminal.expect("home")
        terminal.send(b"g\t")
        terminal.send(b"t")
        deadline = time.monotonic() + 75
        while selected.held():
            require(time.monotonic() < deadline, "MODE_TUI_STOP_TIMEOUT")
            require(terminal.child.poll() is None, "MODE_TUI_EXITED_DURING_STOP")
            time.sleep(0.05)
        # The final home redraw is produced after stop_stack returns. Matching
        # it prevents a queued toggle from racing the still-running stop action.
        stopped = b"stack stopped"
        while stopped not in terminal.screen.text():
            require(terminal.child.poll() is None, "MODE_TUI_EXITED_DURING_STOP")
            try:
                data = terminal.read()
                require(data, "MODE_TUI_EOF_DURING_STOP")
                terminal.screen.feed(data)
            except JourneyError as error:
                if str(error) != "MODE_TUI_READ_WAIT":
                    raise
        selected.begin_launch()
        terminal.send(b"t")
        health = selected.ready(persistent, timeout=250)
        return terminal.finish(), health
