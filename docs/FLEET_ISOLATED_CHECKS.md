---
title: Isolated fleet commit checks
status: active
created: 2026-10-13
updated: 2026-10-13
summary: Exact-commit grading, reusable managed checkouts, and explicit working-tree command boundaries.
---

# Two execution contracts

Configured automatic checks, explicit configured gates, and startup gates grade
their captured commit in a detached managed worktree. A later change to the
shared branch does not retarget a queued grade. Historical `fleet_check(commit)`
uses the same isolation machinery while retaining its separate baseline ledger.
Normal automatic grades retain their notification and proof identities.

Explicit `fleet_check(command)` remains a working-tree operation. It can inspect
intended uncommitted changes; it is not silently converted to a historical grade.
A waiting/running receipt is not a pass.

# Managed worktrees and source validity

Mike reuses `.claude/worktrees/mike-check` and `mike-gate` inside the project.
A checkout-level filesystem lock covers preparation, execution, post-run source
verification and final gate-log publication across check, gate, baseline and
startup paths. Seats do not create additional worktrees or per-task targets.

Preparation verifies the slot's canonical location, repository identity and
detached state. Dirty managed slots are refused rather than reset, cleaned or
stashed. Switching commits refuses to overwrite an ignored obstruction. Tracked,
staged and untracked peer files in the shared checkout are left untouched.

Each slot retains its own ignored build cache. Mike does not copy the shared
target tree, arbitrary untracked files or `node_modules`. The configured command
must prepare the ignored dependencies it needs on first use. Internal Git
preparation and verification disable Git hooks command-locally; ordinary commit
hooks are unchanged. Projects relying on checkout-hook setup must express that
setup inside the configured grading command. Failure output names this boundary.

Post-run source verification checks the captured SHA and tracked/nonignored
working-tree state. Mutation or unavailable verification invalidates reusable
proof, even if the process originally returned zero. Diagnostics are interpreted
relative to the actual execution checkout. Ownership notifications do not prove
who caused a failure; genuinely committed defects remain failures.

This is source isolation, not a hermetic filesystem sandbox. Commands that read
absolute paths back into the shared checkout or other external state cannot have
those inputs certified by Git status. Project command design, dependency versions,
features and artifact provenance still matter.

# Logs, timeouts and cancellation

The latest gate log and its execution-specific completion log are published only
through a validated managed log directory while the execution lock is held.
Refused unsafe slots are not used as log destinations; persisted output remains
the fallback. A previous completion cannot overwrite a later gate's streamed log.

Internal Git calls have a 60-second bound. Unix process-group cleanup is
best-effort on cancellation or timeout; it does not prove synchronous termination
of arbitrary Git-filter descendants before the lock is released. Canceled,
timed-out or source-unverified work does not become a successful grade. No live
fleet cancellation or project-specific setup exercise is claimed by this document.

# Early shared scaffolds

The declared producer publishes a compiling scaffold before later implementation
contributions. Use the narrow project compile check; a Rust library scaffold uses
`cargo check -p <crate> --lib`, rather than unrelated tests or all-target suites.
Queue/share it through `fleet_check(command)` and wait for a real pass. Hooks and
scoped formatting still apply, and the scaffold is not final acceptance.

Shared files may receive successive named commits. `fleet_commit` snapshots whole
files, not selected authors' hunks: coordinate their complete contents and never
sweep unfinished peer edits into a scaffold. Unrelated staged files remain outside
the private commit index. The sequential regression uses distinct producer and
peer sessions and verifies both commits preserve unrelated staged/working files.
