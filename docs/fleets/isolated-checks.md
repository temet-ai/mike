---
title: Isolated fleet commit checks
status: active
created: 2026-10-13
updated: 2026-11-06
summary: Exact-commit grading, reusable managed checkouts, and explicit working-tree command boundaries.
---

# Two execution contracts

Configured automatic checks, explicit configured gates, and startup gates grade
their captured commit in a detached managed worktree. A later change to the
shared branch does not retarget a queued grade. Historical `fleet_check(commit)`
uses the same isolation machinery while retaining its separate baseline ledger.
Normal automatic grades retain their notification and proof identities.

Explicit `fleet_check(command)` remains a working-tree operation: it runs on the
shared checkout and can inspect intended uncommitted changes. It is not silently
converted to a historical grade. A waiting/running receipt is not a pass.

# Managed worktrees and source validity

Mike keeps its managed worktrees inside the project, under `.claude/worktrees/`:
`mike-check` and `mike-check-2` for checks, `mike-gate` for gates
(`launch/fleet/gate_slot.rs`). It adds `.claude/worktrees/` to the repository's
`info/exclude` so they stay out of `git status`. Seats do not create additional
worktrees or per-task targets.

Each slot has a lock file next to it (`<slot>.lock`, in `.claude/worktrees/`) held
for preparation, execution, post-run source verification and final gate-log
publication (`launch/fleet/gate_isolation.rs`). A check takes whichever of its two
slots is free and waits for one otherwise; a gate uses `mike-gate`. The wait is
bounded by the operation's timeout.

Preparation verifies the slot's canonical location, repository identity and
detached state. A slot with untracked files is refused rather than reset, cleaned
or stashed; tracked changes an earlier run left are put back before the run, and the
output says so. Switching commits refuses to overwrite an ignored obstruction.
Tracked, staged and untracked peer files in the shared checkout are left untouched.

Each slot retains its own ignored build cache. Mike does not copy the shared
target tree, arbitrary untracked files or `node_modules`. The configured command
must prepare the ignored dependencies it needs on first use. Mike's internal Git
preparation and verification disable Git hooks command-locally
(`core.hooksPath=/dev/null`, `launch/fleet/owned_git.rs`); ordinary commit hooks
are unchanged. Projects relying on checkout-hook setup must express that setup
inside the configured grading command. Failure output names this boundary.

Post-run source verification checks the captured SHA and tracked and untracked
working-tree state (`launch/fleet/gate_source.rs`). Mutation or unavailable
verification invalidates reusable proof, even if the process originally returned
zero. After a run, files the command changed in the slot are put back and the
output names them; a check must not write source files. Diagnostics are
interpreted relative to the actual execution checkout. Ownership notifications do
not prove who caused a failure; genuinely committed defects remain failures.

This is source isolation, not a hermetic filesystem sandbox. Commands that read
absolute paths back into the shared checkout or other external state cannot have
those inputs certified by Git status. Project command design, dependency versions,
features and artifact provenance still matter.

# Logs, timeouts and cancellation

The latest gate log (`.claude/worktrees/mike-gate-output.log` in the slot) and its
execution-specific completion log are published only after the managed directory
is checked as an ordinary directory, while the slot's lock is held. If it is not,
nothing is written there and persisted output remains the fallback. A previous
completion cannot overwrite a later gate's streamed log.

Internal Git calls have a 60-second bound. Cleanup of a cancelled or timed-out
command's process group is best-effort; it does not prove synchronous termination
of arbitrary Git-filter descendants before the lock is released. Canceled,
timed-out or source-unverified work does not become a successful grade.

# Early shared scaffolds

When tasks share a file, the producer's compiling scaffold is committed before
later contributions. Queue the narrow project compile check through
`fleet_check(command)` and wait for a real pass; the scaffold is not final
acceptance.

Shared files may receive successive named commits. `fleet_commit` snapshots whole
files, not selected authors' hunks: coordinate their complete contents and never
sweep unfinished peer edits into a scaffold. The commit is built in a private
index, so unrelated staged files stay out of it. The regression
(`launch/fleet/commit_stub_tests.rs`) commits a shared file from two distinct
sessions in turn and checks that both commits touch only that file and leave the
peer's staged and untracked files as they were.
