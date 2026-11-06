#!/usr/bin/env python3
"""G1 — INSERT completeness grader.

Catches the 20260526 bug class: a migration adds a NOT NULL column to a table,
and one or more INSERT sites in Rust code don't set it.

Handles both migration patterns:
  Pattern A: ADD COLUMN col_name TYPE NOT NULL  (inline constraint)
  Pattern B: ADD COLUMN col_name TYPE NULL  →  ALTER COLUMN col SET NOT NULL
             (two-step: nullable for backfill, then tightened)

Usage:
  python3 g1_insert_completeness.py [--diff | --full] [--migration path.sql]

Exit: 0=PASS, 1=FAIL, 2=SKIP
"""

import re
import subprocess
import sys
from pathlib import Path

# ── helpers ───────────────────────────────────────────────────────────────────

def git_root() -> Path:
    r = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                       capture_output=True, text=True)
    return Path(r.stdout.strip()) if r.returncode == 0 else Path(".")

ROOT = git_root()
MIGRATIONS_DIR = ROOT / "crates/mike/src/store/persistence/migrations"

def find_notnull_additions(migration_path: Path) -> list[tuple[str, str]]:
    """Return list of (table, column) pairs that become NOT NULL in this migration,
    excluding columns that also get a DEFAULT (auto-populated via sequence/function)."""
    text = migration_path.read_text()
    results = []

    # Track which columns gain a DEFAULT in this migration (sequence, etc.)
    # These don't require explicit INSERT binding — Postgres fills them in.
    columns_with_default: set[str] = set()
    for line in text.splitlines():
        m = re.match(r'\s*ALTER\s+COLUMN\s+(\w+)\s+SET\s+DEFAULT\s+', line.strip(), re.I)
        if m:
            columns_with_default.add(m.group(1).lower())
        # Also catch: col TYPE DEFAULT <expr> in ADD COLUMN inline
        m = re.match(r'.*ADD\s+COLUMN(?:\s+IF\s+NOT\s+EXISTS)?\s+(\w+)\s+.*\bDEFAULT\b', line, re.I)
        if m:
            columns_with_default.add(m.group(1).lower())

    current_table = None
    for line in text.splitlines():
        line_stripped = line.strip()

        m = re.match(r'ALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?((?:\w+\.)?\w+)', line_stripped, re.I)
        if m:
            current_table = m.group(1)
            if '.' not in current_table:
                current_table = f"mike.{current_table}"

        # Pattern A: ADD COLUMN col TYPE ... NOT NULL (without DEFAULT)
        m = re.match(
            r'ADD\s+COLUMN(?:\s+IF\s+NOT\s+EXISTS)?\s+(\w+)\s+\S+.*?NOT\s+NULL',
            line_stripped, re.I
        )
        if m and not re.search(r'\bDEFAULT\b', line_stripped, re.I):
            col = m.group(1).lower()
            if current_table and col not in columns_with_default:
                results.append((current_table, col))

        # Pattern B: ALTER COLUMN col SET NOT NULL  (two-step backfill pattern)
        m = re.match(r'ALTER\s+COLUMN\s+(\w+)\s+SET\s+NOT\s+NULL', line_stripped, re.I)
        if m:
            col = m.group(1).lower()
            if current_table and col not in columns_with_default:
                results.append((current_table, col))

    return list(dict.fromkeys(results))

def check_insert_site(file_path: Path, lineno: int, column: str) -> bool:
    """Check whether an INSERT at `lineno` in `file_path` sets `column`."""
    lines = file_path.read_text().splitlines()
    window = "\n".join(lines[lineno - 1: lineno + 20])
    # Column name must appear as a word boundary in the INSERT block
    return bool(re.search(rf'\b{re.escape(column)}\b', window, re.I))

def find_insert_sites(rs_path: Path, short_table: str) -> list[int]:
    """Return line numbers (1-based) of INSERT INTO <table> in a Rust file."""
    text = rs_path.read_text()
    pattern = re.compile(
        rf'INSERT\s+INTO\s+(?:mike\.|claude_proxy\.)?{re.escape(short_table)}\b',
        re.I | re.MULTILINE
    )
    return [text[:m.start()].count('\n') + 1 for m in pattern.finditer(text)]


# ── main ──────────────────────────────────────────────────────────────────────

def main():
    mode = "--full"
    migration_override = None
    i = 1
    while i < len(sys.argv):
        arg = sys.argv[i]
        if arg in ("--diff", "--full"):
            mode = arg
        elif arg == "--migration" and i + 1 < len(sys.argv):
            migration_override = Path(sys.argv[i + 1])
            i += 1
        i += 1

    # Which Rust files to scan
    if mode == "--diff":
        r = subprocess.run(
            ["git", "diff", "--name-only", "HEAD", "--", "*.rs"],
            capture_output=True, text=True, cwd=ROOT
        )
        rs_files = [ROOT / p for p in r.stdout.splitlines() if p.endswith(".rs")]
    else:
        rs_files = [
            p for p in ROOT.rglob("*.rs")
            if "target" not in p.parts
            and "tests" not in p.parts
            and ".claude" not in p.parts  # exclude worktrees / scratch
        ]

    # Which migrations to scan
    if migration_override:
        migrations = [migration_override]
    elif mode == "--diff":
        r = subprocess.run(
            ["git", "diff", "--name-only", "HEAD", "--", "*.sql"],
            capture_output=True, text=True, cwd=ROOT
        )
        staged_r = subprocess.run(
            ["git", "ls-files", "--others", "--exclude-standard", "--", "*.sql"],
            capture_output=True, text=True, cwd=ROOT
        )
        sql_paths = r.stdout.splitlines() + staged_r.stdout.splitlines()
        migrations = [ROOT / p for p in sql_paths if p.endswith(".sql")]
    else:
        # All migrations including untracked (they're on disk)
        migrations = sorted(MIGRATIONS_DIR.glob("*.sql")) if MIGRATIONS_DIR.exists() else []

    print("=== G1: INSERT completeness grader ===")
    print(f"Mode: {mode} | RS files: {len(rs_files)}")
    print()

    overall_fail = 0
    checked = 0

    for migration_path in migrations:
        if not migration_path.exists():
            continue
        additions = find_notnull_additions(migration_path)
        if not additions:
            continue

        print(f"Migration: {migration_path.name}")

        for table, column in additions:
            short_table = table.replace("mike.", "")
            checked += 1
            print(f"  [NOT NULL] {table}.{column}")

            missing = []
            for rs_file in rs_files:
                sites = find_insert_sites(rs_file, short_table)
                for lineno in sites:
                    if not check_insert_site(rs_file, lineno, column):
                        rel = rs_file.relative_to(ROOT)
                        missing.append(f"    FAIL: {rel}:{lineno} — INSERT missing '{column}'")

            if missing:
                for m in missing:
                    print(m)
                overall_fail += 1
            else:
                total_sites = sum(len(find_insert_sites(rs_f, short_table)) for rs_f in rs_files)
                print(f"    ok ({total_sites} INSERT site(s) all set '{column}')")
        print()

    print("─" * 40)
    if checked == 0:
        print("SKIP: no NOT NULL column additions found in scanned migrations")
        sys.exit(2)
    elif overall_fail == 0:
        print(f"PASS: all {checked} NOT NULL column addition(s) have complete INSERT coverage")
        sys.exit(0)
    else:
        print(f"FAIL: {overall_fail}/{checked} column(s) have incomplete INSERT coverage")
        sys.exit(1)

if __name__ == "__main__":
    main()
