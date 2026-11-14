"""The scripts an installed mike runs, packed under share/mike/scripts/.

SCRIPTS is the one ship list. An entry is a tracked file ("scripts/learning-promote") or a
directory ending in "/" (every tracked file under it except tests). The archive, the
candidate verifier and the Rust ship-list check (test_ship_list.py) all read it.

Checkout-only jobs are deliberately absent: lessons-commit, memory-review, usage-audit.sh
and six-hourly-fixture commit into the repository, edit its threads or are test fixtures.
"""

import posixpath

from common import ReleaseError, source_git
from mod_assets import MAX_ASSET_BYTES, SCRIPTS_PREFIX, blob, is_asset

SCRIPTS = (
    # Scheduled jobs: learning-promote, learning-audit, compressor-loop, lesson-relations.
    "scripts/learning-promote",
    "scripts/learning-audit",
    "scripts/compressor-loop",
    "scripts/lesson-relations",
    # Lesson sync, and the tool-reuse runner the generated skills name.
    "scripts/mike-knowledge",
    "scripts/tool-reuse",
    # Codex hooks: install copies mike-codex-hook beside itself.
    "scripts/codex-hooks/install",
    "scripts/codex-hooks/mike-codex-hook",
    # Python packages those scripts import.
    "scripts/learning_audit/",
    "scripts/knowledge/",
    "scripts/tool_reuse/",
    "scripts/compressor_eval/commands.py",
    "scripts/compressor_eval/core.py",
    "scripts/compressor_eval/followups.py",
    "scripts/compressor_eval/qualification.py",
    "scripts/compressor_eval/trials.py",
)
TREE = "scripts/"


def is_test(path):
    parts = path.split("/")
    name = parts[-1]
    return ("tests" in parts or "__pycache__" in parts
            or (name.startswith("test_") and name.endswith(".py")))


def covers(entry, path):
    return path.startswith(entry) if entry.endswith("/") else path == entry


def shipped(path, entries=SCRIPTS):
    """Whether a repository path (scripts/...) is in the ship list."""
    return any(covers(entry, path) for entry in entries) and not is_test(path)


def archive_path(path):
    return "share/mike/" + path


def payload(root):
    """(archive path, bytes, mode) for every shipped script; empty without a scripts/ folder."""
    if not (root / "scripts").exists():
        return []
    records = source_git(root, "ls-files", "--stage", "-z", "--", "scripts")
    contents = {}
    for record in filter(None, records.split("\0")):
        metadata, path = record.split("\t", 1)
        mode, digest, stage = metadata.split()
        if not shipped(path):
            continue
        name = archive_path(path)
        if not is_asset(name) or mode not in ("100644", "100755") or stage != "0":
            raise ReleaseError("scripts must be tracked regular files without merge conflicts")
        data = blob(root, digest)
        if len(data) > MAX_ASSET_BYTES:
            raise ReleaseError("script asset exceeds package limit")
        contents[name] = (name, data, int(mode, 8) & 0o777)
    missing = [entry for entry in SCRIPTS
               if not any(covers(entry, name[len("share/mike/"):]) for name in contents)]
    if missing:
        raise ReleaseError("shipped scripts are not tracked in the source: " + ", ".join(missing))
    return [contents[name] for name in sorted(contents)]


def check_inventory(paths, entries):
    """Require the script payload a manifest recorded in a mike inventory; return the problems."""
    if (not isinstance(entries, list) or not entries
            or any(not isinstance(entry, str) or not entry.startswith(TREE) for entry in entries)):
        return ["invalid script_assets list"]
    present = {path for path in paths if path.startswith(SCRIPTS_PREFIX)}
    problems = []
    for entry in entries:
        if not any(covers(entry, path[len("share/mike/"):]) for path in present):
            problems.append("missing " + entry)
    for path in sorted(present):
        relative = path[len("share/mike/"):]
        if not shipped(relative, entries) or posixpath.normpath(relative) != relative:
            problems.append("unexpected " + path)
    return problems
