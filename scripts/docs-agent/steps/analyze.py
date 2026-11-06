"""Step 0: analyze the diff and decide which doc categories need updating."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
import client
import git_util

_SYSTEM = """You are a code change classifier for temet-proxy, a Rust reverse proxy for Claude API traffic.
Given a git diff, decide which documentation categories need updating.
Return ONLY valid JSON — no markdown fences, no prose."""

_REQUIRED_KEYS = {"summary", "architecture", "schema", "guides", "ui_help"}

# A malformed analysis must never abort the run or trigger a doc write — treat
# it the same as a legitimate "nothing affected" classification.
_FALLBACK_ANALYSIS = {
    "summary": "docs-agent: analysis response was unparseable — no docs updated",
    "architecture": False,
    "schema": False,
    "guides": False,
    "ui_help": False,
}


def run() -> dict:
    files = git_util.changed_files()
    stat = git_util.diff_stat()
    diff_head = git_util.diff_full()[:6000]

    user = f"""Changed files:
{chr(10).join(files)}

Diff summary:
{stat}

Diff (first 6 KB):
{diff_head}

Return JSON with exactly this structure:
{{
  "summary": "one sentence — what changed, present tense",
  "architecture": true/false,
  "schema": true/false,
  "guides": true/false,
  "ui_help": true/false
}}

Criteria:
- architecture: true when crates/ changed in a way that alters system design, port layout,
  data flows, component responsibilities, or MCP tool count/shape.
- schema: true when migrations/ has new .sql files or existing ones changed.
- guides: true when scripts/, Makefile, .cargo/, or developer-workflow files changed.
- ui_help: true when frontend/src/routes/ gained or lost route files or page components."""

    raw = client.complete(_SYSTEM, user)
    parsed = _parse_classification(raw)
    if parsed is not None:
        return parsed

    # A decorated first reply is a formatting slip, not evidence there's
    # nothing to document — retry once with the exact bad output called out
    # before conceding the run to the "nothing to update" fallback, so a
    # commit that genuinely needs docs updated doesn't silently get skipped.
    print(f"  analyze: first response unparseable ({raw[:200]!r}) — retrying with a corrective prompt")
    retry_user = (
        f"{user}\n\n"
        f"Your previous reply could not be parsed as JSON:\n{raw[:500]}\n\n"
        f"Reply with ONLY the JSON object — no markdown fence, no explanation, "
        f"nothing before or after the braces."
    )
    raw = client.complete(_SYSTEM, retry_user)
    parsed = _parse_classification(raw)
    if parsed is not None:
        return parsed

    print(
        f"  analyze: retry also unparseable ({raw[:200]!r}) — falling back to "
        f"'nothing to update'; this run's classification failed, doc drift may "
        f"go undetected for this commit"
    )
    return dict(_FALLBACK_ANALYSIS)


def _parse_classification(raw: str) -> dict | None:
    parsed = git_util.parse_json_lenient(raw)
    if isinstance(parsed, dict) and _REQUIRED_KEYS.issubset(parsed):
        return parsed
    return None
