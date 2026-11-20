---
title: Executable fleet rulings
status: active
created: 2026-10-13
updated: 2026-11-20
summary: Register acceptance-owned checks and require clean-commit receipts for every affected task.
---

# Executable fleet rulings

Acceptance registers a prospective ruling through `write_session_note`, using
its own session, `kind: task`, and tags `acceptance-obligation` and
`fleet:<fleet UUID>`. The affected tasks must be explicit. Name only the work
within the new ruling's scope; a prose mail or amended brief never creates,
replaces, or satisfies an executable ruling.

Example `data`:

```json
{
  "obligation_id": "Q29-action-ids",
  "tasks": ["T4", "T5"],
  "criteria": [],
  "required": true,
  "outcome": "pending",
  "ruling": {
    "version": 1,
    "command": "scripts/check-action-ids",
    "expected_exit_code": 0
  }
}
```

The command must assert the desired condition. `expected_exit_code` is required
and ranges from 0 through 125; for example, a forbidden-pattern grep can expect
1. A command's success establishes its assertions, not arbitrary claims in its
output. Existing prose obligations have no executable semantics and remain
backward compatible.

Registration validates the acceptance owner, task/criterion scope and ruling
schema in the same transaction as the note write. Mike sends the affected
seats its current list of rulings; replacement seats receive the list in their
starting prompt. `get_fleet` retains the authoritative notes and proof status,
so a missing mail does not erase the requirement.

## Run and report

After committing, a worker calls:

```json
{"fleet":"example","session_id":"<worker UUID>","ruling":"<ruling note UUID>","commit":"<reported SHA>"}
```

Use the existing `fleet_check` tool, or
`mike fleet check --ruling <note UUID> --commit <SHA>`. The ruling selector
cannot be combined with `test`. A task seat can request only its affected
rulings; fleet control seats can inspect or run them. `status: true` starts
nothing.

The command runs in Mike's locked, reusable historical checkout at the resolved
SHA. Mike verifies its repository identity, detached commit, and clean state
before and after execution. Shared uncommitted files are not used. A timeout,
unknown exit status, changed checkout, or refused checkout is never a pass.

The durable tree-check ledger is separate from the fleet's default check and
from focused tests. Its identity includes the ruling note UUID, version,
command, expected exit code, task/criterion scope, full commit SHA and command
fingerprint. Running another command, another revision, or another commit
cannot satisfy it. The shell wrapper compares the actual command exit code
with the declared expected code; the receipt's zero exit is that comparison's
result, with the command's observed exit and full output retained.

`fleet_report` records each task ruling under `proof.rulings`, including the
command, version, expected result, reported commit, state, ledger, check ID,
timestamps, full output and `passed`, `failed`, `running`, `missing`, or
`unverifiable` outcome. `proof.rulings_unavailable` exposes a failed register
read. Captured Bash receipts remain separately labelled as having unknown
commit binding; worker prose and generic tool success cannot satisfy a ruling.

A task's `done_when` is a ruling mike declares itself (T13, `ba9bc34e`): when a
seat reports, `floor.rs` builds one ruling from the task's `checks`,
`files_changed`, `tests_added_under` and `exercise`, runs it at the reported
commit and records `floor_passed` or `floor_failed`
([`fleets/events.md`](fleets/events.md)). Unlike a registered ruling it is the
task's own floor: a failing floor sends the seat a code and no judge sees the
report. See [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md) for the codes.

Acceptance still records its observation and final verdict. Marking the
obligation `accepted` or writing a final PASS alone cannot satisfy a required
ruling: fleet status verifies every affected task's current reported commit
against the exact durable receipt. New commits require their own receipt.

Fleet status reads the latest verdict and current obligation outcomes together.
A final PASS must be at least as recent as each required obligation's creation
(or its current superseding note's creation). Recording an accepted outcome or
adding an observation to that existing obligation takes effect immediately;
acceptance does not need to repeat the final PASS. Pending, blocked, failed,
not-exercised and invalidated outcomes still block acceptance. Newly created or
superseding obligations require a final PASS covering their new creation time.

All acceptance obligations retain their identity, logical task/criterion scope,
required flag and fleet tags when updated; equivalent task/package aliases and array ordering
are accepted for non-executable obligations. Changed scope or identity needs a
new obligation, and a changed version of the same obligation uses supersession.
A malformed legacy observation can be repaired in place. If an invalid legacy
identity or scope cannot be repaired within those boundaries, remove its
`acceptance-obligation` tag, keep the original record, and create a corrected
obligation. Valid obligations cannot shed that tag to bypass acceptance. Existing ordinary
notes, including quarantined legacy notes, cannot gain the marker later; create
a new obligation rather than reusing an older note's creation time. Changing an
optional obligation into a required one likewise needs a new successor. An
omitted required flag and explicit `true` are the same requirement.

## Change a ruling

The executable contract, version, scope, obligation identity and tags are
immutable on a registered note. Observation, outcome, body and status updates
remain possible. Update `data` with its complete existing contract and scope,
because notebook updates replace that JSON rather than merging it.

To change the command, create a successor acceptance note with the same
`obligation_id` and scope, `supersedes: <current note UUID>`, and a higher ruling
version. Previous receipts remain auditable but cannot satisfy the successor.
Executable ruling records cannot be deleted; unrelated notebook cleanup keeps
its usual behavior.
