"""Step 4: update frontend route docs when SvelteKit routes change."""

import sys
from pathlib import Path
from datetime import date

sys.path.insert(0, str(Path(__file__).parent.parent))
import client
import git_util

_SYSTEM = """You are a technical writer maintaining the frontend route documentation for temet-proxy,
a SvelteKit SPA that serves as the inspection UI for captured Claude API traffic.

Rules:
- Only document routes that actually exist in the codebase.
- Only add/remove/rename entries that the diff justifies — do not invent pages.
- Update the `updated:` frontmatter field to today's date when you make any change.
- If no change is needed, reply with the single token NO_CHANGES and NOTHING ELSE — do not explain.
- If a change is needed, return the COMPLETE current document with your edits applied,
  never a summary or a shortened version."""


def run(diff: str) -> list[str]:
    routes_diff = git_util.scope_diff_to(diff, "frontend/src/routes/")
    if not routes_diff.strip():
        return []

    content = git_util.read("docs/frontend/ROUTES.md")
    today = date.today().isoformat()

    user = f"""Today: {today}

--- current docs/frontend/ROUTES.md ---
{content}
--- end document ---

--- frontend routes diff ---
{routes_diff[:4000]}
--- end diff ---

Were any SvelteKit routes added, removed, or renamed?
If yes, update ROUTES.md and return the COMPLETE document — never a summary or a
shortened version.
If no structural route changes (only internal component edits), reply with the single
token NO_CHANGES and NOTHING ELSE — do not explain."""

    result = client.complete(_SYSTEM, user)

    if git_util.is_no_change(result):
        print("  docs/frontend/ROUTES.md: no changes needed")
        return []

    try:
        git_util.write("docs/frontend/ROUTES.md", result)
    except git_util.SuspiciousWriteError as e:
        print(f"  docs/frontend/ROUTES.md: REFUSED — {e}")
        return []
    print("  docs/frontend/ROUTES.md: updated")
    return ["docs/frontend/ROUTES.md"]
