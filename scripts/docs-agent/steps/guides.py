"""Step 3: update developer guides when scripts/Makefile/tooling changes."""

import sys
from pathlib import Path
from datetime import date

sys.path.insert(0, str(Path(__file__).parent.parent))
import client
import git_util

_SYSTEM = """You are a technical writer maintaining developer guides for temet-proxy.
Update documentation when scripts, Makefile targets, or developer tooling changes.

Rules:
- Only correct instructions or command references that the diff makes wrong.
- Do not rewrite sections unrelated to the changed tooling.
- Update the `updated:` frontmatter field to today's date when you make any change.
- If no change is needed, reply with the single token NO_CHANGES and NOTHING ELSE — do not explain.
- If a change is needed, return the COMPLETE current document with your edits applied,
  never a summary or a shortened version."""

_GUIDE_DOCS = [
    (
        "docs/TROUBLESHOOTING.md",
        "Common local build and run failures, fixes for accelerators, linker, toolchain, DB.",
    ),
    (
        "docs/development/sqlx-prepare.md",
        "How to regenerate the sqlx offline cache after query! changes.",
    ),
    (
        "docs/development/mcp-from-monitor.md",
        "How to use the MCP server from the Monitor session.",
    ),
]


def run(diff: str) -> list[str]:
    tooling_diff = git_util.scope_diff_to(diff, "scripts/", "Makefile", ".cargo/")
    if not tooling_diff.strip():
        return []

    today = date.today().isoformat()
    updated: list[str] = []

    for path, description in _GUIDE_DOCS:
        content = git_util.read(path)
        if not content:
            continue

        user = f"""Document: {path}
Purpose: {description}
Today: {today}

--- current document ---
{content}
--- end document ---

--- tooling/scripts diff ---
{tooling_diff[:4000]}
--- end diff ---

Does this diff make any commands, paths, or instructions in this document wrong?
If yes, return the COMPLETE corrected document — never a summary or a shortened version.
If no, reply with the single token NO_CHANGES and NOTHING ELSE — do not explain."""

        result = client.complete(_SYSTEM, user)

        if git_util.is_no_change(result):
            print(f"  {path}: no changes needed")
            continue

        try:
            git_util.write(path, result)
        except git_util.SuspiciousWriteError as e:
            print(f"  {path}: REFUSED — {e}")
            continue
        updated.append(path)
        print(f"  {path}: updated")

    return updated
