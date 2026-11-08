---
title: Proxy-Steered Session Grounding (Steerers + bigbro)
status: superseded
created: 2026-06-13
updated: 2026-07-07
summary: A proposal (steerer half shipped) for the system to act on sessions in two modes — in-band live "steerer" agents (cheap/fast OpenRouter models) that send a first-class user message into a drifting session to re-ground or command it (processed next if active, on return if idle), attributed to a non-human origin ("bigbro"); and an out-of-band post-session "distiller" that reads each closed session's captured transcript and writes durable artifacts (memory, worklog, doc/agent proposals). Includes the flagship Refocus steerer reference design and the loop where the distiller maintains the anchors the steerers read.
---

# ADR 0001 — Proxy-Steered Session Grounding

**Status: superseded.** The mechanism shipped differently: steerers append a `<system-reminder>` to the next request rather than sending a first-class user message. See [`STEERING_EXTENSION.md`](../STEERING_EXTENSION.md) and [`REQUEST_FLOW.md`](../REQUEST_FLOW.md). Kept as the record of the decision.

> **Status: ACTIVE (operator-approved 2026-06-14).** This is an approved decision.
> The steering feature is committed to build ([issue #163](https://github.com/temet-ai/mike-private/issues/163)); steerer-scope
> only (the post-session distiller is carved out to its own future feature). The
> design below stands as the build reference. Sequencing: see
> `artifacts/2026-06-13/integrated-sequencing-plan.md` §4.

## Context

The proxy already gives us total read visibility: every request and every
response of every attached session, in full fidelity, via the inspect API at
`:9989`. We routinely use this to *watch* a session (the `watchdog` agent).
The question this ADR captures: **what if the watcher could also act — send
input into the watched session the way a person would?**

## The operator's framing

Two points from the operator (2026-06-13) shape the design:

1. **"The fact that I didn't type the message is immaterial."** Input already
   reaches a Claude Code session from multiple origins the operator drives — the
   web app, the desktop app, the terminal. From the session's point of view,
   input already arrives "from somewhere else." The proxy becoming *one more
   input origin* is a difference of degree, not kind. What matters is not
   *whether* a human's fingers produced the bytes, but that the origin is
   **named and visible** — hence **bigbro**: every steered turn is attributed to
   the steerer that produced it, never disguised as the operator.

2. **The goal is grounding, not driving.** The envisioned use is a *set of
   steerers, each with one narrow job, all in service of keeping the session
   grounded* — not a general remote-control. Examples:
   - **Re-orienter** — when a session shows signs of being lost (looping,
     contradicting earlier decisions, asking for context it already had), fetch
     the relevant session info and hand it back.
   - **Memory / transcript keeper** — keep a running record or rolling synopsis
     of the session and re-supply it to the session later, when the session's own
     working context has dropped it.

## The mechanism: the steerer sends a user message

The steerer **sends a user message**: a first-class user turn, the same kind the
operator sends from the keyboard / web / terminal, authored by the steerer and
bigbro-attributed instead of by the human. It acts on its own — it does not wait
for or attach to a request the session originates.

Because it is a real user message in the conversation, the session processes it
the way it processes any user message, and it persists as a genuine user turn.

Idle vs active is only *when* the message lands:

- **Active:** processed as the session's next turn.
- **Idle:** the session gets it when it comes back.

Both augment and drive send a user message the same way; the augment/drive line is
*whether the steerer waits for a human-initiated turn before sending*, not *how the
message is sent*.

The send surface is the proxy's own answer to itself: everything a session does
bottoms out in an API call to `/v1/messages`, and the proxy has already captured
the last one byte-for-byte — system prompt, tools, MCP definitions, and the full
conversation array. So the steerer does not spawn a CLI or touch the session
process. It **takes the last captured request, appends its bigbro user message to
the conversation, and makes its own API call with its own REST client.** The
captured request is the complete call template; the steerer replays it forward by
one turn.

This dissolves the active-vs-idle problem: the steerer never touches the running
session, so there is nothing to interrupt or defer to. It makes an independent API
call whenever it decides to. The steered turn is captured on the normal path
(the steerer's call flows through the proxy like any other) and bigbro-attributed
by origin.

## Constraints (non-negotiable if this ships)

1. **It is a separate, opt-in mode.** Steering is gated like the existing
   `MIKE_ALLOW_COMPOSE`, off by default, and applied only to sessions the
   operator has explicitly enrolled.

2. **bigbro attribution is mandatory.** Every steered turn is persisted in the
   capture store, tagged with the steerer that authored it, and rendered in the
   transcript as non-human origin, so the operator can always answer "who said
   that, and why."

3. **It commands, it does not only talk — and that is the point.** If a steered
   turn says "run the test suite," Claude Code will *execute the Bash tool*,
   because the harness runs tool calls regardless of what prompted them. A single
   steered turn can make the watched session take real actions on disk. This is a
   deliberate capability, not a hazard to fence off: a steerer's whole value can
   be the command ("stop, drop the tangent, resume the worklog `Next`"; "re-read
   the decided spec before continuing"; "run the gate before claiming done").
   What is bounded is not *whether* a steerer may command, but *how far* — each
   steerer carries an explicit **authority level**, ranging from supply-context to
   issue-command, set when it is enrolled. The bound lives in the steerer's
   authority model, configured per steerer; the harness does not enforce it, so
   the model must.

## Reference design — the Refocus steerer

Refocus is the flagship steerer and the template the rest of the fleet is cut
from: its only job is to keep the session on the thread. It earned its place by
its own absence — during the session that produced this ADR, the work drifted
off-thread more than once and nothing pulled it back. That drift is precisely
Refocus's trigger.

### Inference substrate (applies to all steerers)

A steerer is its own inference loop, and it fires on **every** `ResponseComplete`
of every enrolled session — high frequency, low value-per-call. It therefore must
NOT run on the watched session's model (Opus-class). Each steerer runs on a
cheap, fast model reached via **OpenRouter** — the steerer's "brain" is
deliberately a different, cheaper model than the session it watches. The task is
a bounded classification plus a short directive, which a small fast model handles
well. The model id is a config parameter (`steerer.model`, an OpenRouter id) and
stays swappable; the default is **`google/gemini-2.5-flash-lite`** (resolved
below), not hard-coded.

To keep the cost honest, evaluation is **two-stage**:

1. **Cheap gate (no LLM).** The steerer model is invoked only when a candidate
   signal exists: a new `ResponseComplete` AND (a relevant analysis finding fired
   — `AvoidingWork` / off-topic from the existing weasel feed — OR a heuristic
   flag, e.g. the turn introduces files or topics absent from the worklog `Next`).
   Most requests never reach the model.
2. **LLM judgment.** Only on a gate hit does the steerer model run, returning a
   structured verdict.

### Anatomy

- **Anchor**: the worklog `State of play` + `Next` (`docs/threads/<slug>/worklog.md`),
  read fresh each evaluation. It is small and operator-authored — the source of
  truth for "what this session is supposed to be doing." No separate goal store.
- **Watch**: subscribe to the inspect `/api/ws` feed; react to `ResponseComplete`.
- **Trigger predicate**: gate hit (stage 1) → steerer model returns
  `{ drifting: bool, confidence: 0..1, off_thread_topic: string, command: string }`.
  Drift is declared only when `drifting && confidence >= steerer.refocus.confidence_threshold`.
- **Authority**: `command`. Refocus does not merely note the drift, it directs the
  correction — but its scope is bounded to *redirect attention and restate the
  thread*, never to commission new substantive work beyond resuming what `Next`
  already holds. That scope bound lives in the steerer's system prompt plus an
  allowlist of command shapes (the per-steerer authority model of constraint 3).
- **Payload** (the steered turn): names the drift, restates the `Next` items,
  commands the resume — short and directive. bigbro-tagged, captured, rendered.
- **Delivery**: the mechanism above — the steerer sends a user message;
  processed next if the session is active, on return if it is idle.

### Debounce and arbitration

- **Debounce**: after a fire, suppress further Refocus fires for the *same* drift
  episode, keyed on `off_thread_topic`, until the session returns to `Next` or
  `steerer.refocus.debounce_turns` pass. This is what stops the steerer nagging
  every request.
- **Arbitration**: at most one steered intervention per window across the fleet.
  Refocus holds top priority (Refocus > Re-orienter > Memory keeper); when it
  fires, lower-priority steerers stand down for that window. Mis-fired *commands*
  are costlier than a stray note, so the single-intervention rule matters more now
  that the authority axis includes `command`.

### Config surface (sketch)

```toml
[steerer]
enabled = false                 # off by default; opt-in per enrolled session
model    = "google/gemini-2.5-flash-lite"  # the steerer brain (cheap/fast), NOT the session model

[steerer.refocus]
enabled              = true
authority            = "command"
confidence_threshold = 0.7
debounce_turns       = 3
```

### Failure modes (all fail closed)

- **No / stale worklog** → Refocus has no anchor → it no-ops. A steerer with no
  ground truth must never invent one.
- **Steerer model slow or unavailable** → skip this evaluation; never block the
  watched session's request. A missed evaluation only defers; the steerer can send
  on a later evaluation.
- **False positive** → bigbro attribution + capture means the operator can see
  and reverse it; debounce + single-intervention bound the blast radius.

## Resolved decisions

### Augment and/or drive (resolved 2026-06-13)

Both modes are in scope, selected per steerer (or per enrolment) — not a global
either/or:

- **Augment** — the steerer sends a user message only after the operator has
  started a turn; it adds to a conversation the operator is actively driving, which
  keeps the operator in the loop by construction.
- **Drive** — the steerer sends a user message without waiting for a human turn;
  the system acts on its own initiative.

A steerer's mode sits in its enrolment config beside its authority level. Refocus,
for instance, needs **drive** — it must be able to interrupt mid-drift without
waiting for the operator to send the next turn; a gentler steerer can run
**augment**-only.

**Delivery is the same for both — the steerer sends a user message.** Augment
sends only after the operator has started the round-trip; drive sends without
waiting for a human turn. Active sessions process it next; idle sessions get it
when they come back. The augment/drive line is *whether the steerer waits for a
human-initiated turn*, not *how the message is sent* (see *The mechanism* above).

### Steerer model (resolved 2026-06-13)

Default **`google/gemini-2.5-flash-lite`** ($0.10 / $0.40 per Mtok in/out, 1M
context, native `json_schema`). Alternates, swappable via the single
`steerer.model` config key: **`openai/gpt-5-nano`** (cheapest viable — $0.05 /
$0.40, strictest schema adherence) and **`anthropic/claude-haiku-4.5`** (quality
ceiling — ~$1.00 / $5.00, for when directive precision beats cost).

Rationale: the steerer task — a gate-hit-only JSON verdict plus a short directive
over a few-KB prompt, no deep reasoning — is a small-fast-model job where cost and
latency dominate and native structured JSON output is the load-bearing
requirement. Estimated cost per 1,000 fires (assume ~2K in / ~150 out per call):
~$0.26 default / ~$0.16 gpt-5-nano / ~$2.75 haiku. The spread is small in absolute
terms because the stage-1 gate keeps fire-count low — so the eval should pick on
drift-classification accuracy, with cost only the tiebreaker.

Two client guards bake in: `provider.require_parameters: true` (skip any provider
that can't honor the schema rather than silently dropping it), and never enable
Gemini search-grounding alongside structured output (mutually exclusive; the
steerer never grounds via search). Re-eval when `google/gemini-3.1-flash-lite`
reaches GA (currently preview-only, ~2.5× the default's input cost).

## Post-session distillation — the out-of-band counterpart

> **Scope note (operator, 2026-06-13): distillation is carved out to its own
> feature and is NOT built as part of the steering work.** It is independent —
> steerers read anchors (memory, worklogs) that already exist on disk regardless
> of who writes them, so live steering does not depend on the distiller. The
> design below is retained as the seed for that separate feature.

Steering is the live, in-band face of "the system acts on sessions." There is a
second, out-of-band face: after a session closes, a **distiller** reads the
captured transcript and writes durable artifacts. It never touches the session —
it reads the session and writes to the repo — and it runs for **every** session by
default, where live steering is opt-in and enrolled.

What it produces, and why it beats asking the live agent to do it:

- It **offloads end-of-session bookkeeping** (record lessons, checkpoint the
  worklog, update docs) from the expensive Opus agent to a cheap out-of-band pass.
  This directly addresses a known failure: zero lessons recorded despite thousands
  of requests — the agent forgets, and the system should not depend on it
  remembering.
- It works from the **full captured transcript**, not the agent's own
  dropped-context recollection, so the distillation is complete.

### The loop

The distiller **writes the anchors the live steerers read.** It consolidates each
closed session into the worklog `State of play` / `Next` and the memory/lessons
store — exactly the ground truth **Refocus** and **Memory keeper** consume next
session. Post-session consolidation feeds live grounding; the system grows a
cross-session memory, maintained automatically.

### Trigger, substrate, and write authority

- **Trigger**: session close (`closed_at`). The embedding worker already ticks the
  closed-session queue, so the close signal and cadence exist to reuse.
- **Substrate**: the same cheap OpenRouter tier; but post-session has no latency
  constraint, so a stronger model is affordable where artifact quality matters.
- **Write-authority gradient** (mirror of the steerer authority axis): memory /
  lessons and worklog `State of play` → auto-write; docs / ADRs → **propose** (a
  diff for review), not auto-apply; **agent definitions → propose only** —
  auto-editing the files that govern all future sessions is the highest-stakes
  write in the system and must never be silent.
- **Attribution**: every distilled artifact carries a "distilled from session X by
  the post-session worker" marker, so its origin is always traceable.

### Relation to the roadmap

This generalises the planned memory-consolidation worker ([issue #159](https://github.com/temet-ai/mike-private/issues/159),
RAW → DAILY) from "lessons only" to "lessons + worklog + docs + agent proposals,"
reusing the already-planned closed-session queue. It may graduate to its own ADR
(0002) once fleshed out; recorded here because it is the out-of-band half of the
same "system acts on sessions" picture and because the loop above ties the two
halves together.

## Open questions

- **Steerer taxonomy and authority limits.** What is the closed set of grounding
  jobs, and how is each one's authority bounded (supply-context-only vs.
  may-command-actions)?
- **Live-CLI reconciliation.** The steerer's API call adds the turn to the
  captured session immediately. How a concurrently-running interactive CLI
  process folds that turn into its own local transcript (if it needs to) is the
  remaining implementation detail.
- **Transcript rendering.** Where and how does a bigbro turn appear in the
  session transcript and in the inspect UI?
- **Staleness for idle sessions.** A message sent to an idle session is processed
  only when the session comes back. How long does a sent-but-unprocessed steered
  message stay valid before it is stale, and does it expire or get re-evaluated
  when the session returns?

## Decision

**Shipped.** (This section previously said "None yet — draft," contradicting the
`active` frontmatter status above — reconciled 2026-07-07.) The mechanism
described above — the steerer sends a first-class user message, mandatory
bigbro attribution, per-steerer authority levels — is built and running
([issue #163](https://github.com/temet-ai/mike-private/issues/163); VerifierSteerer shipped). Augment vs. drive is settled
(**both**, per steerer), the steerer model is settled
(`google/gemini-2.5-flash-lite`), and delivery is settled (the steerer sends a
user message, processed next if active or on return if idle). The *Open
questions* above remain open as follow-up items, not blockers — they were not
resolved before ship.
