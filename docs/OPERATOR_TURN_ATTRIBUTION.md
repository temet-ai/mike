---
title: Operator-turn attribution
status: active
created: 2026-09-20
updated: 2026-11-01
summary: Worker turn inheritance, its evidence boundaries, the historical repair command, turn disposition by prompt kind, and agent-lineage requests on the primary lane.
---

# Operator-turn attribution

Worker requests inherit the turn that dispatched them. Attribution uses an
`agents.instances` dispatch link, a session-scoped worker producer ID, or the
structured result of `spawn_session` / `mcp__mike__spawn_session`. A request is
linked only to an operator turn of its own session (#619): a separate child
session, a session started with `spawn_session`, and anything that session
spawns in turn never take the parent session's turn, so a turn's facts, dossier
and verdicts describe one session's work. Conflicting parent links and cycles
without an attributable root remain unknown. A persona name alone does not
establish parentage.

Inline subagent requests without a dispatch link use the turn covering their
arrival time in their own session. Membership is written at initial capture,
so finishing after the parent advances does not move a streaming worker into
the next turn. Aborting an attributed child preserves the parent's lifecycle.
Late captured dispatch results repair already completed child requests.

Completing an inherited request refreshes the parent turn's materialized tool,
error, token and text aggregates. Child requests do not replace the parent's
start/end request pointers or open another operator turn.

## Turn disposition

Only a primary-lane `user_prompt` request without an agent id can open or close a turn, and
what it does depends on `dossier::genuine::classify_prompt`:

| Request | Effect on the open turn | Row opened |
|---|---|---|
| genuine operator prompt | closed | `opened_by = 'operator'` |
| in-lane machine event (hook feedback, teammate message, system notification, loop wakeup, channel wake, tool loaded, compaction, empty, harness-authored) | closed | `opened_by = 'machine'` |
| probe (activity label, analysis probe, session recap) | untouched | none, `operator_turn_id` stays NULL |
| agent id present, any origin | request attached | none |
| any other lane or a tool loop | request attached | none |

A machine row keeps the tool loop that follows a background event attributed to a turn.
Readers that mean "operator turn" (the turn judge claim, the session judge re-judge trigger)
skip `opened_by` of `machine` and `agent`; NULL counts as operator. The aborted-request path
applies the same table: an aborted sidecar or subagent request no longer closes the operator's turn.

## Agent-lineage requests on the primary lane

Requests carrying `x-claude-code-agent-id` were stored `primary` and `operator` since
2026-09-17 and opened operator turns in the parent session (#354). Root cause:

1. The workflow subagent marker had drifted. `extract/request/turn_origin_classify.rs` matched
   only `running inside a workflow script`, while current Claude Code workflow agents carry
   `You are a subagent spawned by a workflow orchestration script.` in their system prompt.
2. With the marker missed, the fallback compared the request's persona with the session
   persona, which is set by the first primary request mike sees for the session in this
   process and never persisted. A workflow agent seen first after a restart became the
   session's identity, and every later agent request of that session classified `primary`.
3. Content then classified the prompt `operator`, and the snapshot opened a turn.

The fix: the agent id decides the lane before any text is read (`workflow_subagent` when a
marker is present, else `subagent`), the session persona is established only from requests
without an agent id, and the workflow markers live in `dossier/genuine/markers.rs`:
the system phrases above, plus the `[Workflow harness —` prefix, which begins a text block
of the first user message (not the system prompt). `author_kind = harness` is not a lane
signal: the main thread's own tool loops are stored `harness` on the primary lane, so the
agent id alone separates the lanes. `classify_author_kind` reads a primary-lane request with
an agent id as `agent`.

Every per-session injection stage (`lessons`, `self_identity`, `workspace_briefing`,
`reminders`) is a separate concern, gated on the actor lane by JC01-F.

## Historical repair

After applying migration `20260920000004_operator_turn_inheritance.sql`, run:

```sh
scripts/backfill-operator-turns
```

The command loads the repository `.env` and uses `MIKE_DATABASE_URL`. It processes
100 unattributed request IDs per transaction, repairs membership and refreshes
affected turn aggregates. It can be interrupted and rerun: existing membership
is preserved, and requests without usable lineage do not stall progress. No
historical writes run automatically during migration or proxy startup.

Requests in independent sessions with neither a captured dispatch link nor an
operator turn stay unattributed. In particular, a terminal launched through an
uncaptured CLI command cannot be linked merely because it has a familiar agent
name. Existing non-null memberships are not rewritten by the repair.

## Query performance

Capture disables PostgreSQL JIT locally within its transaction before resolving
inheritance. Compiling this small recursive graph query costs substantially more
than executing its indexed lookups. The committed performance snapshot expands
the parent-link view for measurement against the capture database before the
migration is installed; measure with `PGOPTIONS='-c jit=off'`, matching capture.
The backfill uses the same attribution SQL and transaction-local JIT setting.

## Outcome labels

`memory.operator_turns.outcome_id` is a projection with one writer, `project_turn_outcomes` (`store/persistence/operator_turns/projection.sql`). Turn close, the idle reap and the abort path close turns and write no label. The projection reads the operator's gold labels and the live verdict of the promoted turn-judge bundle, and records which one decided the label in `outcome_source`. A label written before the projection existed keeps `outcome_source` NULL until the projection or the history re-judge overwrites it, and its old value is kept in `memory.operator_turn_legacy_labels`.
