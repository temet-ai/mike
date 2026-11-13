#!/usr/bin/env python3
"""The text each fleet seat reads: words per seat, and instructions a seat reads twice.

Each seat reads the fleet rules, its own role file, the project's
.claude/fleet/project.md and the brief mike writes for it. The brief's fixed
text lives in the Rust functions named in SEATS; placeholders such as
{slug} are filled at launch and are not counted.

  fleet_text.py words      print each source and each seat's total; last line "total N"
  fleet_text.py overlap    print every pair of sentences one seat reads that share
                           a run of six words; exit 1 when a pair is not in
                           fleet-text-overlap.allow
"""
import itertools
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
PLAN = "crates/mike-launch/src/launch/fleet/plan.rs"
PLAN_BRIEFS = "crates/mike-launch/src/launch/fleet/plan_briefs.rs"
ALLOW = "scripts/rule-checks/fleet-text-overlap.allow"
RULES = ".claude/fleet/rules.md"
PROJECT = ".claude/fleet/project.md"

BRIEF_SHARED = [(PLAN, "facts_block"), (PLAN, "facts_block_reading")]
SEATS = {
    "author": {"files": [".claude/fleet/fleet-command.md"], "brief": []},
    "worker": {
        "files": [".claude/skills/fleet-worker/SKILL.md", PROJECT],
        "brief": [
            (PLAN_BRIEFS, "worker_prompt"),
            (PLAN, "facts_block_reading"),
            (PLAN_BRIEFS, "read_the_fleet"),
            (PLAN_BRIEFS, "check_instructions"),
            (PLAN_BRIEFS, "reference_line"),
        ],
    },
}
SHINGLE = 6
SHARED_FRACTION = 0.3


def read(path):
    full = os.path.join(ROOT, path)
    if not os.path.isfile(full):
        return ""
    with open(full, encoding="utf-8") as f:
        return f.read()


def rust_literals(src):
    out = []
    for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', src):
        s = m.group(1)
        s = re.sub(r"\\\n\s*", "", s)
        s = s.replace("\\n", "\n").replace('\\"', '"')
        out.append(s)
    return out


def consts(src):
    found = {}
    for m in re.finditer(r'const ([A-Z_]+): &str =\s*"((?:[^"\\]|\\.)*)";', src):
        found[m.group(1)] = rust_literals('"' + m.group(2) + '"')[0]
    return found


def fn_body(src, name):
    m = re.search(r"fn " + re.escape(name) + r"\s*\(", src)
    if not m:
        sys.exit(f"fleet_text.py: no fn {name}; update SEATS in {__file__}")
    i = src.index("{", src.index(")", m.end()))
    depth = 0
    for j in range(i, len(src)):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[i : j + 1]
    return src[i:]


def brief_text(parts):
    known = {}
    for path in {PLAN, PLAN_BRIEFS}:
        known.update(consts(read(path)))
    texts = []
    for path, name in parts:
        for lit in rust_literals(fn_body(read(path), name)):
            lit = re.sub(r"\{([A-Z_]+)\}", lambda m: known.get(m.group(1), ""), lit)
            lit = re.sub(r"\{[a-z_0-9:?]*\}", " ", lit)
            texts.append(lit)
    return "\n".join(texts)


def markdown(path):
    return re.sub(r"\A---\n.*?\n---\n", "", read(path), flags=re.S)


def words(text):
    return len(text.split())


def seat_sources(seat):
    spec = SEATS[seat]
    sources = [(RULES, markdown(RULES))]
    sources += [(f, markdown(f)) for f in spec["files"]]
    if spec["brief"]:
        sources.append((f"brief:{seat}", brief_text(spec["brief"])))
    return sources


def cmd_words():
    counted = {}
    for seat in SEATS:
        for name, text in seat_sources(seat):
            counted[name] = words(text)
    print(f"{'SOURCE':<46} {'WORDS':>7}")
    for name, n in counted.items():
        print(f"{name:<46} {n:>7}")
    print()
    print(f"{'SEAT':<12} {'READS':>7}")
    for seat in SEATS:
        print(f"{seat:<12} {sum(words(t) for _, t in seat_sources(seat)):>7}")
    print(f"total {sum(counted.values())}")


def sentences(name, text):
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        for s in re.split(r"(?<=[.;:])\s+", line):
            toks = re.findall(r"[a-z0-9_`'-]+", s.lower())
            if len(toks) >= SHINGLE:
                grams = {tuple(toks[i : i + SHINGLE]) for i in range(len(toks) - SHINGLE + 1)}
                out.append((name, n, s.strip(), grams))
    return out


def cmd_overlap():
    allowed = {
        line.split("#")[0].strip()
        for line in read(ALLOW).splitlines()
        if line.split("#")[0].strip()
    }
    seen, bad = set(), 0
    for seat in SEATS:
        units = [u for name, text in seat_sources(seat) for u in sentences(name, text)]
        for a, b in itertools.combinations(units, 2):
            if a[0] == b[0] and a[1] == b[1]:
                continue
            shared = a[3] & b[3]
            if not shared or len(shared) / min(len(a[3]), len(b[3])) < SHARED_FRACTION:
                continue
            key = " <-> ".join(sorted([f"{a[0]}:{a[2][:60].strip()}", f"{b[0]}:{b[2][:60].strip()}"]))
            if key in seen:
                continue
            seen.add(key)
            mark = "allowed" if key in allowed else "REPEATED"
            bad += mark == "REPEATED"
            print(f"{mark} ({seat})\n  {a[0]}:{a[1]}: {a[2][:140]}\n  {b[0]}:{b[1]}: {b[2][:140]}")
            if mark == "REPEATED":
                print(f"  allow-line: {key}")
    if bad:
        print(f"\nFAIL - {bad} instruction(s) a seat reads twice. Say it once, in the file of the seat that acts;")
        print(f"       a repeat that must stay goes in {ALLOW} with the reason, after a #.")
        return 1
    print(f"PASS - no instruction read twice by one seat ({len(seen)} allowed)")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("words", "overlap"):
        sys.exit(__doc__)
    sys.exit(cmd_overlap() if sys.argv[1] == "overlap" else cmd_words())
