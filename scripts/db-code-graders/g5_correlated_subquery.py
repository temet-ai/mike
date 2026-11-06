#!/usr/bin/env python3
"""Correlated-scalar-subquery detector — the executable form of the first clause
of .claude/rules/rust-database-reliability.md.

Banned: a scalar `(SELECT …)` in an expression position that references the outer
row, because it re-runs once per row. Explicitly ALLOWED by the same rule, and
therefore never reported here: `EXISTS`/`NOT EXISTS` semi- and anti-joins,
`IN (SELECT …)`, uncorrelated subqueries, derived tables (`FROM (SELECT …)`),
and `LATERAL`.

The regex this replaces ran with DOTALL over whole files, so it matched a
`SELECT` in one string literal against a `(SELECT … WHERE a = b.c)` in a
different one further down. Every one of its 10 standing "blocking" hits was a
false positive of that kind, which is how a rule with a real bug class behind it
came to be background noise. So the detector works on one SQL literal at a time
and decides two things structurally:

  position    — the token before `(` says whether the subquery is a scalar
                expression or one of the allowed shapes.
  correlation — a qualified reference `x.col` inside the subquery is an OUTER
                reference when `x` is bound nowhere inside it. Aliases bound at
                any nesting depth count as inner, so ambiguity is resolved
                toward NOT reporting: this gate blocks commits.

A `grader-allow: <reason>` comment within the literal, or on one of the 3 lines
above it, suppresses a hit — same marker G4 honours.

Usage:  g5_correlated_subquery.py <file.rs|file.sql> ...
        g5_correlated_subquery.py --self-test
Exit:   0 = clean, 1 = correlated scalar subquery found, 2 = usage error.
"""

import re
import sys

# Tokens that make a following `(SELECT …)` one of the allowed shapes rather
# than a scalar expression. `,` and `(` cover FROM-lists and nested grouping.
ALLOWED_BEFORE = {
    "exists", "in", "any", "all", "some",
    "from", "join", "lateral", "using",
    "union", "intersect", "except", "as", "with",
}

ALIAS_STOP = {
    "on", "using", "where", "group", "order", "limit", "offset", "having",
    "inner", "left", "right", "full", "cross", "join", "lateral", "and", "or",
    "union", "intersect", "except", "window", "fetch", "for", "as", "select",
}

WORD = r"[A-Za-z_][A-Za-z0-9_]*"


def sql_literals(text):
    """Yield (offset, literal) for Rust string literals that look like SQL.

    Raw strings first: `r#"…"#` is how most multi-line SQL is written here, and
    a naive `"`-scanner would end such a literal at the first inner quote.
    """
    spans = []
    for m in re.finditer(r'r(#+)"', text):
        hashes = m.group(1)
        end = text.find('"' + hashes, m.end())
        if end == -1:
            continue
        spans.append((m.end(), text[m.end():end]))
    covered = [(s, s + len(b)) for s, b in spans]

    for m in re.finditer(r'(?<!r)"((?:[^"\\]|\\.)*)"', text, re.DOTALL):
        if any(a <= m.start() < b for a, b in covered):
            continue
        spans.append((m.start(1), m.group(1)))

    for off, body in spans:
        if re.search(r"\bSELECT\b", body, re.IGNORECASE):
            yield off, body


def balanced(sql, open_idx):
    """Return the text inside the parentheses opening at open_idx, or None."""
    depth = 0
    for i in range(open_idx, len(sql)):
        c = sql[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return sql[open_idx + 1:i]
    return None


def preceding_token(sql, idx):
    """The word immediately before position idx, or the punctuation there."""
    j = idx - 1
    while j >= 0 and sql[j] in " \t\r\n":
        j -= 1
    if j < 0:
        return ""
    if not (sql[j].isalnum() or sql[j] == "_"):
        return sql[j]
    k = j
    while k >= 0 and (sql[k].isalnum() or sql[k] == "_"):
        k -= 1
    return sql[k + 1:j + 1].lower()


def bound_names(sub):
    """Every name a qualified reference inside `sub` could legally resolve to.

    Collected at all nesting depths on purpose — a qualifier bound in a deeper
    subquery is not an outer-row reference, and treating it as one would make
    this gate block correct code.
    """
    names = set()
    for m in re.finditer(
        r"\b(?:FROM|JOIN)\s+((?:%s\.)?%s)(?:\s+(?:AS\s+)?(%s))?" % (WORD, WORD, WORD),
        sub, re.IGNORECASE,
    ):
        table, alias = m.group(1), m.group(2)
        # Both halves of `schema.table`: an unaliased `FROM tools.uses` is
        # referenced as `tools.uses`, so the schema is a legal qualifier here
        # and must not read as an outer-row reference.
        for part in table.lower().split("."):
            names.add(part)
        if alias and alias.lower() not in ALIAS_STOP:
            names.add(alias.lower())
    # CTE names, and `WITH ORDINALITY AS u(...)` / `UNNEST(...) AS u(...)`.
    for m in re.finditer(r"\b(?:WITH|,)\s+(%s)\s+AS\s*\(" % WORD, sub, re.IGNORECASE):
        names.add(m.group(1).lower())
    for m in re.finditer(r"\bAS\s+(%s)\s*\(" % WORD, sub, re.IGNORECASE):
        names.add(m.group(1).lower())
    return names


def outer_refs(sub):
    """Qualifiers used inside `sub` that nothing inside `sub` binds."""
    bound = bound_names(sub)
    refs = set()
    for m in re.finditer(r"\b(%s)\.%s" % (WORD, WORD), sub):
        q = m.group(1).lower()
        if q in bound:
            continue
        refs.add(m.group(0))
    return tuple(sorted(refs))


def scan_sql(sql):
    """Yield (offset_in_sql, snippet, outer_refs) for each violation."""
    for m in re.finditer(r"\(\s*SELECT\b", sql, re.IGNORECASE):
        if preceding_token(sql, m.start()) in ALLOWED_BEFORE:
            continue
        sub = balanced(sql, m.start())
        if sub is None:
            continue
        refs = outer_refs(sub)
        if refs:
            snippet = " ".join(sub.split())[:110]
            yield m.start(), snippet, refs


class Unreadable(Exception):
    """A path this gate was asked to grade but could not read.

    Raised rather than returning no hits: an unreadable file that reports clean
    is a gate that passes by failing, which is the whole bug class this rewrite
    exists to remove.
    """


def scan_file(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        raise Unreadable(f"{path}: {exc.strerror or exc}") from exc
    lines = text.split("\n")
    hits = []
    for off, body in sql_literals(text):
        # The suppression window is the literal's own lines plus the 3 above it
        # (where the justifying comment goes) — fixed per literal, never derived
        # from the hit's line, or one hit inside a literal gets suppressed by a
        # marker a later hit in the same literal cannot see.
        lit_start = text[:off].count("\n") + 1
        lit_end = lit_start + body.count("\n")
        window = "\n".join(lines[max(0, lit_start - 4):lit_end])
        if "grader-allow" in window:
            continue
        for sub_off, snippet, refs in scan_sql(body):
            lineno = text[:off + sub_off].count("\n") + 1
            hits.append((lineno, snippet, refs))
    return sorted(set(hits))


SELF_TESTS = [
    # (should_flag, sql) — the allowed shapes come straight from the rule.
    (False, "SELECT 1 FROM requests.requests r WHERE EXISTS (SELECT 1 FROM tools.uses u WHERE u.request_id = r.id)"),
    (False, "SELECT 1 FROM messages.messages m WHERE NOT EXISTS (SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id)"),
    (False, "SELECT x FROM (SELECT id AS x FROM sessions.sessions s WHERE s.id > 1) t"),
    (False, "SELECT r.id FROM UNNEST($1::bigint[]) WITH ORDINALITY AS u(rule_id, ord) JOIN rules.rules r ON r.id = u.rule_id"),
    (False, "SELECT id FROM sessions.sessions WHERE id IN (SELECT session_id FROM requests.requests WHERE created_at > now())"),
    (False, "SELECT s.id, l.n FROM sessions.sessions s LEFT JOIN LATERAL (SELECT count(*) AS n FROM requests.requests r WHERE r.session_id = s.id) l ON true"),
    (False, "SELECT (SELECT count(*) FROM tools.uses) AS total FROM sessions.sessions"),
    (True, "SELECT s.id, (SELECT count(*) FROM requests.requests r WHERE r.session_id = s.id) AS n FROM sessions.sessions s"),
    (True, "SELECT COALESCE((SELECT max(x.created_at) FROM tools.uses x WHERE x.session_id = s.id), now()) FROM sessions.sessions s"),
]


def self_test():
    bad = 0
    for expect, sql in SELF_TESTS:
        got = bool(list(scan_sql(sql)))
        status = "ok" if got == expect else "MISMATCH"
        if got != expect:
            bad += 1
        print(f"  {status:9} expect_flag={expect!s:5} got={got!s:5} {' '.join(sql.split())[:88]}")
    print(f"  self-test: {len(SELF_TESTS) - bad}/{len(SELF_TESTS)} correct")
    return 1 if bad else 0


def main(argv):
    if not argv:
        print(__doc__.strip().split("Usage:")[-1], file=sys.stderr)
        return 2
    if argv[0] == "--self-test":
        return self_test()
    total = 0
    unreadable = 0
    for path in argv:
        try:
            hits = scan_file(path)
        except Unreadable as exc:
            print(f"  ERROR: cannot read {exc}", file=sys.stderr)
            unreadable += 1
            continue
        if hits:
            print(f"  FAIL: {path}")
            for lineno, snippet, refs in hits:
                print(f"    L{lineno}: correlated on {', '.join(refs)} — {snippet}")
            total += len(hits)
    if unreadable:
        print(f"  ERROR: {unreadable} path(s) unreadable — refusing to report clean", file=sys.stderr)
        return 2
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
