---
title: Adding a steerer, a judge question or judge context
status: active
created: 2026-10-05
updated: 2026-11-06
summary: The shared pieces every steerer and judge uses to decide, word, send and grade a note, and where a new one plugs in.
---

# Adding a steerer, a judge question or judge context

## What this gives you

A new steerer, a new turn-guard question or a new piece of context for a judge takes one
small change in one named place. It then obeys the same limits, speaks in the same voice and
is graded by the same labels as everything else that sends a session a note. Nothing about
"may this note be sent" or "what does the judge know about this session" is written twice.

## The shared pieces

| Job | The one place | What it gives |
|---|---|---|
| May this note be sent to this session now | `crates/mike/src/steerer/note_policy.rs` | `before_judging` (the per-session limit and the quiet period after a wrong note), `after_verdict` (is the note needed enough), `refusal` (both, for a producer that judges first), `stop_before_quorum` (the order of checks on a verdict). One set of reasons: `session_note_limit`, `quiet_after_wrong_note`, `need_below_level`. |
| Which notes count | `session_notes_since` in `store/persistence/steering/recent_delivery.rs` | Notes from any steerer that were delivered, pushed or are still waiting, as of a moment, each with how the session took it. |
| How much a note is needed | `Need` and `NEED_RUBRIC` in `steerer/verdict.rs` | Low, medium or high. A judge returns it; a producer with fixed questions declares it (`Question::need` in `steerer/guard/mod.rs`). |
| What the judge knows about the session | The dossier's steerer sections: `SteererSection` in `dossier/view/types.rs`, rendered in `dossier/render/steerer.rs` and `steerer_notes.rs` | Named sections, loaded as of a moment, cut to a budget by one allocator. |
| The words around a note | `steerer/inject.rs` (`build_steering_block_with`, `REPLY_LINE`) and `NUDGE_WORDING` in `steerer/verdict.rs` | The frame every note is delivered in, and the rule a judge follows when it writes one. |
| Was the note right | `steerer/reply_label/` for steerers, `steerer/guard/label.rs` for the turn guard | Both write `outcome_label` on the note's evaluation row, which is what the quiet period and the switch-off rule read. |
| Replaying history | `steerer/fleet/replay/` (`mike verifier-replay`) | Builds the same judge message and applies the same checks as of a past moment. |

## A new steerer

A steerer is a row in `steering.steerers` with a prompt, an invocation and a rollout. It runs
through `steerer/fleet/steerer_eval/`, so it already gets the limit, the need grade, the
session's dossier block, the note frame and the reply labeller. Add the row through
`POST /api/steerers`; start it with `config.rollout = "shadow"`. Its prompt must say when to
answer low, medium or high only where the general rubric is not enough.

Write no code for limits, earlier notes, operator reactions or wording. If the steerer seems to
need one of those written again, the shared piece is missing something: change the shared
piece.

## A new turn-guard question

Add the variant to `Question` in `steerer/guard/mod.rs` and give it a `need`. Its gate goes in
`steerer/guard/question_gates.rs`. Delivery already asks the note rule in
`steerer/guard/live.rs`; a refusal is recorded as the live decision `exempt` with the rule's
reason.

## New context for a judge

1. Add a variant to `SteererSection` and a name for it in `dossier/view/parse.rs`.
2. Say what it loads in `Needs::for_section` (`dossier/view/types.rs`). If it reads something
   the dossier does not load yet, add the load to `dossier/view/load.rs`, as of the moment.
3. Render it in `dossier/render/` as an `AllocSection`: a heading constant and a body. Use
   `SectionBody::List` for a list written oldest first and `SectionBody::NewestFirst` for one
   written newest first, so a cut drops the right end.
4. Add it to `every_section()` in `crates/mike/tests/integration/dossier_view/sections.rs`.
   That test fails if the narrow load renders differently from the full dossier.
5. To show it to every steerer's judge, add it to `session_sections()` in
   `steerer/fleet/session_block.rs`, and its heading to `SECTION_PREFIXES` in
   `steerer/fleet/replay/parity.rs`.

The turn guard's judge input (`steerer/guard/input.rs`) is fixed while its questions are
measured against a threshold fitted to that input. A change there is a change to the guard's
evidence and belongs with a new fit, not with a context change.

## A new producer of notes

Anything that would create a row in `steering.interventions` outside the two paths above must
ask `note_policy::refusal` first and record the reason when it is refused. Today two files
create such rows: `steerer/fleet/intervention.rs` and
`steerer/fleet/steerer_eval/guard_delivery.rs`.

## What is still separate

- The turn guard has its own cap on notes pushed into an idle session,
  `steerer.turn_guard.wake_cap_per_session_hour` (3 by default), applied when a waiting note
  comes due. Every such note already counted toward the session limit when it was planned, so
  at the default settings the session limit is reached first.
- The turn guard labels its notes from the operator's next message; the other steerers are
  labelled from the session's reply. Both write the same label column.
