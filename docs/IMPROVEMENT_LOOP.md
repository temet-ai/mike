---
title: The Improvement Loop
status: active
created: 2026-09-29
updated: 2026-09-29
summary: What mike does with a corrected turn — how a correction becomes a lesson, how a directive proposal is approved, what evidence the turn guard gathers, how an action's effect is measured honestly, and what the operator sees and does.
---

# The Improvement Loop

Every turn the judge labels `corrected` used to just sit in a table. This is what happens to it
now: it can become a lesson injected into later sessions, it feeds the turn guard's evidence, and
every action this produces is measured against its own before/after or holdback/treated window —
honestly, which mostly means saying "not enough data yet" until the sample really is enough.

Built overnight 2026-09-28 to 2026-09-29 (`artifacts/2026-09-29/judgement-at-work/PLAN.md` is the
design; `artifacts/2026-09-28/judgement-at-work/BRIEF.md` is the operator's original ask). This
doc is the plain-words summary for a later session or the operator; PLAN.md §§1-2 carries the
full reasoning and the alternatives that were rejected.

## 1. What the judge marks

The turn judge already labels every turn with a `reaction`: `accepted`, `redirected`,
`corrected`, or a few narrower classes. Nothing here changes that judging — this loop starts
from a verdict the judge already wrote (`memory.turn_verdicts`) and asks what to do with the
ones marked `corrected`.

A `corrected` verdict only becomes eligible for the next stage once its turn is fully resolved:
the operator's own correcting message has to exist and be captured
(`memory.turn_facts.next_state = 'resolved'`) — a correction the judge inferred from a turn that
never got a human reply back isn't enough evidence to act on.

## 2. Correction → lesson candidate

A new background worker, the **lesson proposer** (`crates/mike/src/lesson_proposer/`), runs from
the quiescence worker's tick — every fifth tick, alongside the existing guard labeller
(`crates/mike/src/quiescence_worker/`). Each tick it looks at corrected, resolved verdicts it
hasn't examined yet (`crates/mike/src/store/persistence/lessons/proposals.rs` does the selection;
`memory.lesson_proposal_evidence` records every verdict it has looked at, whether or not that
verdict produced a lesson).

**Gate before it even asks a model:** the verdict's own confidence must be at least
`judge_lessons.min_confidence` (default `0.7`). Below that, the verdict is recorded as examined
and nothing else happens.

**What it asks a model:** the operator's ask, the assistant's ending, the operator's correcting
message, the judge's own rationale, and up to three similar earlier corrections
(`semantic::Retriever`, so a repeated pattern shows up as reinforcement rather than noise). The
model is also shown the full text of the three closest Claude Code memory files and project skills
of the project, as data, and when one already states the rule it leaves `rule` empty and names the
file in its reason (`docs/PROJECT_KNOWLEDGE.md`, "What the lesson proposer sees"). The
model answers whether the correction generalises into a rule, and what kind it is
(`correction` / `convention` / `warning` / `preference`). If it answers "no" (`rule: null` —
the correction was specific to that one turn, not a standing rule), nothing is written beyond the
evidence row.

**Write-time decision:** before writing a new lesson, the proposer embeds the candidate rule and
looks up the project's own active lessons within cosine distance 0.25 (at most five;
`crates/mike/src/lesson_proposer/relate.rs`). For every accepted rule, with or without
neighbours, one model call (reserved from the same daily budget, role `lesson_proposer`) is shown
the new rule, every neighbour in full and the project's closest memory and skill text, and
decides the lesson's home (below) and, per neighbour, `store` (unrelated, keep both), `skip` (already said), `update` (same fact,
the new text wins), `merge` (complementary, one lesson replaces both) or `contradicts` (cannot both
be true). Distance alone never decides: "X is broken" and "X was fixed" are near-duplicates in
embedding space (#110), so the prompt tells the model to prefer `update` over `skip` and
`contradicts` over `update` when unsure. The decision is written as a `memory.lesson_relations`
row of method `write_time` and status `proposed`:

| Decision | What is stored | Relation |
|---|---|---|
| `store` | the new lesson | none |
| `skip` | no active lesson; the text is kept as an archived duplicate (`duplicate_of_id`), the correction counts as evidence for the neighbour (`consolidation_count`) | new → neighbour, `recurs` |
| `update` | one new lesson holding the model's replacement text | replacement → neighbour, `supersedes` |
| `merge` | one new lesson holding the model's replacement text | replacement → each merged neighbour, `refines` |
| `contradicts` | the new lesson, beside the neighbour | new → neighbour, `contradicts`, never applied |

Mike validates the model's answer: a decision about a lesson that was not shown is ignored, a
replacement that is empty, longer than `lessons.shape.max_chars` or not rule-shaped turns the
update or merge into a plain `store`, and a failed or refused model call leaves the saved
proposal pending for a backed-off retry instead of writing anything, so a model outage cannot
archive a source or create an untraceable duplicate. The relation's `evidence_json` carries the
verdict, the new source turn, and the related lesson's source, creation time, turn and session.

The nightly consolidation run (`crates/mike/src/lesson_worker/nightly.rs`) then does two things.
It confirms every `proposed` write-time relation whose lessons are all judge-written, once the
replacement has been promoted to `daily` or `long_term` (the same reinforcement rule judge
lessons already use; the evidence behind the replaced lessons counts toward it). A relation that
touches an operator lesson stays `proposed` until the operator confirms it on the existing
relation endpoint, and `contradicts` is never confirmed automatically. It then applies every
confirmed `supersedes`, and write-time `refines`, relation whose replacement is injectable:
the replaced lesson is archived as `superseded` with its text kept, its evidence rows are
re-pointed at the replacement, its other relations are copied onto the replacement, and the
relation's `evidence_json` records how many evidence rows moved and how many injections the
replaced lesson had (`moved_evidence`, `inherited_injections`). Injection rows stay on the
archived lesson; the relation is the link between the two histories.

**Rule shape:** a lesson is a durable instruction, not a status report. `record_lesson`, the
proposer and a scan on the proposer tick check every lesson (`crates/mike/src/lesson_shape.rs`):
over `lessons.shape.max_chars` characters (default 500), opening with a status word (Satisfied,
Left unmet, Done, Built and similar), naming a session uuid or an early commit sha as its
subject, or carrying a `verifier:` topic. A flagged lesson is accepted whole, recorded in
`memory.lesson_shape_findings`, held at `raw`, kept out of consolidation, auto-promotion and
injection, and the `record_lesson` result says why in its `shape` field. The shape pass
(`crates/mike/src/lesson_proposer/shape_pass.rs`, at most three lessons per tick, reserved from
the proposer budget) rewrites it into up to four rules with a `supersedes` relation of method
`rewritten` to the original, which is archived as `superseded` with its text kept. A
`verifier:` carryover note, or a status-shaped text the model finds no rule in, is archived as
`not_a_lesson` and written to its session's notebook as a `handover` note. Three failed attempts
leave the lesson as it is (`kept`) and release it, and so does the nightly run for any hold the
pass has not resolved within 14 days. With the `judge_lessons` toggle off nothing runs the pass,
so nothing is held: the lesson is recorded as `kept`, injected as written, and the
`record_lesson` result says the proposer is off. The pass applies to lessons already in the
store as well: an over-length `long_term` lesson keeps injecting until its rewrite replaces it.

**Placement:** a lesson has one home, and each home has one job so that none of them compounds
into text nobody can manage: a `rule` is a short constraint tied to file paths (proposed to the
operator for `.claude/rules/`, never written by mike); a `skill` is a procedure for a recognisable
situation; an `agent` is a persona, who the agent is and how it decides (proposed to the operator
for the agent definition, never written by mike); `memory` is a fact or correction recalled when
relevant; `nowhere` means a memory file or skill the proposer was shown already covers it. The
write-time call returns the home in the same answer as the store, skip, update or merge decision
(`placement`: `home`, `covered_by`, `reason`, `persona`), and it is stored on the lesson
(`memory.lessons.placement_id`, `placement_covered_by`, `placement_reason`). Mike checks the
answer: `covered_by` is kept only when it names a document that was shown, `agent` is kept only
when the model also says the text is persona (a how-to said to one agent becomes `memory`), and
`nowhere` is honoured only with a shown document that covers it, in which case the lesson is not
stored and the correction is kept as evidence naming the document. Lessons written without a
placement (`record_lesson`, rewrites, lessons older than the migration) are placed by the
placement pass on the proposer tick (`placement_pass.rs`, at most five per tick, from the same
budget, three failed attempts stop a lesson). The model is shown the project's closest memory
files and skills from the project-knowledge index, and its closest rules and agent definitions read
from the checkout on this machine (`file_guidance.rs`), so any of the four can be named as
covering a lesson.

Only lessons placed as `skill` feed skill drafts and the promoter's topic skills, so rule-shaped
and persona lessons never reach a skill, and an agent-scoped topic produces a `<topic>-<agent>`
skill only when its lessons are placed as skills and all belong to that one agent. The weekly
learning audit lists the `rule` and `agent` lessons in full as proposals for the operator.

**Skill budgets:** a project gets at most one mike-written skill per purpose. The purpose pass
(`purpose.rs`, on the proposer tick) moves the skill-placed lessons of a topic whose words overlap
an existing topic of the same project onto the surviving topic (the shipped one, then the larger),
so a new draft for the same purpose patches that skill instead of adding another; the promoter
holds back any overlapping topic that is still separate. The promoter
(`scripts/learning_audit/promote.py`) also keeps each project at or under
`lessons.skills.max_per_project` skills (default 12): before a new skill ships past the cap it
retires the shipped skill with the fewest loads in the last 30 days (then the oldest), never one
shipped in the last seven days, and holds the new skill back when none can be retired. The audit
reports lessons by placement and each project's skill count against the cap.

**Automatic promotion:** a lesson candidate starts at stage `raw` — it sits on the Improvement
page waiting for the operator. Once its backing evidence (the original correction plus every
reinforcing one) reaches `judge_lessons.auto_promote_min_evidence` (default `2`), the proposer
promotes it to `daily` itself, with a `config.change_log` row recording that the decision was the
judge's, not the operator's. A repeated correction is treated as strong enough evidence to act on
without waiting; a single correction only proposes. Setting this value to `0` makes every
candidate wait for the operator regardless of reinforcement; `1` promotes every candidate
immediately. All of this is on `/settings` under "Lesson proposer" (`judge_lessons.*`, defined in
`crates/mike/src/settings/meta_improvement.rs`).

**Cost control:** `judge_lessons.max_per_day` (default `40`) caps how many corrections the
proposer will examine in a day, each examination being at most one model call
(`LlmRole::LessonProposer` in the usage ledger). The two lesson maintenance passes, the rewrite of
over-long lessons and the placement of lessons, keep their own daily count in
`memory.lesson_proposer_daily_budget.maintenance_consumed`, limited by
`judge_lessons.maintenance_max_per_day` (default `400`), so a backlog of maintenance never uses up
the calls for new corrections. When a maintenance pass is refused a call, by that count or by the
call limiter, the proposer logs it at info level at most once an hour per pass and the
`llm_role_health` row on the home page names the pass, the time and the setting to raise.

The whole thing sits behind the feature toggle `judge_lessons` (default **off** — it sends turn
text to a model, so the default-off-mutation-and-egress rule applies;
`crates/mike/src/feature_toggles/mod.rs`). **This install turns it on** at deploy time, recorded
in the deploy's own report, not hardcoded into the toggle's default.

## 3. Approving a candidate, or a directive proposal

A lesson candidate at stage `raw` needs a decision:

- **Approve** (`POST /api/improvement/lessons/{id}/approve`, editable text) sets the lesson's
  stage to `daily` — the same stage the automatic-promotion path uses — so the very next
  `lesson_worker` snapshot includes it for injection into later sessions of that project.
- **Reject** (`POST /api/improvement/lessons/{id}/reject`) takes a reason:
  `not_a_correction`, `too_specific`, `wrong`, or `other`. Rejecting with `not_a_correction`
  additionally writes a **gold** reaction label on the turn (through the existing gold-label
  write path) — the judge was wrong to call it a correction at all, and that correction feeds
  back into how the judge itself is scored, not just this one lesson.

A **directive proposal** (an instruction the operator gave more than once, or a step he
confirmed was already authorised — `memory.directive_proposals`, unrelated to the judge) reaches
a lesson the same way once approved: `POST /api/directive-proposals/{id}/approve` writes one
lesson per target project, stage `daily` from the start (the operator already decided, so there
is nothing to consolidate first), links the lesson back to the proposal
(`memory.directive_lessons`) so the "What happened" feed can say which lessons a directive
produced, and logs `config.change_log`. Rejecting a directive proposal takes a free-text note and
archives it — no gold label, since a directive proposal was never a judge verdict to begin with.

Both approval paths are deliberately **not automatic**: PLAN §1.3 chose this middle ground
because the judge is right about 80% of the time on its own holdout, cheap when the judgement
only marks a turn, and expensive once it becomes a lesson that reaches every later session of a
project — the operator has said lessons were "creating a lot of noise" before, so one correction
proposes and only a repeated correction (§2's auto-promote rule) acts without him.

### What a fleet seat learned reaches the next run

A fleet seat can mark a session note `data.carry: true` (with `data.discipline` when the lesson
is for another discipline, and `data.line` for the one line to carry). mike makes it a lesson
candidate for the project and that discipline, tagged `carry:<slug of the title>`, and it waits
in the same approval queue as a correction (source `carry`). Approving it puts it, one line, into
that discipline's template when a later fleet is warmed. Beside each item the queue shows two
counts: the runs it was given to, and the later findings that carry its tag. An item whose count
of later findings keeps rising is a problem that came back. mike only counts: it marks nothing,
decides nothing from the counts, and the lesson worker never rewrites a carried line. See
[`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md) for the run record written when a fleet closes.

## 4. What evidence the turn guard gathers

The turn guard already asks a handful of questions on some turns (`relapse`,
`unobserved_done`, and others) and, when a question is live, can wake the model for a new turn
or add a note to its reply. Three things are new here — all about giving the guard's questions
an honest track record, not about changing when they fire:

1. **The labeller now scores live pass rows, not just holdback rows.** Previously only turns the
   guard *held back on purpose* got an outcome label from the next message. When
   `steerer.turn_guard.label_live_passes` is on, an ordinary turn the guard looked at and chose
   not to act on (`mode = 'live', status = 'resolved_pass'`) also gets labelled
   (`crates/mike/src/store/persistence/steering/guard/labels.rs`). Treated fires (the guard
   actually acted) stay excluded from labelling — the next message after an intervention is
   confounded by that intervention, so it can't honestly score the question's own precision.
2. **Every relapse evaluation records the evidence behind it.** In `record` mode (the only mode
   built so far — see §7), the relapse question's evaluation carries up to three similar earlier
   corrections (`semantic::Retriever`, the same mechanism §2 uses) as `verdict_json.semantic.relapse`.
   Nothing about the question's own gate or its input changes yet — this only makes the evidence
   visible for later.
3. **The operator can mark a delivered wake or append "right" or "wrong"** directly from the page
   that shows it (`PATCH /api/steering/evaluations/{id}/label`), which writes
   `label_source = operator` and feeds straight into that question's own precision.

**The guard obeys the limit on notes per session.** A session that has already received its
limit of steering notes inside the window (`steerer.session_note_limit`, three in an hour by
default, counting every steerer) is not looked at by the guard until the window moves on, so
those turn ends have no evaluation row in either arm. The fixed notes the guard sends were
reworded on 2026-10-05 to say what it could not see and to leave room for the work being done
already (`crates/mike/src/steerer/guard/nudges.rs`).

**Notes from the other steerers are labelled from the session's whole reply.** The reply labeller
waits for the turn that carried the note to end, reads everything the session wrote from the note
to the end of that turn, and replaces any label an older labeller version wrote in the last 72
hours. Version 1 read only the first request after the note, which graded a session that checked
something and then declined the note as having complied.

**Holdback, and why it moved from 10% to 25% on this install:** a guard question's randomised
holdback draw is its control arm — the honest way to know whether the nudge actually helped is to
compare the holdback arm against the treated arm (§5's read point: 34 holdback events per
question). Raising `holdback_pct` (default 10, this install 25 for `relapse` and
`unobserved_done` only, `crates/mike/src/steerer/guard/policy.rs`) increases the share of fires
held back, so the holdback arm reaches that read point faster — recorded as a settings change for
this install, not a new default (a fresh install still holds back only 10%).

## 5. Measuring an action's effect, honestly

Every lesson, directive and guard question gets an effect reading. The two kinds of action are
measured differently because they *are* different:

**Two kinds of action, two measures, one table.** A lesson or directive and a guard question are
never measured the same way, on purpose:

- **A lesson or directive** is observational — there's no holdback for it (a decision made in
  2026, #288: nothing splits which sessions get an approved lesson and which don't, so there's no
  control group to compare against). It gets a **before/after corrected share**: the corrected
  share of judged turns in the lesson's own project before it went live, versus after, reported
  with a 95% Newcombe (Wilson score) interval. The read point is at least 30 judged turns on both
  sides and at least 7 days elapsed — below that, the page says **"Not enough data yet"**, never
  a number.
- **A guard question** is randomised — every fire draws into a holdback or a treated arm before
  anyone sees the outcome. It gets **only** a holdback-versus-treated risk-difference estimate,
  with a bootstrap interval, read at 34 holdback events per question. A guard question is never
  given a before/after share, because **the treated arm is confounded by the nudge itself**: the
  whole point of the nudge is to change what happens next, so "after the guard started firing"
  and "before" aren't comparable populations the way a lesson's before/after is — only the
  randomised holdback arm gives an honest control.

Either way, below its own read point the actions feed says "Not enough data yet" with the sample
size, never a bare percentage; above it, a lesson/directive reads "fewer corrections" only when
its interval excludes zero, and a guard question's holdback comparison reads the same way against
its own bootstrap interval. The page states plainly that a lesson/directive reading is a
before/after comparison, not a controlled trial — other things can move in the same window; a
guard question's reading is the one place in this whole loop that *is* a controlled comparison.

The weekly corrected-share trend (overall and per project, from the existing `eval.rates`
`base_rate` rows) sits alongside the actions on the same page, with a marker for the week each
action happened — but it is a trend, not a claimed effect, and the page says so once, not per
row.

## 6. Where this lives, and what the operator does

**Sessions › What the judge did** (`/sessions/improvement`) is the new landing page: what
mike did with the turns you corrected, what still waits on you, and whether you're being
corrected less. Three sections: **Waiting for you** (lesson candidates, directive proposals,
guard wakes with no verdict yet — approve, reject, or answer them right there), **What
happened** (the last 7 days, in full sentences, never a bare count), and **Is it getting
better** (the trend and the per-action effect table).

**Sessions › What the judge did › Guard** (`/sessions/improvement/guard`) is one panel per
turn-guard question: its mode, how often it fires, how often it's right, the holdback arm and
effect, relapse-evidence progress, and every wake/append it has delivered with a "was this
right?" control.

**Sessions › What the judge did › One lesson** (`/sessions/improvement/lessons/[id]`) is a
single candidate or lesson: the rule (editable before approval), the turn it came from, similar
and reinforcing corrections, and — once it's live — its injection count and effect.

**Sessions › Judge** (`/sessions/labelling`, unchanged URL) stays the page for how accurate the
judge is and what it costs; it now carries a one-line banner pointing at "What the judge did" and
a cost card. **Review labels** is gone from the navigation entirely — label review now only
happens where a label decides something (a lesson candidate, a guard wake), never as a queue of
its own.

## 7. What this deliberately does not do yet

- **No-project directives (J7).** A directive proposal with no associated project is a small
  share of the total and the operator hasn't decided the policy for it yet (blocked on issue
  #297's default-off kill switch for automatic artifact injection). Approving a no-project
  directive is not built.
- **`input` mode for relapse evidence.** §4's relapse evidence is `record` mode only — it's
  written onto the evaluation for later reading, but nothing yet feeds it into the guard's own
  gate or the model's input. That's a pre-registered change: it waits for 14 days of rows and 30
  labelled would-fires on each side, with a Newcombe lower bound above zero, before the evidence
  is trusted enough to act on rather than just record.
- **Session-judge gating.** The session-level judge (as opposed to the turn judge) isn't trusted
  to drive any action here — it has no passed accuracy gate yet (only one session-end holdout
  batch exists, n=15, too few to measure). It's display and guard-label evidence only until that
  changes.

## 8. Settings this install changed from their shipped default

| Setting | Shipped default | This install |
|---|---|---|
| Feature toggle `judge_lessons` | off | on |
| `steerer.turn_guard.label_live_passes` | off | on |
| `steerer.turn_guard.holdback_pct` (relapse, unobserved_done only) | 10 | 25 |
| `semantic.relapse_evidence` | record | record (unchanged) |

Every one of these is a live `/settings` value, changeable by the operator at any time; none of
them are hardcoded into a new default for a fresh install.
