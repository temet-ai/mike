#!/usr/bin/env python3
"""Aggregate the raw reporter files that scripts/test-timing collects.

Reads, from one directory:
  phases.jsonl      wall time of every phase the orchestrator ran
  rust-junit.xml    nextest JUnit (classname = binary id, name = test path)
  vitest-*.xml      vitest JUnit (testsuite name = file, testcase = test)
  playwright.xml    Playwright JUnit
  scripts.log       `scripts/test-suite scripts` output (per-phase seconds)

Writes timings.json, timings.csv and report.md next to them.

Categories (the four the operator asked for, plus `scripts` for the
repository's own tooling suites, which have no per-test reporter):
  unit          Rust lib/bin unit tests, vitest `unit` project
  integration   Rust tests/*.rs harnesses (proxy, cli, schema, reminder_on_token,
                and the DB-backed `integration` harness), vitest `integration`
  e2e           Rust `e2e` harness (DB + proxy fixtures), Playwright
  visual        vitest `storybook` browser project (stories, axe, screenshots)
  scripts       shell / Python / Node regression suites (phase-level only)
"""
from __future__ import annotations

import csv
import json
import re
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path

RUST_HARNESS = {
    "proxy": ("integration", "rust-proxy", "fast"),
    "cli": ("integration", "rust-cli", "fast"),
    "schema": ("integration", "rust-schema", "fast"),
    "reminder_on_token": ("integration", "rust-reminder_on_token", "fast"),
    "integration": ("integration", "rust-integration-db", "db"),
    "e2e": ("e2e", "rust-e2e-db", "db"),
}


def classify_rust(binary_id: str) -> tuple[str, str, str]:
    crate, _, harness = binary_id.partition("::")
    if not harness or harness.startswith("bin/"):
        return "unit", f"rust-unit:{crate}", "fast"
    return RUST_HARNESS.get(harness, ("integration", f"rust-{harness}", "fast"))


def classify_vitest(file: str) -> tuple[str, str, str]:
    if file.endswith(".stories.svelte"):
        return "visual", "storybook", "browser"
    if ".integration." in file:
        return "integration", "vitest-integration", "node"
    return "unit", "vitest-unit", "node"


def status_of(case: ET.Element) -> str:
    for tag, st in (("failure", "fail"), ("error", "error"), ("skipped", "skip")):
        if case.find(tag) is not None:
            return st
    return "pass"


def rows_from_junit(path: Path, family: str, classify, name_of) -> list[dict]:
    rows = []
    root = ET.parse(path).getroot()
    for suite in root.iter("testsuite"):
        for case in suite.findall("testcase"):
            key = name_of(suite, case)
            category, suite_name, tier = classify(key)
            rows.append(
                {
                    "family": family,
                    "category": category,
                    "suite": suite_name,
                    "tier": tier,
                    "group": key,
                    "name": case.get("name", ""),
                    "seconds": float(case.get("time") or 0.0),
                    "status": status_of(case),
                    "source": path.name,
                }
            )
    return rows


def load_rust(out: Path) -> list[dict]:
    p = out / "rust-junit.xml"
    if not p.exists():
        return []
    return rows_from_junit(p, "rust", classify_rust, lambda s, c: c.get("classname", s.get("name", "")))


def load_vitest(out: Path) -> list[dict]:
    rows = []
    for p in sorted(out.glob("vitest-*.xml")):
        rows += rows_from_junit(p, "frontend", classify_vitest, lambda s, c: s.get("name", "") or c.get("classname", ""))
    return rows


def load_playwright(out: Path) -> list[dict]:
    p = out / "playwright.xml"
    if not p.exists():
        return []
    return rows_from_junit(
        p, "e2e", lambda _k: ("e2e", "playwright", "browser"), lambda s, c: c.get("classname", s.get("name", ""))
    )


PHASE_RE = re.compile(r"^<== (PASS|FAIL) (.+?) \((\d+)s(?:, exit (\d+))?\)")


def load_scripts(out: Path) -> list[dict]:
    p = out / "scripts.log"
    if not p.exists():
        return []
    rows = []
    for line in p.read_text(errors="replace").splitlines():
        m = PHASE_RE.match(line)
        if not m:
            continue
        rows.append(
            {
                "family": "scripts",
                "category": "scripts",
                "suite": "scripts",
                "tier": "host",
                "group": m.group(2),
                "name": m.group(2),
                "seconds": float(m.group(3)),
                "status": "pass" if m.group(1) == "PASS" else "fail",
                "source": p.name,
            }
        )
    return rows


def load_phases(out: Path) -> list[dict]:
    p = out / "phases.jsonl"
    if not p.exists():
        return []
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()]


def fmt(s: float) -> str:
    if s >= 60:
        return f"{s / 60:.1f} min"
    if s >= 1:
        return f"{s:.2f} s"
    return f"{s * 1000:.0f} ms"


def table(headers: list[str], rows: list[list[str]]) -> str:
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join("---" for _ in headers) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


def summarise(rows: list[dict], key: str) -> list[tuple[str, int, float, float, float]]:
    agg: dict[str, list[float]] = defaultdict(list)
    for r in rows:
        agg[r[key]].append(r["seconds"])
    res = []
    for k, v in agg.items():
        v.sort()
        res.append((k, len(v), sum(v), v[len(v) // 2], v[-1]))
    return sorted(res, key=lambda t: -t[2])


def bucket(rows: list[dict]) -> list[tuple[str, int, float]]:
    edges = [(0, 0.01, "< 10 ms"), (0.01, 0.1, "10–100 ms"), (0.1, 1, "100 ms – 1 s"), (1, 5, "1–5 s"), (5, 30, "5–30 s"), (30, 1e9, "> 30 s")]
    res = []
    for lo, hi, label in edges:
        sel = [r["seconds"] for r in rows if lo <= r["seconds"] < hi]
        res.append((label, len(sel), sum(sel)))
    return res


def module_of(name: str) -> str:
    parts = name.split("::")
    depth = 2 if parts[0] in ("store", "launch", "config", "steerer", "proxy", "api") else 1
    return "::".join(parts[:depth])


def rust_lib_by_module(rows: list[dict]) -> str:
    lib = [dict(r, group=module_of(r["name"])) for r in rows if r["suite"].startswith("rust-unit:") and "::" not in r["group"]]
    if not lib:
        return ""
    total = sum(r["seconds"] for r in lib)
    body = []
    for k, n, s, med, mx in summarise(lib, "group")[:30]:
        slow = [r["seconds"] for r in lib if r["group"] == k and r["seconds"] >= 1]
        body.append([k, n, fmt(s), f"{100 * s / total:.1f}%", fmt(med), fmt(mx), len(slow), fmt(sum(slow))])
    return "\n".join([
        "## unit: Rust lib tests by module",
        "",
        "A lib test that takes a second or more is almost always a DB fixture, a network timeout or a sleep, "
        "not a unit test; the last two columns count those.",
        "",
        table(["module", "tests", "sum", "share", "median", "max", "tests ≥ 1 s", "their sum"], body),
        "",
    ])


def family_rows(tests: list[dict], phases: list[dict]) -> list[list[str]]:
    fam_of = {"storybook": "storybook", "vitest unit": "vitest-unit", "vitest integration": "vitest-integration"}
    out = []
    for p in phases:
        label = p["phase"]
        if p["family"] == "rust" and "nextest" in label:
            body = sum(r["seconds"] for r in tests if r["family"] == "rust")
        elif p["family"] == "storybook":
            body = sum(r["seconds"] for r in tests if r["suite"] == "storybook")
        elif label in fam_of:
            body = sum(r["seconds"] for r in tests if r["suite"] == fam_of[label])
        elif p["family"] == "e2e":
            body = sum(r["seconds"] for r in tests if r["suite"] == "playwright")
        else:
            out.append([f"{p['family']}: {label}", fmt(p["seconds"]), "none", "-", "compile or typecheck, no tests"])
            continue
        wall = p["seconds"]
        if body == 0:
            reading = "no test ran; the whole phase is overhead"
        elif body > wall:
            reading = f"parallel: {body / wall:.1f}x more test time than wall"
        else:
            reading = f"serial: {fmt(wall - body)} of start-up and setup overhead"
        out.append([f"{p['family']}: {label}", fmt(wall), fmt(body), f"{body / wall:.2f}" if wall else "-", reading])
    return out


def write_report(out: Path, rows: list[dict], phases: list[dict]) -> None:
    run = (out / "run.txt").read_text() if (out / "run.txt").exists() else ""
    tests = [r for r in rows if r["category"] != "scripts"]
    total_test = sum(r["seconds"] for r in tests)
    total_wall = sum(p["seconds"] for p in phases)
    lines = [f"# Test timing report — {out.parent.name}", "", "```", run.strip(), "```", ""]
    lines += [
        "Per-test seconds are what each reporter recorded for the test body alone. "
        "Phase seconds are wall time around the whole runner, so the gap between the "
        "two is compile, collection, browser start-up, fixture and reporter overhead.",
        "",
        "## Phases (wall time)",
        "",
        table(
            ["family", "phase", "wall", "exit"],
            [[p["family"], p["phase"], fmt(p["seconds"]), p["exit"]] for p in phases],
        ),
        "",
        f"**Wall clock, what you wait for: {fmt(total_wall)}** across {len(phases)} phases.",
        "",
        f"**Test-body time, what the tests themselves cost: {fmt(total_test)}** over {len(tests)} tests. "
        "This is larger than the wall clock because nextest runs tests in parallel; it is the number to use "
        "for ranking tests, never for predicting how long a gate takes.",
        "",
        "## Wall clock against test-body time, per family",
        "",
        table(
            ["family", "wall", "test-body sum", "ratio", "reading"],
            family_rows(tests, phases),
        ),
        "",
        "## By category",
        "",
        table(
            ["category", "tests", "sum of test time", "share", "median", "max"],
            [
                [k, n, fmt(s), f"{100 * s / total_test:.1f}%" if total_test else "-", fmt(med), fmt(mx)]
                for k, n, s, med, mx in summarise(tests, "category")
            ],
        ),
        "",
        "## By suite",
        "",
        table(
            ["suite", "category", "tests", "sum", "median", "max"],
            [
                [k, next(r["category"] for r in tests if r["suite"] == k), n, fmt(s), fmt(med), fmt(mx)]
                for k, n, s, med, mx in summarise(tests, "suite")
            ],
        ),
        "",
        "## Distribution",
        "",
        table(["bucket", "tests", "sum"], [[l, n, fmt(s)] for l, n, s in bucket(tests)]),
        "",
    ]
    lines += [rust_lib_by_module(tests)]
    for cat in ("unit", "integration", "e2e", "visual"):
        sel = [r for r in tests if r["category"] == cat]
        if not sel:
            continue
        cat_total = sum(r["seconds"] for r in sel)
        lines += [f"## {cat}: slowest groups", ""]
        lines += [
            table(
                ["group (binary / file)", "tests", "sum", "share of category", "max"],
                [
                    [k, n, fmt(s), f"{100 * s / cat_total:.1f}%" if cat_total else "-", fmt(mx)]
                    for k, n, s, _m, mx in summarise(sel, "group")[:25]
                ],
            ),
            "",
            f"## {cat}: slowest tests",
            "",
            table(
                ["seconds", "status", "group", "test"],
                [
                    [f"{r['seconds']:.3f}", r["status"], r["group"], r["name"]]
                    for r in sorted(sel, key=lambda r: -r["seconds"])[:40]
                ],
            ),
            "",
        ]
    scripts = [r for r in rows if r["category"] == "scripts"]
    if scripts:
        lines += [
            "## scripts (phase-level)",
            "",
            table(["suite", "wall", "status"], [[r["name"], fmt(r["seconds"]), r["status"]] for r in scripts]),
            "",
        ]
    failed = [r for r in rows if r["status"] in ("fail", "error")]
    if failed:
        lines += ["## Not passing during this run", ""]
        lines += table(["status", "group", "test"], [[r["status"], r["group"], r["name"]] for r in failed]).splitlines()
        lines += [""]
    (out / "report.md").write_text("\n".join(lines))


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: test-timing-report.py OUT_DIR", file=sys.stderr)
        return 2
    out = Path(argv[1])
    rows = load_rust(out) + load_vitest(out) + load_playwright(out) + load_scripts(out)
    phases = load_phases(out)
    if not rows and not phases:
        print(f"test-timing-report: nothing to aggregate in {out}", file=sys.stderr)
        return 1
    (out / "timings.json").write_text(json.dumps({"phases": phases, "tests": rows}, indent=1))
    with (out / "timings.csv").open("w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=["family", "category", "suite", "tier", "group", "name", "seconds", "status", "source"])
        w.writeheader()
        w.writerows(rows)
    write_report(out, rows, phases)
    by_cat = summarise([r for r in rows if r["category"] != "scripts"], "category")
    for k, n, s, _med, _mx in by_cat:
        print(f"  {k:12} {n:6} tests  {fmt(s):>10}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
