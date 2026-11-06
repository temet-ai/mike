#!/usr/bin/env python3
"""Row-by-row INSERT detector (audit item H3).

Flags an INSERT that executes once per iteration of a Rust loop, where a single
batched `INSERT … SELECT FROM UNNEST(…)` should run instead.

The heuristic this replaces flagged any `for` within 20 lines of an `INSERT`,
which matched the canonical batch shape — a loop that fills Vecs followed by one
UNNEST insert — i.e. it reported the recommended fix as the bug. All four of its
standing hits were of that kind or test-fixture seeding. So this decides
structurally:

  in-loop    — the INSERT sits inside the loop's braces, counted after string
               and comment text is stripped (a format string like `{i:064x}`
               otherwise unbalances the count).
  batched    — an INSERT whose SQL uses UNNEST is the batch form, never the bug.
  test code  — a colocated `#[cfg(test)]` module is excluded, matching
               g4_raw_queries.sh and the suite's own severity legend ("tests:
               acceptable, dynamic test data"). Path-excluded `tests/` dirs are
               already filtered by the caller.

`grader-allow: <reason>` on the statement suppresses a hit, same marker G4 honours.

Usage:  g5_row_by_row_insert.py <file.rs> ...
        g5_row_by_row_insert.py --self-test
Exit:   0 = clean, 1 = row-by-row INSERT found, 2 = usage error / unreadable input.
"""

import re
import sys

FOR_LOOP = re.compile(r"\bfor\s+[A-Za-z_(&][^;]*\bin\b")
LOOP_SQL_WINDOW = 20
# Lookback for the grader-allow marker. Wide enough that the justification can be
# a readable comment block above the call rather than a 200-column inline tail.
MARKER_LOOKBACK = 8


def strip_noise(line):
    """Blank out string/char literal bodies and line comments.

    Only the brace balance matters downstream, so blanking keeps every other
    column aligned while removing braces that live inside text.
    """
    out, i, n = [], 0, len(line)
    while i < n:
        if line.startswith("//", i):
            out.append(" " * (n - i))   # pad, never truncate: positions must stay aligned
            break
        if line.startswith('r#"', i):
            end = line.find('"#', i + 3)
            if end == -1:
                return "".join(out)
            out.append(" " * (end + 2 - i))
            i = end + 2
            continue
        if line[i] == '"':
            j = i + 1
            while j < n:
                if line[j] == "\\":
                    j += 2
                    continue
                if line[j] == '"':
                    break
                j += 1
            out.append(" " * (min(j, n - 1) + 1 - i))
            i = j + 1
            continue
        out.append(line[i])
        i += 1
    return "".join(out)


def test_module_lines(lines):
    """Line indices (0-based) belonging to a `#[cfg(test)] mod … { … }` block."""
    excluded = set()
    depth = 0
    in_mod = False
    pending = False
    for idx, raw in enumerate(lines):
        line = strip_noise(raw)
        if not in_mod:
            if "#[cfg(test)]" in raw:
                pending = True
                continue
            if pending and re.search(r"\bmod\b", line):
                in_mod = True
                depth = line.count("{") - line.count("}")
                excluded.add(idx)
                pending = False
                if depth <= 0 and "{" in line:
                    in_mod = False
                continue
            if pending and line.strip():
                pending = False
            continue
        excluded.add(idx)
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            in_mod = False
    return excluded


def scan_text(text):
    """Yield (lineno, detail) for each row-by-row INSERT.

    Walks brace-by-brace within each line rather than netting a line's counts:
    `fn a(){ for r in rows { … } }` opens and closes at three different depths on
    one line, and a per-line net can neither record the loop's true entry depth
    nor tell whether a statement sits before or after the closing brace.
    """
    lines = text.split("\n")
    skip = test_module_lines(lines)
    hits = []
    loops = []  # (start_lineno, depth_outside_the_loop_body)
    depth = 0
    for idx, raw in enumerate(lines):
        line = strip_noise(raw)
        # `for … in …` positions still awaiting their opening brace on this line.
        pending = [m.end() for m in FOR_LOOP.finditer(line)]
        insert_at = raw.find("INSERT INTO")
        reported = False
        for pos, ch in enumerate(line):
            if insert_at != -1 and not reported and pos == insert_at:
                if loops and idx not in skip:
                    window = "\n".join(lines[max(0, idx - MARKER_LOOKBACK):idx + LOOP_SQL_WINDOW])
                    if "UNNEST" not in window.upper() and "grader-allow" not in window:
                        hits.append((idx + 1,
                                     f"INSERT inside the for-loop opened at line {loops[-1][0]}"))
                reported = True
            if ch == "{":
                if pending and pending[0] <= pos:
                    pending.pop(0)
                    loops.append((idx + 1, depth))
                depth += 1
            elif ch == "}":
                depth -= 1
                while loops and depth <= loops[-1][1]:
                    loops.pop()
        # An INSERT past the last brace on the line still needs its check.
        if insert_at != -1 and not reported and loops and idx not in skip:
            window = "\n".join(lines[max(0, idx - MARKER_LOOKBACK):idx + LOOP_SQL_WINDOW])
            if "UNNEST" not in window.upper() and "grader-allow" not in window:
                hits.append((idx + 1, f"INSERT inside the for-loop opened at line {loops[-1][0]}"))
    return hits


def scan_file(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        return scan_text(fh.read())


SELF_TESTS = [
    (False, 'fn a(){ for t in tools { ords.push(t.ord); }\n'
            '  sqlx::query!("INSERT INTO requests.tools SELECT u.x FROM UNNEST($1::int[]) u"); }'),
    (True, 'fn a(){ for r in rows {\n'
           '  sqlx::query!("INSERT INTO requests.tools (a) VALUES ($1)").bind(r); } }'),
    (False, 'fn a(){ for r in rows {\n'
            '  sqlx::query!("INSERT INTO t SELECT * FROM UNNEST($1::int[])").bind(r); } }'),
    (False, '#[cfg(test)]\nmod tests {\n fn s(){ for r in rows {\n'
            '  sqlx::query("INSERT INTO t (a) VALUES ($1)").bind(r); } }\n}'),
    (False, 'fn a(){ for r in rows {\n'
            '  sqlx::query( // grader-allow: test fixture seeding\n'
            '    "INSERT INTO t (a) VALUES ($1)").bind(r); } }'),
    # A format string's braces must not unbalance the loop scope.
    (False, 'fn a(){ for i in 0..n { let s = format!("{i:064x}"); v.push(s); }\n'
            '  sqlx::query!("INSERT INTO t SELECT * FROM UNNEST($1::text[])"); }'),
]


def self_test():
    bad = 0
    for expect, src in SELF_TESTS:
        got = bool(scan_text(src))
        if got != expect:
            bad += 1
        print(f"  {'ok' if got == expect else 'MISMATCH':9} expect_flag={expect!s:5} got={got!s:5} "
              f"{' '.join(src.split())[:80]}")
    print(f"  self-test: {len(SELF_TESTS) - bad}/{len(SELF_TESTS)} correct")
    return 1 if bad else 0


def main(argv):
    if not argv:
        print("usage: g5_row_by_row_insert.py <file.rs> ... | --self-test", file=sys.stderr)
        return 2
    if argv[0] == "--self-test":
        return self_test()
    total, unreadable = 0, 0
    for path in argv:
        try:
            hits = scan_file(path)
        except OSError as exc:
            print(f"  ERROR: cannot read {path}: {exc}", file=sys.stderr)
            unreadable += 1
            continue
        if hits:
            print(f"  FAIL: {path}")
            for lineno, detail in hits:
                print(f"    L{lineno}: {detail}")
            total += len(hits)
    if unreadable:
        print(f"  ERROR: {unreadable} path(s) unreadable — refusing to report clean", file=sys.stderr)
        return 2
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
