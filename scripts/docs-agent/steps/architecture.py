"""Step 1: update architecture docs when crates/ changes."""

import sys
from pathlib import Path
from datetime import date

sys.path.insert(0, str(Path(__file__).parent.parent))
import client
import git_util

_SYSTEM = """You are a technical writer maintaining architecture documentation for temet-proxy,
a Rust reverse proxy that captures Claude API traffic for inspection.

Rules (non-negotiable):
- Only correct facts that the diff makes wrong. Never rewrite content that is still accurate.
- Never add historical narrative: no "previously", "used to", "now does X instead", phase/wave labels.
- Describe what the system IS, not how it got there.
- Update the `updated:` frontmatter field to today's date when you make any change.
- If no change is needed, reply with the single token NO_CHANGES and NOTHING ELSE — do not explain.
- If a change is needed, return the COMPLETE current document with your edits applied,
  never a summary or a shortened version."""

_DOCS = [
    (
        "docs/ARCHITECTURE.md",
        "Core architecture, system map, port layout, component responsibilities, MCP tool count.",
    ),
    (
        "docs/REQUEST_FLOW.md",
        "Runtime request flow: proxy bind, capture pipeline, SSE tee, inspect bind. Line-cited.",
    ),
    (
        "docs/STARTUP_FLOW.md",
        "Boot sequence for mike and mike-mcp binaries. Line-cited.",
    ),
]


def run(diff: str) -> list[str]:
    crate_diff = git_util.scope_diff_to(diff, "crates/")
    if not crate_diff.strip():
        return []

    today = date.today().isoformat()
    updated: list[str] = []

    for path, description in _DOCS:
        content = git_util.read(path)
        if not content:
            continue

        user = f"""Document: {path}
Purpose: {description}
Today: {today}

--- current document ---
{content}
--- end document ---

--- crates/ diff ---
{crate_diff[:5000]}
--- end diff ---

Does this diff make any facts in the document wrong or incomplete?
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
