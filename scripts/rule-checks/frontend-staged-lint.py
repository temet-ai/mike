#!/usr/bin/env python3
"""Commit-time frontend lint on the staged files.

Prettier formats each file and the result is staged. A file with unstaged
changes is refused rather than formatted, so those changes never join the
commit. ESLint refuses an error on a line this commit adds; errors already in
the file do not block it.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

FORMATTED = (".ts", ".js", ".mjs", ".svelte", ".css", ".json")
LINTED = (".ts", ".js", ".mjs", ".svelte")


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True)


def git(*args):
    return run("git", *args).stdout


def added_lines(path):
    lines = set()
    for match in re.finditer(r"^@@ -\S+ \+(\d+)(?:,(\d+))? @@", git("diff", "--cached", "-U0", "--", path), re.M):
        start, count = int(match[1]), int(match[2] or 1)
        lines.update(range(start, start + count))
    return lines


def main():
    root = Path(git("rev-parse", "--show-toplevel").strip())
    frontend = root / "frontend"
    if not (frontend / "node_modules").is_dir():
        return 0
    staged = git("diff", "--cached", "--name-only", "--diff-filter=ACMR", "--", "frontend").split("\n")
    staged = [p for p in staged if p.endswith(FORMATTED) and (root / p).is_file()]
    if not staged:
        return 0

    status = 0
    ready = []
    for path in staged:
        if run("git", "diff", "--quiet", "--", path, cwd=root).returncode:
            print(f"{path}: has unstaged changes, so it cannot be formatted; run pnpm format")
            status = 1
        else:
            ready.append(path)

    if ready:
        before = {p: (root / p).read_bytes() for p in ready}
        relative = [p.removeprefix("frontend/") for p in ready]
        run("pnpm", "exec", "prettier", "--write", "--ignore-unknown", *relative, cwd=frontend)
        record = Path(root, git("rev-parse", "--git-dir").strip(), "rust-staged-formatted")
        for path in ready:
            if (root / path).read_bytes() != before[path]:
                run("git", "add", "--", path, cwd=root)
                with record.open("a") as listed:
                    listed.write(path + "\n")

    linted = [p.removeprefix("frontend/") for p in ready if p.endswith(LINTED)]
    if linted:
        output = run("pnpm", "exec", "eslint", "-f", "json", *linted, cwd=frontend).stdout
        for result in json.loads(output or "[]"):
            path = str(Path(result["filePath"]).relative_to(root))
            added = added_lines(path)
            for message in result["messages"]:
                if message["severity"] == 2 and message.get("line") in added:
                    print(f"{path}:{message['line']}: {message['ruleId']}: {message['message']}")
                    status = 1
    return status


if __name__ == "__main__":
    sys.exit(main())
