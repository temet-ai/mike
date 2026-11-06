---
title: Release source gates and hosted routing
status: active
created: 2026-10-14
updated: 2026-10-18
summary: Local source-gate commands, strict frontend audit outcomes, hosted routing, and the operator-requested October Actions pause.
---

# Release source gates and hosted routing

WP01 / SG01–SG03 verify the selected source revision. They do not qualify archive
bytes, installed journeys, publication, or a different source revision. Record the
actual source SHA, clean/dirty tree state, command, tool versions and exit status
with each result. A required FAIL, NOT RUN, or DEFERRED prevents qualification.

## Temporary hosted CI pause

GitHub Actions is disabled at repository level for `temet-ai/mike` at the
operator's request through 2026-10-31 (Africa/Johannesburg). This covers all
workflows, including scheduled agents and manual dispatches. Workflow definitions
and local quality gates remain in place.

During this pause, use applicable local checks. Do not dispatch, rerun, or poll
hosted jobs, and do not repeatedly raise the known billing limitation in progress
or completion reports. Report actual local validation and new failures. Retain
truthful evidence: a local pass does not mean a hosted check passed. If explicitly
asked about hosted CI, describe it as intentionally paused.

Review availability from 2026-11-01. Restoration is manual, with no automatic
timer. After confirming hosted execution is available, restore the prior
repository setting and verify it:

```sh
gh api --method PUT repos/temet-ai/mike/actions/permissions -F enabled=true
gh api repos/temet-ai/mike/actions/permissions --jq .enabled
```

Only `enabled` was changed; existing workflow definitions and other permission
settings are preserved. Once restored, remove the temporary availability notice
from `AGENTS.md` and `CLAUDE.md` and update this section. New events can trigger
workflows; there is no need to rerun the October backlog.

## Local execution first

Use the existing local commands when their prerequisites are
available; do not install tools or contact an unrelated database implicitly.

- `scripts/gate check`: authoritative Rust formatting, structural lint, full fast
  and DB tests, and `cargo deny check` advisory/license policy.
- `scripts/test-suite all`: Rust test families, frontend typecheck/logic/Storybook,
  the frontend advisory gate, Playwright smoke, and script/release regressions.
- `scripts/test-suite frontend-audit`: the same required frontend advisory gate
  on its own, without running a browser or compiling Rust.
- `pnpm --dir frontend run storybook:build`: separate production Storybook build.

`test-suite all` remains fail-fast: later phases are unexecuted after a failure.
It is not a substitute for `scripts/gate check` (which includes lint and the Rust
advisory/license gate) or for the standalone Storybook build. Preserve each real
exit status; never pipe a gate through a command that masks failure.

The DB tier requires an explicitly selected disposable PostgreSQL 19 fixture with
pgvector and `MIKE_TEST_ADMIN_URL`; the capture database is never an implicit
fallback. Hosted Rust uses the exact digest pinned by the installed provider's
`DEFAULT_IMAGE` in `crates/mike/src/setup/docker_resource.rs`, checked for drift
by the offline routing tests. This configuration does not itself prove that the
container started or the migrations succeeded.

## Frontend advisory policy

`scripts/frontend-audit` runs the installed project-declared pnpm against the
frontend lockfile. It audits production, development and optional dependencies,
uses the lowest documented `--audit-level=low`, and additionally fails any `info`
advisories present in the report. It never fixes dependencies, adds ignores,
raises the severity threshold, tolerates registry errors, or downloads a package
manager. It also disables Corepack network downloads for this invocation.

A valid complete JSON report, zero advisories, a zero pnpm exit status, and an
unchanged lockfile are all required for PASS. Metadata counts and returned
advisories both count: suppressed inventory cannot turn nonzero totals green.
The lockfile SHA-256 is printed to bind the result to the audited dependency set.

- Exit 0 / PASS: a complete successful audit with zero reported advisories.
- Exit 1 / FAIL: one or more advisories at any reported severity, including those
  without a fix; a zero pnpm exit does not override reported vulnerabilities.
- Exit 2 / NOT RUN: prerequisites unavailable, registry/tool timeout or failure,
  an incomplete/unsupported report, unexpected command arguments, or a changed
  lockfile. This fails the containing source gate; it never means a clean audit.

The audit needs the declared pnpm/Node toolchain and registry access. The command
rejects policy-changing arguments. Arbitrary registry/tool error bodies are not
printed because they may include authenticated URLs. Consult the installed
package manager's own diagnostics privately when the generic failure reason
needs more detail. pnpm's audit flags and registry-error behavior are documented
in the [official audit reference](https://pnpm.io/cli/audit).

## Hosted routing contract

`rust-quality.yml`, `storybook.yml` and `e2e.yml` have three equivalent routes to
their existing jobs:

- Pull requests retain domain path filters to avoid irrelevant expensive checks.
- Pushes to `main` are unfiltered, including documentation-only source changes.
- Manual `workflow_dispatch` and reusable `workflow_call` entrypoints are
  unfiltered. Checkout uses the event's selected ref, never a hardcoded `main`.

The explicit candidate route is `.github/workflows/release-source-gates.yml`
("Candidate source gates"). After hosted execution is authorized and available,
select the reviewed candidate branch/tag as its dispatch ref. Its three relative
reusable workflow references and default checkouts resolve to the same event
commit. Verify the resulting run SHA against the intended candidate; selecting a
moving branch is not evidence of a previously reviewed SHA. The workflow also
runs the offline routing and audit-failure regressions. It does not build or
publish release archives and has only `contents: read` permissions.

Each reusable gate's concurrency group includes the caller workflow name so a
standalone main/manual run cannot cancel a candidate orchestration run. No route
adds `continue-on-error`, job-level branch exclusions or inherited secrets.

These are checked-in routes, not evidence of successful hosted execution.
Hosted job execution, trigger delivery and service startup remain **NOT RUN /
DEFERRED: intentional hosted CI pause** until actual execution provides those
observations. Repository Actions availability was verified as disabled on
2026-10-18. This expected state is recorded here rather than repeated in routine
completion reports.

## Offline regression commands

These commands use Python's standard library, temporary files and fake audit
responses. They do not contact a registry, start containers, install software,
run Rust/Node/browser suites, or invoke GitHub:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_source_gate_routing.py' -v
python3 -m unittest discover -s scripts/tests -p 'test_frontend_audit.py' -v
```

They verify source routing, candidate calls, read-only permissions, PG19 image recipe
identity, local/hosted audit registration, all-severity failure propagation,
missing and malformed evidence, timeout, missing tools, lockfile drift, and the
real `test-suite` wrapper with fake audit tools. They are also included in
`scripts/test-suite scripts`. A PASS here proves those offline contracts only;
it is not a live advisory result or hosted SG03 qualification.
