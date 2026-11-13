"""Step 2: update docs/SCHEMA.md when migrations change."""

import sys
from pathlib import Path
from datetime import date

sys.path.insert(0, str(Path(__file__).parent.parent))
import client
import git_util

_SYSTEM = """You are a technical writer maintaining the database schema reference for temet-proxy.
The database is Postgres with a `mike` schema, using sqlx migrations.

Rules:
- Only add or correct what the migration actually changes: new tables, columns, indexes, views,
  dropped objects.
- Preserve existing structure and descriptions for objects that didn't change.
- Update the `updated:` frontmatter field to today's date when you make any change.
- If no change is needed, reply with the single token NO_CHANGES and NOTHING ELSE — do not explain.
- If a change is needed, return the COMPLETE current document with your edits applied,
  never a summary or a shortened version."""


def run(diff: str) -> list[str]:
    migration_diff = git_util.scope_diff_to(diff, "crates/mike-base/src/store/persistence/migrations/")
    if not migration_diff.strip():
        return []

    content = git_util.read("docs/SCHEMA.md")
    today = date.today().isoformat()

    user = f"""Today: {today}

--- current docs/SCHEMA.md ---
{content}
--- end document ---

--- migration diff ---
{migration_diff[:5000]}
--- end diff ---

Update SCHEMA.md to reflect new or changed tables, columns, indexes, views, or dropped objects.
If the migration is data-only (no DDL changes), reply with the single token NO_CHANGES
and NOTHING ELSE — do not explain.
Otherwise return the COMPLETE updated document — never a summary or a shortened version."""

    result = client.complete(_SYSTEM, user)

    if git_util.is_no_change(result):
        print("  docs/SCHEMA.md: no changes needed")
        return []

    try:
        git_util.write("docs/SCHEMA.md", result)
    except git_util.SuspiciousWriteError as e:
        print(f"  docs/SCHEMA.md: REFUSED — {e}")
        return []
    print("  docs/SCHEMA.md: updated")
    return ["docs/SCHEMA.md"]
