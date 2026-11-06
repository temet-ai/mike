"""Git and filesystem helpers for the docs-agent."""

import json
import os
import re
import subprocess
from pathlib import Path

import httpx

ROOT = Path(__file__).parent.parent.parent


class SuspiciousWriteError(RuntimeError):
    """Refused to overwrite an existing doc with a drastically smaller body."""


def _git(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=check,
        capture_output=True,
        text=True,
    )


def _diff_range() -> tuple[str, str]:
    """Return (base, head) for the diff, honouring DIFF_RANGE if set."""
    env_range = os.environ.get("DIFF_RANGE", "").strip()
    if env_range and ".." in env_range:
        base, head = env_range.split("..", 1)
        return base, head
    return "HEAD~1", "HEAD"


def diff_full() -> str:
    base, head = _diff_range()
    return _git("diff", base, head, "--unified=2").stdout


def diff_stat() -> str:
    base, head = _diff_range()
    return _git("diff", base, head, "--stat").stdout


def changed_files() -> list[str]:
    base, head = _diff_range()
    out = _git("diff", "--name-only", base, head).stdout
    return [f for f in out.strip().splitlines() if f]


def read(path: str) -> str:
    p = ROOT / path
    return p.read_text() if p.exists() else ""


_SENTINEL = re.compile(r"^NO[ _\-]?CHANGES?\b\s*(?P<rest>.*)$")
_CONTINUATIONS = {"ARE", "NEEDED", "REQUIRED", "NECESSARY", "MADE", "DETECTED"}


def is_no_change(result: str) -> bool:
    """True when the model replied with a no-change sentinel, not a document.

    Models decorate the sentinel ("NO_CHANGES\\n\\nexplanation…", "**NO_CHANGES**",
    "No changes."), so an exact == comparison treats the decoration as a document
    and clobbers the file with it. But stripping ALL punctuation over-matches:
    "No, changes are needed." must NOT collapse into the sentinel. So: strip
    markdown decoration only, require the token at line start with word-boundary
    punctuation intact, and trust prose continuations only on short replies —
    a long body is a document, and write()'s clobber guard (not this check) is
    the defense against writing a bad one.
    """
    stripped = result.strip() if result else ""
    if not stripped:
        return True
    first_line = stripped.splitlines()[0]
    line = re.sub(r"[*`#>\"']", "", first_line).strip().upper()
    m = _SENTINEL.match(line)
    if not m:
        return False
    rest = m.group("rest").strip()
    if not rest or rest in {".", "!"}:
        return True
    if len(stripped.splitlines()) > 15:
        return False
    return rest[0] in ".,;:!—–-" or rest.split()[0].rstrip(".,;:!—–-") in _CONTINUATIONS


_FENCE = re.compile(r"^```(?:json)?\s*|\s*```\s*$")


def parse_json_lenient(raw: str) -> dict | None:
    """Parse a JSON object out of a model response that may be fenced and/or
    followed by trailing prose — the same decoration problem as is_no_change,
    but json.loads has no tolerance for it and raises "Extra data" instead of
    just failing a string match. Returns None on anything unparseable; callers
    must treat that as absence of data, never let it propagate as a crash.
    """
    if not raw or not raw.strip():
        return None
    stripped = _FENCE.sub("", raw.strip()).strip()
    try:
        obj, _ = json.JSONDecoder().raw_decode(stripped)
    except (json.JSONDecodeError, ValueError):
        return None
    return obj if isinstance(obj, dict) else None


def write(path: str, content: str) -> None:
    p = ROOT / path
    # Clobber guard: a doc rewrite must return the COMPLETE document, so a body
    # under half the existing line count is a stub/summary, never a valid edit.
    if p.exists():
        existing_lines = len(p.read_text().splitlines())
        new_lines = len(content.splitlines())
        if existing_lines > 20 and new_lines < existing_lines * 0.5:
            raise SuspiciousWriteError(
                f"refusing to overwrite {path}: new content is {new_lines} lines "
                f"vs {existing_lines} on disk — probable clobber, not an edit"
            )
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(content)


def working_tree_changes() -> list[str]:
    out = _git("diff", "--name-only").stdout
    return [f for f in out.strip().splitlines() if f]


def commit_and_push(branch: str, message: str) -> bool:
    changed = working_tree_changes()
    if not changed:
        return False
    _git("config", "user.email", "docs-agent@github-actions")
    _git("config", "user.name", "docs-agent")
    _git("checkout", "-b", branch)
    _git("add", "--", *changed)
    _git("commit", "-m", message)
    _git("push", "-u", "origin", branch)
    return True


def create_pr(title: str, body: str) -> None:
    token = os.environ["GH_TOKEN"]
    repo = os.environ.get("GITHUB_REPOSITORY", "temet-ai/mike")
    result = _git("branch", "--show-current")
    head = result.stdout.strip()
    resp = httpx.post(
        f"https://api.github.com/repos/{repo}/pulls",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
        json={"title": title, "body": body, "head": head, "base": "main"},
        timeout=30,
    )
    if not resp.is_success:
        print(f"  PR creation error {resp.status_code}: {resp.text[:500]}")
    resp.raise_for_status()


def scope_diff_to(diff: str, *prefixes: str) -> str:
    """Return only the hunks whose paths start with one of the given prefixes."""
    lines: list[str] = []
    include = False
    for line in diff.splitlines():
        if line.startswith("diff --git"):
            include = any(f" b/{p}" in line for p in prefixes)
        if include:
            lines.append(line)
    return "\n".join(lines)
