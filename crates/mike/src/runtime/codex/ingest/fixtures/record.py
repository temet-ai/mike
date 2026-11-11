#!/usr/bin/env python3
"""Record Codex app-server transcripts for the ingest fixtures.

Runs the installed `codex app-server` in a temporary CODEX_HOME holding only a copy of
auth.json, with its working directory in a temporary git repository, and writes one
JSON line per frame: {"dir": "out"|"in", "frame": <the exact line>}. "out" is what the
client sent, "in" what the server wrote. Temporary paths, the host name and the
installation id are replaced with fixed placeholders.

Usage: record.py <output dir>
"""

import json
import os
import queue
import threading
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

TURN_PROMPT = (
    "First call your plan tool with a three-step plan. Then run the shell command "
    "`sleep 4 && echo slow-done`. Then create a file notes.txt containing the word hello. "
    "Answer with one short sentence."
)
INTERRUPT_PROMPT = "Run the shell command `sleep 60 && echo never` and report its output."


class Session:
    def __init__(self, home, cwd, out):
        self.proc = subprocess.Popen(
            ["codex", "app-server"],
            cwd=cwd,
            env={**os.environ, "CODEX_HOME": home},
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )
        self.out = out
        self.next_id = 1
        self.frames = []
        self.lines = queue.Queue()
        threading.Thread(target=self.pump, daemon=True).start()

    def pump(self):
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def write(self, message):
        line = json.dumps(message)
        self.frames.append({"dir": "out", "frame": line})
        print("out", line[:160], file=sys.stderr, flush=True)
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()

    def request(self, method, params):
        request_id = self.next_id
        self.next_id += 1
        self.write({"id": request_id, "method": method, "params": params})
        return request_id

    def read_until(self, predicate, deadline_s):
        deadline = time.time() + deadline_s
        while time.time() < deadline:
            try:
                line = self.lines.get(timeout=max(0.0, deadline - time.time()))
            except queue.Empty:
                break
            if line is None:
                raise RuntimeError("app-server closed stdout")
            line = line.rstrip("\n")
            self.frames.append({"dir": "in", "frame": line})
            print("in", line[:160], file=sys.stderr, flush=True)
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if predicate(message):
                return message
        raise RuntimeError("deadline passed")

    def call(self, method, params, deadline_s=30):
        request_id = self.request(method, params)
        return self.read_until(lambda m: m.get("id") == request_id and "method" not in m, deadline_s)

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def handshake(session):
    session.call("initialize", {"clientInfo": {"name": "mike-fixture", "version": "1"}})
    session.write({"method": "initialized"})


def drain(session, seconds):
    try:
        session.read_until(lambda message: False, seconds)
    except RuntimeError:
        pass


def notified(method):
    return lambda message: message.get("method") == method


def first_session(home, cwd):
    session = Session(home, cwd, None)
    handshake(session)
    started = session.call(
        "thread/start",
        {"cwd": cwd, "approvalPolicy": "never", "sandbox": "workspace-write"},
    )
    thread_id = started["result"]["thread"]["id"]
    session.request(
        "turn/start",
        {"threadId": thread_id, "input": [{"type": "text", "text": TURN_PROMPT}], "effort": "low"},
    )
    completed = session.read_until(notified("turn/completed"), 240)
    turn_id = completed["params"]["turn"]["id"]
    session.call("thread/read", {"threadId": thread_id, "includeTurns": False})
    first = session.call(
        "thread/turns/list",
        {"threadId": thread_id, "limit": 1, "sortDirection": "asc", "itemsView": "notLoaded"},
    )
    page = session.call(
        "thread/items/list",
        {"threadId": thread_id, "turnId": turn_id, "limit": 3, "sortDirection": "asc"},
    )
    cursor = page.get("result", {}).get("nextCursor")
    if cursor:
        session.call(
            "thread/items/list",
            {"threadId": thread_id, "turnId": turn_id, "limit": 50, "cursor": cursor,
             "sortDirection": "asc"},
        )
    session.call("thread/read", {"threadId": "no-such-thread", "includeTurns": False})
    session.request(
        "turn/start",
        {"threadId": thread_id, "input": [{"type": "text", "text": INTERRUPT_PROMPT}],
         "effort": "low"},
    )
    started_command = session.read_until(
        lambda m: m.get("method") == "item/started"
        and m["params"]["item"].get("type") == "commandExecution",
        120,
    )
    session.call(
        "turn/interrupt",
        {"threadId": thread_id, "turnId": started_command["params"]["turnId"]},
    )
    session.read_until(notified("turn/completed"), 60)
    forked = session.call("thread/fork", {"threadId": thread_id})
    session.call("thread/compact/start", {"threadId": thread_id})
    session.read_until(notified("turn/completed"), 180)
    drain(session, 8)
    session.close()
    return session.frames, thread_id, forked.get("result", {}).get("thread", {}).get("id")


def reconnect_session(home, cwd, thread_id):
    session = Session(home, cwd, None)
    handshake(session)
    session.call("thread/resume", {"threadId": thread_id})
    session.call("thread/read", {"threadId": thread_id, "includeTurns": False})
    turns = session.call(
        "thread/turns/list",
        {"threadId": thread_id, "limit": 20, "sortDirection": "asc", "itemsView": "notLoaded"},
    )
    for turn in turns.get("result", {}).get("data", []):
        session.call(
            "thread/items/list",
            {"threadId": thread_id, "turnId": turn["id"], "limit": 100, "sortDirection": "asc"},
        )
    session.close()
    return session.frames


def sanitize(frames, replacements):
    host = socket.gethostname()
    for frame in frames:
        text = frame["frame"]
        for old, new in replacements:
            text = text.replace(old, new)
        text = text.replace(host, "<host>")
        text = re.sub(r'"installationId":"[^"]*"', '"installationId":"<installation>"', text)
        frame["frame"] = text
    return frames


def write(path, header, frames):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(header) + "\n")
        for frame in frames:
            handle.write(json.dumps(frame) + "\n")


def main():
    out_dir = sys.argv[1]
    version = subprocess.run(["codex", "--version"], capture_output=True, text=True).stdout.strip()
    home = tempfile.mkdtemp(prefix="codex-home-")
    cwd = tempfile.mkdtemp(prefix="codex-repo-")
    subprocess.run(["git", "init", "-q", cwd], check=True)
    shutil.copy(os.path.expanduser("~/.codex/auth.json"), os.path.join(home, "auth.json"))
    os.chmod(os.path.join(home, "auth.json"), 0o600)
    replacements = [(home, "<codex-home>"), (cwd, "<repo>")]
    try:
        frames, thread_id, fork_id = first_session(home, cwd)
        header = {"codex_version": version, "synthetic": False, "thread_id": thread_id,
                  "fork_thread_id": fork_id, "recorded_by": "record.py"}
        write(os.path.join(out_dir, "live_turn.jsonl"), header, sanitize(frames, replacements))
        frames = reconnect_session(home, cwd, thread_id)
        header = {"codex_version": version, "synthetic": False, "thread_id": thread_id,
                  "recorded_by": "record.py"}
        write(os.path.join(out_dir, "reconnect.jsonl"), header, sanitize(frames, replacements))
    finally:
        shutil.rmtree(home, ignore_errors=True)
        shutil.rmtree(cwd, ignore_errors=True)


if __name__ == "__main__":
    main()
