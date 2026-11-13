#!/usr/bin/env python3
"""Record Codex app-server transcripts of native subagents for the children fixtures.

Reuses ../ingest/fixtures/record.py: a temporary CODEX_HOME holding only a copy of
auth.json, a temporary git repository as the working directory, sanitized frames.
The first session runs two parent turns: one spawns a child, waits, sends it a follow-up,
waits again and closes it; the other spawns a slow child and interrupts it. The second
session reconnects, resumes the parent, reads every child's history, resumes the first
child, tries a direct turn on it (Codex refuses input to a sub-agent), has the parent
resume that child instead, and reads a thread that does not exist.

Usage: record_children.py <output dir>
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(
    0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingest", "fixtures")
)
import record  # noqa: E402

SPAWN_PROMPT = (
    "Spawn exactly one sub-agent. Its task: run the shell command `echo child-one` and reply "
    "with only the output. Wait for it to finish. Then send that same sub-agent a follow-up "
    "task: run `echo child-two` and reply with only the output. Wait for it again. Then close "
    "the sub-agent. Answer with one short sentence."
)
INTERRUPT_PROMPT = (
    "Spawn exactly one sub-agent whose task is to run `sleep 90 && echo late`. Do not wait for "
    "it; immediately interrupt that sub-agent, then answer with one short sentence."
)
CHILD_PROMPT = "Reply with the single word resumed."
RESUME_PROMPT = (
    "Resume the first sub-agent you spawned earlier and send it the task: reply with the "
    "single word resumed. Wait for it, then answer with one short sentence."
)


def children(frames):
    found = []
    for frame in frames:
        if frame["dir"] != "in":
            continue
        message = json.loads(frame["frame"])
        if message.get("method") != "item/completed":
            continue
        item = message["params"]["item"]
        child = item.get("agentThreadId") if item.get("type") == "subAgentActivity" else None
        if child and child not in found:
            found.append(child)
    return found


def turn_done(thread_id):
    return lambda m: m.get("method") == "turn/completed" and m["params"].get("threadId") == thread_id


def live(home, cwd):
    session = record.Session(home, cwd, None)
    record.handshake(session)
    started = session.call(
        "thread/start", {"cwd": cwd, "approvalPolicy": "never", "sandbox": "workspace-write"}
    )
    thread_id = started["result"]["thread"]["id"]
    for prompt in (SPAWN_PROMPT, INTERRUPT_PROMPT):
        session.request(
            "turn/start",
            {"threadId": thread_id, "input": [{"type": "text", "text": prompt}], "effort": "low"},
        )
        session.read_until(turn_done(thread_id), 300)
    record.drain(session, 6)
    session.call("thread/loaded/list", {})
    session.close()
    return session.frames, thread_id, children(session.frames)


def reconnect(home, cwd, thread_id, child_ids):
    session = record.Session(home, cwd, None)
    record.handshake(session)
    session.call("thread/resume", {"threadId": thread_id})
    session.call("thread/read", {"threadId": thread_id, "includeTurns": True})
    for child in [thread_id, *child_ids]:
        if child != thread_id:
            session.call("thread/read", {"threadId": child, "includeTurns": True})
        turns = session.call(
            "thread/turns/list",
            {"threadId": child, "limit": 20, "sortDirection": "asc", "itemsView": "notLoaded"},
        )
        for turn in turns.get("result", {}).get("data", []):
            session.call(
                "thread/items/list",
                {"threadId": child, "turnId": turn["id"], "limit": 100, "sortDirection": "asc"},
            )
    if child_ids:
        session.call("thread/resume", {"threadId": child_ids[0]})
        session.call(
            "turn/start",
            {"threadId": child_ids[0], "input": [{"type": "text", "text": CHILD_PROMPT}],
             "effort": "low"},
        )
        session.request(
            "turn/start",
            {"threadId": thread_id, "input": [{"type": "text", "text": RESUME_PROMPT}],
             "effort": "low"},
        )
        session.read_until(turn_done(thread_id), 240)
        record.drain(session, 3)
    session.call("thread/read", {"threadId": "no-such-child", "includeTurns": False})
    session.close()
    return session.frames


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
        frames, thread_id, child_ids = live(home, cwd)
        header = {"codex_version": version, "synthetic": False, "thread_id": thread_id,
                  "child_thread_ids": child_ids, "recorded_by": "record_children.py"}
        record.write(os.path.join(out_dir, "children_live.jsonl"), header,
                     record.sanitize(frames, replacements))
        frames = reconnect(home, cwd, thread_id, child_ids)
        record.write(os.path.join(out_dir, "children_reconnect.jsonl"), header,
                     record.sanitize(frames, replacements))
    finally:
        shutil.rmtree(home, ignore_errors=True)
        shutil.rmtree(cwd, ignore_errors=True)


if __name__ == "__main__":
    main()
