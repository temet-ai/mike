---
title: Alpha feedback and corrective releases
status: active
created: 2026-10-13
updated: 2026-11-06
summary: Safe install/runtime feedback forms, candidate handoff requirements, and evidence templates for alpha feedback; the pilot was dropped, and nothing here claims that any target or channel is qualified.
---

# Alpha feedback and corrective releases

This is the feedback kit for the public alpha (FEEDBACK01 in the
[acceptance matrix](RELEASE_ACCEPTANCE_MATRIX.md)). The file keeps its old name
because the issue forms link to it. There is no pilot and no invited cohort: anyone
who installs an alpha may report. The kit does not certify an archive, install channel,
outside-user journey, or corrective release; [`RELEASING.md`](RELEASING.md#where-things-stand-2026-11-06)
records where the release stands. Required **NOT RUN is not a pass**. Local checks remain useful when hosted CI
is unavailable.

## Before pointing users at an alpha

The release lead supplies one candidate-specific page containing:

- Exact version/source SHA, qualified target and OS baseline, archive filenames
  and SHA-256 hashes, and an access-appropriate download location. Do not invent a
  release URL or substitute a source build for an installed-binary observation.
- Tested install → setup → first synthetic capture → browser → MCP → restart
  instructions. State the expected result of each step. Include the four paths:
  `mike claude`, standalone proxy, `mike-mcp`, and embedded browser UI.
- Storage choice and prerequisites: trial is ephemeral; Docker PostgreSQL or a
  compatible existing PostgreSQL instance are the durable choices to qualify.
  User-managed native/Homebrew PostgreSQL is an existing-database option, not an
  assertion that automatic provisioning is supported.
- Tested stop, backup, restore, update, and uninstall steps for that candidate.
  State which files/data remain after uninstall and which deletion is optional.
  Require a verified recoverable backup before a stateful update; do not suggest
  an untested downgrade or deleting a database to fix setup.
- Candidate-specific known limitations, supported client versions, safe failure
  recovery steps, and a **verified private security reporting route**.

If one of these is missing, record it as a blocker and complete the kit before
announcing the alpha. [Getting Started](GETTING_STARTED.md) describes the source-based
development path; it is not evidence of a prebuilt installation. This document
intentionally does not supply guessed install, backup, or uninstall commands.

## Report an ordinary install or runtime problem

Use [Install or update problem](../.github/ISSUE_TEMPLATE/install-problem.yml) or
[Capture or runtime problem](../.github/ISSUE_TEMPLATE/runtime-problem.yml) in the
repository's issue chooser. Each asks what you tried, expected and observed,
version/build identity, platform, storage method, and a reproducible non-sensitive
example. Use `unknown` rather than guessing a version or result.

**Review every field and attachment before submitting.** Never share `.env`, full
environment output, prompts, conversation dumps, request headers, tokens,
credentials, full database URLs, or sensitive paths. Raw logs, screenshots,
`print-config` output and database exports can contain private material; they are
not routine support evidence. Do not paste them into an issue or ask others to.
A synthetic example should be newly written from disposable test data, not a
lightly edited real conversation. If you cannot reproduce safely, say so.

Safe diagnostic summaries are limited to build/platform, coarse configuration
source (defaults, environment, CLI, saved settings), prerequisite/capability state,
and error codes or exit statuses. Copy only reviewed, necessary facts.
`mike --config PRIVATE_CONFIG doctor --support-bundle` prints a read-only,
allowlisted JSON summary locally and uploads nothing automatically. It excludes
configuration values, credentials, private paths and capture content. Preview the
output and explicitly choose whether to share only the necessary fields; a
support summary is not permission to attach raw logs or database exports.
`mike --version` identifies the executable. These implemented commands still need
qualification against the exact installed candidate across stopped, degraded and
ready states before AC27 passes. `mike spawn doctor` concerns a terminal backend,
not a general installation health certificate.

## Security reports

**Do not put vulnerability details or exploit material in the public issue forms.**
The checked-in [security document](SECURITY.md) describes the trust boundary and
names GitHub's private vulnerability reporting (the repository's **Security** tab), but
this guide does not provide a verified private reporting route: on 2026-11-06 GitHub
reported that feature as disabled on `temet-ai/mike`. No private email address or
GitHub advisory endpoint is assumed here.

Until the maintainer enables the feature and confirms it works,
withhold the details until a private route is published and verified.
Do not publish a vulnerability merely because no private route is listed. Enabling and verifying that route is a release-lead prerequisite
to announcing an alpha; these forms do not resolve it.

## Known limitations to carry into each candidate's notes

- An untested target is unsupported. Linux x86_64/Arch, Linux ARM64 and Apple
  Silicon macOS need independent observations. A download archive and Homebrew each need
  their own channel verification; a locally built archive proves neither.
- Trial/in-memory history is lost on restart and is bounded while running.
  Durable retention, backup, restore and predecessor upgrade need real database
  journey evidence; a helper test does not complete those journeys.
- mike is a single-user, loopback-only application with an unauthenticated
  inspect API. Do not expose it to a LAN or the internet; see the
  [trust boundary](SECURITY.md#the-trust-boundary).
- Forwarding is not proof of capture. Standalone clients need a valid canonical
  `x-claude-code-session-id` header. Missing or malformed session identity, and
  certain memory-pressure conditions, can forward uncaptured traffic; see
  [request flow](REQUEST_FLOW.md#capture-reconstruction-and-uncaptured-paths).
  Check that a synthetic request actually appears before claiming capture.
- Optional provider features, sidecar/terminal integrations, automatic updates,
  autostart and additional platforms are not qualified by this kit. State their
  actual per-candidate status rather than inferring support from source code.

## Triage and observation record

Keep one record per observed install or repeat-use attempt, using a pseudonymous
reporter ID. Record only consented, non-sensitive observations; no background
telemetry or conversation uploads are required. Ask whether the user returned,
not what their real conversations contained.

```text
Reporter ID / observation date:
Candidate version / source SHA:
Artifact filename / SHA-256 / target / OS version:
Channel actually used (private candidate, published download, Homebrew, source):
Storage method / PostgreSQL version / client version (or unknown):
Attempt (first install, first capture, return use, corrective update):
Time to first capture (elapsed minutes, or not reached):
Maintainer intervention (none, instructions clarified, manual fix; count):
Tried / expected / actual:
Synthetic reproduction / safe error code / exit status:
Outcome: PASS | FAIL | NOT RUN, with reason and next action:
Issue link / WP ID / acceptance case / priority / owner:
Consent to include this sanitized observation in release evidence:
```

Prioritize P0 (security/data loss) and P1 (blocks the supported core journey) before
new features. Route security details privately. Every public issue/evidence link
must itself be safe to share. Keep the triage record open until the reporter or
an independent verifier confirms the fix on the relevant installed candidate.

## Corrective release notes and evidence template

Fill this in only with observed results. The release lead and independent verifier
record commands, exit statuses, and outcomes without credentials or private paths.
Record local/CI execution separately; a deferred CI run stays NOT RUN.

```text
Candidate version / source SHA / predecessor actually used:
Changes, linked to sanitized feedback issues:
Qualified targets / OS baseline / client and database versions:
Archive filenames / SHA-256 hashes / independently downloaded hashes:
Known limitations / unsupported targets / open P0/P1 blockers:
Tested install, stop, backup, restore, update and uninstall guide:
Verified private security reporting route (confirmed by release lead):
Evidence for each case: case ID, command, exit status, observation, safe link.
AC26: NOT RUN until the actual published channel installs with matching hashes.
AC27: NOT RUN until version, read-only doctor and opt-in diagnostics pass the
      stopped/degraded/ready, idle/expected-capture, and planted-secret cases.
FEEDBACK01: NOT RUN until an independent outside install and feedback exist,
            and a feedback-driven corrective update passes predecessor/channel tests.
Required remaining cases / deferred CI checks / reasons / owners:
Independent verifier / observation date:
Release decision and authorization:
```

The first candidate can be prepared before publication. Observe the real download
channel after publication; staging files do not satisfy AC26. Run the next version
through the same gates and the actual predecessor upgrade (AC23 is outside the first
alpha, which has no predecessor), then link the fix to its feedback. AC26 stays open
until channel installs pass, and FEEDBACK01 until the observed feedback and
corrective release demonstrate repeatability. Adding these templates closes none
of those acceptance cases and does not recruit a cohort or authorize publication.

## Local template check

With Python 3 and PyYAML available (also used by the repository's knowledge tests):

```bash
python3 -m unittest discover -s scripts/tests -p 'test_pilot_feedback.py'
```

This offline check validates form structure, required safe fields, privacy warnings,
security routing, and evidence wording. It does not certify runtime diagnostics,
GitHub's rendered issue chooser, or any installed-user journey.
