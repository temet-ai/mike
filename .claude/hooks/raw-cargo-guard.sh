#!/bin/sh
# PreToolUse hook, matcher "Bash": refuse a command that runs cargo's compiling subcommands
# (build, check, test, clippy, nextest, and the aliases b, c, t) or cargo-nextest directly.
#
# scripts/cx sends those through the shared mbx store at the lowest priority, on at most half
# the processors and one heavy build at a time on the machine (scripts/lib/heavy-build.sh).
# A raw `cargo build` skips all of it: in a new worktree it compiles everything cold, and beside
# another session's build it overloads the machine (2026-11-13: load 16 on 16 cores, 83°C,
# 12 GB in swap). make, scripts/lint-rust, the push hook and scripts/deploy-proxy.sh call cargo
# through cx or under the same lock, so they are not refused: the hook reads only the command
# the session typed, not what a script runs.
#
# Allowed: cargo fmt, metadata, tree and every other subcommand; commands that only mention
# cargo (grep cargo, echo "cargo build"). Looks through `bash -c '...'`, env assignments and the
# wrappers env, nice, ionice, time, timeout, nohup, command and exec.
#
# Bypass for one command: prefix it with MIKE_RAW_CARGO=1; for a session: export MIKE_RAW_CARGO=1.
# Fails open: any error in this script exits 0 with no output, so a broken hook never blocks work.

input="$(cat 2>/dev/null)" || exit 0
[ "${MIKE_RAW_CARGO:-}" = "1" ] && exit 0
command -v python3 >/dev/null 2>&1 || exit 0
case "$input" in
  *cargo*) ;;
  *) exit 0 ;;
esac

printf '%s' "$input" | python3 -c '
import json, os, re, shlex, sys

HEAVY = {"build", "b", "check", "c", "test", "t", "clippy", "nextest"}
WRAPPERS = {"env", "nice", "ionice", "time", "timeout", "nohup", "command", "exec", "sudo"}
WRAPPER_VALUE_OPTIONS = {"-n", "-c", "-p", "--adjustment", "--class", "-k", "--kill-after", "-s", "--signal", "-u"}
SHELLS = {"bash", "sh", "zsh", "dash"}
SPLIT = re.compile(r"&&|\|\||;|\||\n|\$\(|`|\(|\)")
ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")

def strip_prefix(tokens):
    bypass = False
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if ASSIGN.match(tok):
            if tok == "MIKE_RAW_CARGO=1":
                bypass = True
            i += 1
            continue
        name = os.path.basename(tok)
        if name in WRAPPERS:
            i += 1
            while i < len(tokens) and (tokens[i].startswith("-") or ASSIGN.match(tokens[i])
                                       or re.fullmatch(r"[0-9.]+[smhd]?", tokens[i])):
                if tokens[i] == "MIKE_RAW_CARGO=1":
                    bypass = True
                if tokens[i] in WRAPPER_VALUE_OPTIONS and i + 1 < len(tokens):
                    i += 1
                i += 1
            continue
        break
    return tokens[i:], bypass

def offending(segment, depth=0):
    try:
        tokens = shlex.split(segment, comments=True)
    except ValueError:
        return None
    tokens, bypass = strip_prefix(tokens)
    if bypass or not tokens:
        return None
    name = os.path.basename(tokens[0])
    if name in SHELLS and depth < 3:
        for j, tok in enumerate(tokens[1:], 1):
            if tok == "-c" or (tok.startswith("-") and not tok.startswith("--") and "c" in tok):
                if j + 1 < len(tokens):
                    return scan(tokens[j + 1], depth + 1)
                return None
        return None
    if name == "cargo-nextest":
        return "cargo-nextest"
    if name != "cargo":
        return None
    for tok in tokens[1:]:
        if tok.startswith("+") or tok.startswith("-"):
            continue
        return f"cargo {tok}" if tok in HEAVY else None
    return None

def scan(command, depth=0):
    for segment in SPLIT.split(command):
        hit = offending(segment.strip(), depth)
        if hit:
            return hit
    return None

def main():
    try:
        event = json.load(sys.stdin)
    except Exception:
        return 0
    if not isinstance(event, dict) or event.get("tool_name") != "Bash":
        return 0
    command = (event.get("tool_input") or {}).get("command")
    if not isinstance(command, str):
        return 0
    hit = scan(command)
    if not hit:
        return 0
    sys.stderr.write(
        f"raw {hit} refused: build and test through scripts/cx (scripts/cx check|test|build|nextest ...) "
        "or make (make test-t t=<filter>), which use the shared mbx cache, half the cores and one heavy "
        "build at a time. Deliberate exception: prefix the command with MIKE_RAW_CARGO=1.\n")
    return 2

try:
    sys.exit(main())
except SystemExit:
    raise
except Exception:
    sys.exit(0)
'
