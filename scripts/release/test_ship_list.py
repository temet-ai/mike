"""Rust that names a scripts/ or .claude/ path must name one the archive ships, or say why not.

An installed mike has no checkout: a path the proxy runs or reads under scripts/ works only
if script_assets.SCRIPTS packs it. Every literal that begins `scripts/` or `.claude/` in
non-test Rust is either shipped, read through include_str!/include_bytes!, or named in
ALLOWED below with its reason. A new literal fails here until one of the three is true.
"""

from pathlib import Path
import re
import unittest

import script_assets

ROOT = Path(__file__).resolve().parents[2]
LITERAL = re.compile(r'"((?:scripts|\.claude)/[^"\s{}\\]*)')
EMBEDDED = ("include_str!", "include_bytes!")

CHECKOUT_JOB = "checkout-only scheduled job: it commits into or edits this repository"
TARGET_PROJECT = "a path inside the operator's own project checkout, not mike's package"
SESSION_COMMAND = "text of a command the operator or a fleet seat runs in a checkout"

# literal (trailing slash stripped) -> why the archive does not need to ship it
ALLOWED = {
    "scripts/lessons-commit": CHECKOUT_JOB,
    "scripts/memory-review": CHECKOUT_JOB,
    "scripts/usage-audit.sh": "checkout-only scheduled job: reads this repository's capture database and docs",
    "scripts/six-hourly-fixture": "test fixture job; no script of that name exists",
    "scripts/gate": SESSION_COMMAND + " (a command name the fleet floor recognises in a seat's work)",
    "scripts/docs-agent": "named in a route's manifest exemption reason as the route's only client",
    "scripts": TARGET_PROJECT + " (the structural snapshot keeps extensionless files under a scanned checkout's scripts/)",
    ".claude/rules": TARGET_PROJECT + " (the rule check reads the session checkout's rule files)",
    ".claude/agents": TARGET_PROJECT + " (agent definitions)",
    ".claude/skills": TARGET_PROJECT + " (skills)",
    ".claude/commands": TARGET_PROJECT + " (slash commands)",
    ".claude/rules/response-style.md": "the operator's own ~/.claude style rules, read from $HOME, not from mike's package",
    ".claude/fleet": TARGET_PROJECT + " (fleet rules, contract and operations)",
    ".claude/settings.local.json": TARGET_PROJECT + " (session settings)",
    ".claude/fleet/worker-contract.md": TARGET_PROJECT + " (fleet worker contract)",
    ".claude/fleet/operations.json": TARGET_PROJECT + " (fleet operation profile)",
    ".claude/threads/registry.jsonl": TARGET_PROJECT + " (legacy work-thread registry import)",
    ".claude/worktrees": TARGET_PROJECT + " (worktree slots mike manages)",
    ".claude/worktrees/mike-gate-": TARGET_PROJECT + " (gate output logs in a worktree slot)",
    ".claude/worktrees/mike-gate-output.log": TARGET_PROJECT + " (gate output log in a worktree slot)",
    ".claude/worktrees/start-baseline.lock": TARGET_PROJECT + " (fleet start lock)",
    ".claude": TARGET_PROJECT + " (also the lesson-proposer's .claude/<folder>/<name> display path)",
}


def is_test_file(path):
    name = path.name
    return ("tests" in path.parts or "benches" in path.parts or name == "tests.rs"
            or name.endswith(("_tests.rs", "_test.rs")) or name.startswith("test_"))


def production_text(path):
    """The file up to its #[cfg(test)] module, which sits at the end by convention.

    A #[cfg(test)] on any other item (a test-only const) is skipped line by line, not the file.
    """
    source = path.read_text(errors="replace").splitlines()
    lines = []
    for number, line in enumerate(source):
        if line.strip().startswith("#[cfg(test)]"):
            following = source[number + 1].strip() if number + 1 < len(source) else ""
            if re.match(r"(pub(\([a-z]+\))? )?mod ", following):
                break
        lines.append(line)
    return lines


def literals():
    found = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        if is_test_file(path.relative_to(ROOT)) or "target" in path.parts:
            continue
        for number, line in enumerate(production_text(path), 1):
            code = line.strip()
            if code.startswith("//") or any(macro in line for macro in EMBEDDED):
                continue
            for match in LITERAL.finditer(line):
                found.append((match.group(1).rstrip("/"), f"{path.relative_to(ROOT)}:{number}"))
    return found


class ShipListTests(unittest.TestCase):
    def test_every_script_or_claude_path_in_rust_is_shipped_or_allowed(self):
        unlisted = [f"{literal} at {where}" for literal, where in literals()
                    if literal not in ALLOWED and not script_assets.shipped(literal)]
        self.assertEqual(unlisted, [], "ship it in scripts/release/script_assets.py SCRIPTS, read it with "
                         "include_str!, or add it to ALLOWED in test_ship_list.py with a reason")

    def test_allow_list_names_no_shipped_path_and_no_stale_entry(self):
        seen = {literal for literal, _ in literals()}
        shipped = sorted(entry for entry in ALLOWED if script_assets.shipped(entry))
        stale = sorted(entry for entry in ALLOWED if entry not in seen)
        self.assertEqual(shipped, [], "allow-listed paths that the archive ships; remove them")
        self.assertEqual(stale, [], "allow-list entries no non-test Rust names any more; remove them")

    def test_a_scheduled_job_ships_exactly_when_it_is_not_checkout_only(self):
        jobs = (ROOT / "crates/mike/src/scheduled_jobs/jobs.rs").read_text()
        named = re.findall(r'script: "(scripts/[^"]+)",.*?checkout_only: (true|false)', jobs, re.S)
        self.assertGreaterEqual(len(named), 8, "jobs.rs no longer lists script and checkout_only per job")
        for script, checkout_only in named:
            self.assertEqual(script_assets.shipped(script), checkout_only == "false", script)
            self.assertEqual(script in ALLOWED, checkout_only == "true", script)

    def test_ship_list_names_only_tracked_non_test_files(self):
        for entry in script_assets.SCRIPTS:
            target = ROOT / entry
            self.assertTrue(target.exists(), entry)
            self.assertFalse(script_assets.is_test(entry), entry)


if __name__ == "__main__":
    unittest.main()
