"""Fail-closed pnpm audit with explicit unavailable/invalid evidence outcomes."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
PROJECTS = ("frontend", "services/prompt-assistant")
SEVERITIES = ("info", "low", "moderate", "high", "critical")
BLOCKING = ("high", "critical")
# No fix, ignored advisories, severity downgrade, or dependency-class exclusions.
# Refuse pnpm's automatic package-manager download/version switch as well.
AUDIT_ARGS = ["audit", "--json", "--audit-level=high", "--dev", "--prod",
              "--optional", "--ignore-registry-errors=false", "--pm-on-fail=error"]
TIMEOUT_SECONDS = 120


def no_result(reason):
    print(f"FRONTEND-AUDIT: NOT RUN ({reason})", file=sys.stderr)
    return 2


def audit_counts(report):
    """Only the full pnpm report proves a completed registry response."""
    if not isinstance(report, dict) or "error" in report:
        raise ValueError("registry did not return an audit report")
    metadata = report.get("metadata")
    counts = metadata.get("vulnerabilities") if isinstance(metadata, dict) else None
    if not isinstance(counts, dict) or set(counts) != set(SEVERITIES):
        raise ValueError("missing or unsupported vulnerability counts")
    if any(type(counts[level]) is not int or counts[level] < 0 for level in SEVERITIES):
        raise ValueError("invalid vulnerability counts")
    advisories = report.get("advisories")
    if not isinstance(advisories, dict):
        raise ValueError("missing advisory inventory")
    observed = dict.fromkeys(SEVERITIES, 0)
    for advisory in advisories.values():
        if not isinstance(advisory, dict) or advisory.get("severity") not in SEVERITIES:
            raise ValueError("invalid advisory severity")
        observed[advisory["severity"]] += 1
    # A populated inventory must fail even if a faulty tool reports zero totals.
    return {level: max(counts[level], observed[level]) for level in SEVERITIES}


def audit_project(root, name):
    project = root / name
    lockfile = project / "pnpm-lock.yaml"
    try:
        lock_hash = hashlib.sha256(lockfile.read_bytes()).hexdigest()
    except OSError:
        return no_result(f"{name} lockfile unavailable")
    pnpm = shutil.which("pnpm")
    if pnpm is None:
        return no_result("pnpm unavailable; install the version declared in frontend/package.json")
    print(f"{name} lockfile sha256: {lock_hash}", flush=True)
    try:
        result = subprocess.run([pnpm, *AUDIT_ARGS], cwd=project, capture_output=True,
                                text=True, timeout=TIMEOUT_SECONDS, check=False,
                                env={**os.environ, "COREPACK_ENABLE_NETWORK": "0"})
    except subprocess.TimeoutExpired:
        return no_result("registry audit timed out")
    except (OSError, UnicodeError):
        return no_result("pnpm could not execute or return a readable report")
    try:
        counts = audit_counts(json.loads(result.stdout))
    except (ValueError, TypeError):
        # Do not echo arbitrary registry/tool error bodies (which can contain
        # authenticated URLs). A successful exit alone is never audit evidence.
        return no_result(f"no valid complete audit report; pnpm exit {result.returncode}")
    summary = name + ": " + ", ".join(f"{level}={counts[level]}" for level in SEVERITIES)
    if any(counts[level] for level in BLOCKING):
        print(f"FRONTEND-AUDIT: FAIL ({summary}; pnpm exit {result.returncode})",
              file=sys.stderr)
        return 1
    if result.returncode != 0:
        return no_result(f"pnpm exit {result.returncode} despite zero reported advisories")
    try:
        if hashlib.sha256(lockfile.read_bytes()).hexdigest() != lock_hash:
            return no_result(f"{name} lockfile changed during the audit")
    except OSError:
        return no_result(f"{name} lockfile became unavailable during the audit")
    print(f"FRONTEND-AUDIT: PASS ({summary})")
    return 0


def run(root=ROOT):
    """Every project is audited: one failure must not hide another's result."""
    verdicts = [audit_project(root, name) for name in PROJECTS]
    return 1 if 1 in verdicts else max(verdicts)


if __name__ == "__main__":
    if len(sys.argv) != 1:
        sys.exit(no_result("no arguments accepted; audit policy cannot be weakened"))
    sys.exit(run())
