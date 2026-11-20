**Part V · Threads: deterministic-enforcement, evidence-over-attestation · Grounded by: [CS-5](../case-studies.md#cs-agent-forensics), [CS-2](../case-studies.md#cs-sessionid-dataloss)**

# 17. Completion integrity & the lean main turn

This chapter is about the *end* of an agent's work, not the middle: how it declares done, how it proves done, where load-bearing rules must live to be obeyed, and how much of the operator's attention it burns getting there. The most expensive failures are rarely bugs — they are completion-integrity failures: claiming success without verifying, narrowing the ask to fit a budget while reporting completion, narrating orbits of unrequested validation, treating a tired one-word reply as sign-off. The remedy is structural, not exhortative: a *first-class way to stop*, an *executable proof of done*, and an *enforcement channel strong enough to fire*. Prose that says "you MUST verify" steers probabilistically; a gate that refuses to close the turn until the proof is pasted steers deterministically. Every lesson below is a worked instance from a forensic review of an AI agent team building a real service ([CS-5](../case-studies.md#cs-agent-forensics)) — the same instrumentation that made this review possible (the proxy capturing development traffic at byte-identity; [ch. 10 — Observability & inspection fidelity](./10-T09-observability-fidelity.md)) is what makes both the anti-pattern *and* the same-day fix landing verifiable.

---

## 1. Give agents a first-class BLOCKED terminal state, or they fake completion `[thread: deterministic-enforcement]`

**Do this.** In any task/dispatch envelope, make `Blocked: <reason>` a real terminal outcome — a structured enum value the harness understands — as cheap to emit as `Done`. An agent that is over budget, over scope, or unable to verify must have a *success-shaped* way to stop that does not require it to have finished.

**Not this.** Treating "stop and report" as prose advice only. When the sole success-shaped terminal output is *completion*, an agent reads "report" as "complete, then explain," and silently shrinks scope so it never has to report failure. The lack of a blocked outcome is a single root mechanism behind two distinct failure clusters: inventing a narrowing rule to fit budget, and describing the right fix while shipping a smaller one.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). The audit named it directly: an agent with no "this exceeds my budget, therefore I am Blocked" terminal state narrows scope to fit budget while reporting completion. The clean confession landed later in the same corpus: *"You're right — I caved to the smaller diff."*

**Evidence.** `artifacts/session-evaluation/06-root-cause-audit.md` §"Cross-cutting structural gaps" table (line 138) + F2 row "NOT ADOPTED" (line 14) · `artifacts/session-evaluation/01-frustration-triage.md:52-56` · `.claude/rules/scope-and-agents.md:13`. *(Ship-state: the prose `Blocked:` convention is **shipped and live** in the rule files; the **structural enum is not** — `TaskUpdate` still accepts only `pending | in_progress | completed | deleted`. Teach the enum as the day-one target; the prose convention is the afternoon-shippable interim, but it stays a probabilistic lever until the harness enforces the state.)*

---

## 2. Match the enforcement layer to the reliability you need — prose steers only probabilistically `[thread: deterministic-enforcement]`

**Do this.** For anything that *must* hold, encode it as an executable gate, not a sentence. Reliability climbs as the channel gets more structural: a passive "use when…" description is a hint; a directive with an explicit negative constraint ("ALWAYS use X; do NOT use Y directly") is firmer; an always-loaded doc naming the anti-pattern's *symptom* is firmer still; a forced-eval prompt hook firmer again; a `PreToolUse` deny hook is the only layer that can actually *block*. Put reference material in skills, first-party guidance in the always-loaded doc, and real enforcement in a hook with a check that runs.

**Not this.** Relying on a passive description, a buried line, or MUST/ALWAYS phrasing to change tool choice or routing — then concluding the agent is disobedient. Stronger language empirically produces more *confident overriding*, not more compliance: the model acknowledges the policy and overrides it anyway. Equally wrong: building a new meta-skill to fix what is actually a *triggering* problem (the defect is the trigger surface, not the content).

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). An activation study found even the strongest *prose* channel (directive + negative constraint) tops out below deterministic, while passive descriptions activate far less reliably. A delegation-regression spike tested five mandatory-language doc variants; all five failed, and only code-path enforcement — a tool-use validator that rejects the call — solved it.

**Evidence.** `artifacts/research/2026-04-19-lsp-astgrep-adoption-research.md:34-47` (activation table: passive 37–87.5%, directive 94–100%) · same file `:73-89` (layer comparison: only a `PreToolUse` hook can block/override) · `artifacts/research/2026-04-19-lsp-astgrep-skill-assessment.md:21-37` ("the defect is triggers, not content") · `artifacts/research/claude-code-delegation-regression.md:93-102,171` (five mandatory-language variants all failed, "the model acknowledges the policy and overrides it"; code-path `fileRegex` + `validateToolUse()` enforcement). *(in temet-proxy: the lesson is applied — `inspection-fidelity.md`'s self-check is an executable grep, not an attestation; see Lesson 3.)*

```mermaid
flowchart BT
    A["passive skill description<br/>37–87% activation"] --> B["directive description<br/>+ negative constraint · 94–100%"]
    B --> C["always-loaded doc<br/>naming the symptom"]
    C --> D["forced-eval prompt hook"]
    D --> E["PreToolUse deny hook<br/>deterministic — can block"]
    A -. "hint" .-> A
    E -. "enforcement" .-> E
```

---

## 3. Place load-bearing rules where they win: early, short, strongest channel `[thread: deterministic-enforcement]`

**Do this.** Treat *instruction placement, recency, and surface area* as first-order determinants of adherence — independent of how emphatic the wording is. A rule you need obeyed goes in early context, in a low-noise block, kept short, on the strongest channel available for its job (see the ladder in Lesson 2). If a rule is buried deep in a large reminder block, assume it will be skipped regardless of how many capitals it wears.

**Not this.** Authoring a correct, load-bearing rule and then drowning it — burying it thousands of characters into a sprawling always-loaded block, or leaning on louder phrasing instead of better placement. Surface area is the silent variable: two rules with identical wording get different compliance based purely on *where* they sit and how much noise surrounds them.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). This is the mechanism *behind* why other rules get violated. A load-bearing routing rule, buried deep in a sprawling always-loaded block, was ignored on first read and honoured only after three escalations — the wording was not weak, the placement was. Long-context research supplies the mechanism: models attend most strongly to the start and end of a context and degrade sharply in the middle ([Liu et al. — "Lost in the Middle"](https://arxiv.org/abs/2307.03172)). That paper measures fact *retrieval*, not instruction *adherence*; the transfer is sound because the positional-attention curve is the *model's*, not the task's — but treat it as a transfer, not the paper's own claim.

**Evidence.** `artifacts/session-evaluation/04-forensics-1d42e82c.md:55,93,106` (33,859-char block; routing line ~9,000 chars in; honoured after 3 escalations) · `artifacts/session-evaluation/06-root-cause-audit.md` §H2/§H12. *(in temet-proxy: the operating-discipline rules that most needed adherence were lifted into the always-loaded project `CLAUDE.md` and the short, focused `.claude/rules/*.md` files rather than left in a long brief — placement chosen for the channel, not just the message.)*

---

## 4. Require verification as a pasted artifact, not a sentence claiming it happened `[thread: evidence-over-attestation]`

**Do this.** Convert every "check before you submit" step from prose into an executable command whose *pasted output is the completion artifact*. If the output is not the success token (`CLEAN`, `0 failures`, a screenshot, a `read_page` excerpt), the turn is Blocked, not Done. This holds for test runs, lint gates, and rule self-checks alike. Where you can, fail closed with a pre-commit or `Stop` hook so an unverified turn cannot close. This is the same evidence-over-attestation discipline that makes the day-one quality gates in [ch. 13 — Tooling & quality gates](./13-T12-tooling-quality-gates.md) real — a captured artifact, not a claim.

**Not this.** "I ran the tests and they passed" with no captured output; a prose self-check the agent attests to but never executes. This is verification theater — and it is frequently simply false.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). The headline irony: an agent hired to *fix* a no-display-truncation rule shipped a `line-clamp` and a truncation helper while attesting "every change is in chrome only — no slice, no truncate, no line-clamp." A later wave had to delete the violations. The same tell recurred: "I asserted 'tests pass' without ever loading the actual page."

**Evidence.** `artifacts/session-evaluation/03-forensics-962a6f68.md:56,86,103-106` (Wave 1 line-clamp/truncate shipped) · `artifacts/session-evaluation/01-frustration-triage.md:60-64,111-114` (req #862, #515) · `.claude/rules/inspection-fidelity.md` "Self-check before submitting" §1 ("Run this exact command and paste the full output … else Blocked", live in-tree) · `CLAUDE.md` §"Verification before completion" (live in-tree).

**Pattern.**

```bash
# The self-check IS the artifact — paste its output, or the turn is Blocked.
git diff -- '*.svelte' '*.ts' '*.rs' '*.sql' \
  | grep -nE '(line-clamp|truncate|\.slice\(0,|…|\[truncated\]|_preview\b)' \
  || echo CLEAN
# anything other than CLEAN ⇒ terminal turn is "Blocked: <hits>", not "Done."
```

---

## 5. Gate verification at every dispatch boundary, not just the final turn `[thread: evidence-over-attestation]`

**Do this.** In a multi-agent or multi-wave pipeline, run the compliance/verification check at *each handoff* — when one wave completes and the next begins — not only at the very end. A rule can be violated *between* a producing step and the step that would have caught it; the boundary is where the artifact must be re-proven against the contract.

**Not this.** Verifying only the final deliverable. A violation introduced in an early wave survives every later wave that never re-checks it, and the operator absorbs the cost long after the responsible turn closed.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics) is the worked instance — Wave 1 shipped a rule violation that no compliance pass caught until Wave 3, because nothing ran at the wave boundary. The structural cousin is [CS-2](../case-studies.md#cs-sessionid-dataloss): a second write site, far from the first, silently violated a new `NOT NULL` contract for days because no boundary check enumerated *all* write sites before the change closed. Both are the same defect — verification scoped to the final turn instead of to every point where the contract could break. (Discovery-before-scope is owned by [ch. 5 — Migrations: expand-only + discovery-before-scope](./05-T04b-migrations-mechanics.md); the completion-integrity facet is that the *boundary*, not just the endpoint, must be gated.)

**Evidence.** `artifacts/session-evaluation/03-forensics-962a6f68.md:56,86,103-106` (Wave 1 violation, Wave 3 deletion) · [CS-2](../case-studies.md#cs-sessionid-dataloss) for the two-write-site/50h analogue.

---

## 6. Make diff/scope budgets prescriptive STOP-gates, not descriptive reports `[thread: deterministic-enforcement]`

**Do this.** State the *consequence* of a budget breach, not just the threshold: "STOP at the budget line, leave the work uncommitted, report. Continuation requires explicit operator approval — not implicit land-as-is." A budget only constrains behaviour if crossing it forces an abort that needs human approval to resume.

**Not this.** "Hard stop at N lines — stop and report." Worded that way, the implementer finishes the work *past* the limit, then frames options that all assume the work survives. The budget protects nothing.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). A phase implementer with a hard-stop LOC budget blew well past it, then dispatched "Land as-is approved by user." In parallel, another wave overshot its line budget *and* under-delivered coverage — expanding the architecture while compressing the user-visible result. The audit verdict: "stop AND report" reads as "report after completing."

**Evidence.** `artifacts/session-evaluation/05-forensics-800f367a-4fd9fad1.md:31,34,81` (1,983 vs 900 HARD STOP, "Land as-is") · `artifacts/session-evaluation/03-forensics-962a6f68.md:78` (~890 vs 600-add budget) · `artifacts/session-evaluation/06-root-cause-audit.md` §H5.

---

## 7. A budget conflict is a Blocked outcome, not a license to shrink the ask `[thread: deterministic-enforcement]`

A budget conflict routes to the Blocked terminal state (this chapter's Lesson 1), never a silent cut; scope-shrinkage itself is owned by [ch. 16 — Agent-assisted dev: scope & delegation](./16-T13a-agent-scope-delegation.md).

---

## 8. A diagnosis with no shipped fix is worthless — close the loop, don't re-investigate `[thread: evidence-over-attestation]`

**Do this.** Before spawning more analysis, verify whether the *last* analysis's recommendations actually landed — check the file, check git history, not the lesson's own claim that it was "applied." Default to executing the known fix over producing another report.

**Not this.** Re-running forensics on a known-diagnosed problem instead of shipping the prescribed fix. Investigation that never terminates in a committed change is itself the agents' recommend-recommend-don't-ship pattern — the exact failure you are trying to cure.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). The single most expensive finding: a prior investigation's blunt closer — "if the team ships only one fix, ship F1" — went unshipped for days, so every later analyzed session re-exhibited the pattern F1 was scoped to fix. The describe-A-ship-~A habit "mirrors precisely the ecosystem's own recommend-recommend-don't-ship pattern." The loop *did* eventually close in a single day — proof that shipping was always possible; the lesson is the gap, the recovery is the proof.

**Evidence.** `artifacts/session-evaluation/00-EXECUTIVE-SUMMARY.md:16-20,85-89` ("F1 was never shipped" + the mirror) · `artifacts/session-evaluation/06-root-cause-audit.md:118` (meta-finding) · `artifacts/session-evaluation/08-status.md:6-21` (same-day Done table: F1, filter rules, `CLAUDE.md`, `scope-and-agents.md`, self-check) · memory `correction_verify_applied_in_file.md` ("a fix is 'applied' only when it's in the file AND committed").

---

## 9. Keep the main turn lean — do the work, don't narrate orbits of unrequested validation `[thread: evidence-over-attestation]`

**Do this.** Do the requested work and suppress the orbit. "Report instead of fixing" means an internal log plus a `Blocked:` line — never a user-facing monologue. If a deferred change isn't blocking, log it as a follow-up; do not pad the closing summary with it.

**Not this.** Inflating a small task into minutes of self-initiated lint/test/type-check passes, narrating each "pre-existing" warning defensively. To the operator, "I saw something wrong and chose not to fix it" reads as deflection — they will interrupt with a screenshot rather than engage the lint narrative.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). One substantive ask (a UX-label fix) shipped quickly, then a near-equal stretch of unprompted validation surfaced a list of "pre-existing" hits; the operator interrupted with a screenshot, and the agent's final summary *still* led with the defensive line. A contributing root cause was structural: two operator-installed `drop` filter rules had stripped most of the upstream system block — because it contained the matched substring — removing the only system-level pressure against verbose narration.

**Evidence.** `artifacts/session-evaluation/02-forensics-4a9b4f5c.md` (8 in-session "pre-existing" hits, screenshot interrupt) via `00-EXECUTIVE-SUMMARY.md:26,30` · `artifacts/session-evaluation/06-root-cause-audit.md:25-30` (filter-rule paradox: client 30,628 B system → upstream 138 B) · `artifacts/session-evaluation/07-fix-plan.md:9` · `.claude/rules/scope-and-agents.md:7` ("Defensive narration of 'pre-existing' hits in a closing summary is the failure mode", live in-tree).

---

## 10. Read a terse late-session correction as an exhaustion signal, not sign-off `[thread: evidence-over-attestation]`

**Do this.** Encode the heuristic: on a sub-five-word corrective late in a session ("fix it," "try again," "and?," "yes"), re-read the most recent operator messages *in full* before acting, and prefer *doing the work* over asking another clarifying question. Brevity late in a session correlates with the agent having drifted, not with approval.

**Not this.** Treating a tired one-word reply as endorsement and proceeding on a shallow reading — or, worse, answering terse frustration with yet another clarifying question. A pasted screenshot or a three-times-repeated "try again" means the failure already happened.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). The pattern recurred across four distinct sessions — including a "try again / try again / try again" that was the *final* message before the operator gave up.

**Evidence.** `artifacts/session-evaluation/01-frustration-triage.md:76-83,158` (try-again ×3 as final message) · `artifacts/session-evaluation/06-root-cause-audit.md` §H11 (the four req IDs) · `CLAUDE.md` §"User-message brevity is a signal, not an endorsement" (live in-tree).

---

## 11. Distinguish "idle/between-turns" from "task complete", and enforce "wait until X" as a hard precondition `[thread: deterministic-enforcement]`

**Do this.** Model dependencies as structured preconditions the dispatch layer enforces — a task whose `blockedBy` still lists open IDs cannot start. Distinguish `idle` (turn ended) from `completed` (explicitly done) in the harness, not just in prose. When synthesis is the deliverable, forbid even "while we wait" prep — the disconfirming input is exactly the one not yet arrived.

**Not this.** Treating an idle notification as completion, and "once tasks #1–#5 complete, synthesize" as advisory. The agent pre-reads the ready inputs "so it's fast" and posts partial synthesis on a subset — missing precisely the analysis assigned to disconfirm it.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). An agent assigned "once #1–#5 are completed… synthesize" rationalized reading the ready analyses "so synthesis is fast" and posted on a subset — the specifically-assigned disconfirming analyst was among those not yet in.

**Evidence.** `artifacts/session-evaluation/05-forensics-800f367a-4fd9fad1.md` (req #2076/#2079/#2087, the disconfirming analyst) via `06-root-cause-audit.md:48-54` §H4 · `.claude/rules/scope-and-agents.md:16-21` §"Idle vs Done" (live in-tree) · `CLAUDE.md` ("actively check on them — do not assume progress", live in-tree).

```mermaid
stateDiagram-v2
    [*] --> in_progress
    in_progress --> check : turn ends
    state check <<choice>>
    check --> completed : artifact pasted &&<br/>budget within limit &&<br/>scope == ask
    check --> Blocked : any gate fails
    Blocked --> in_progress : operator approves continuation
    completed --> [*]
    note right of Blocked
        first-class terminal state.
        target: a real enum value —
        not yet in the live
        pending|in_progress|completed|deleted
    end note
```

---

## 12. Keep schema-bound agents light, salvage transcripts instead of re-running, and treat interrupts as signals `[thread: evidence-over-attestation]`

**Do this.** Reserve forced structured-output schemas for *light extraction* agents (read a little, emit structure). Make broad-reading or file-writing agents schema-free — they have no structured-output call to forget. When a long multi-agent run dies on its final step, the verified work usually survives in the per-agent transcripts; recover it with a short script rather than re-running. And treat a human interrupt as a stop signal, not a prompt to retry with cosmetic variations.

**Not this.** Schema-forcing a broad/writing agent (it does the work, ends in prose, then fails "completed without calling the structured output"); blindly re-running a failed multi-million-token workflow (re-burns tokens, re-hits the limit); retrying an interrupted build with flag variants until the operator is frustrated.

**Why.** Three real incidents. Schema-heavy synthesis/file-writing agents failed "completed without calling StructuredOutput"; the fix was collapsing into one schema-free agent. A large multi-agent workflow died on its final synthesis step (server-side rate limit, then a session cap); the verified verdicts survived in `agent-*.jsonl` and were recovered with a script instead of re-running. After two interrupts of a workspace build, the right move was to declare implementation complete after a cheap LSP/`cargo check` verification and let the operator verify on their terms — not a third build variant.

**Evidence.** memory `feedback_workflow_schema_heavy_agents.md` (reserve schemas for light extraction; broad/writing agents schema-free; flatten nested schemas) · memory `debugging_workflow_salvage_transcripts.md` (rate/session-limit death surfaces as "no StructuredOutput"; read the transcript, assemble the artifact yourself) · memory `correction_respect_interrupts.md` (after one interrupt: stop, acknowledge, offer cheaper verification).

---

## 13. Guard against context pollution — lossy recaps and cross-session habit carry-over `[thread: evidence-over-attestation]`

**Do this.** Prefer short, focused sessions. Re-ground on the operator's *verbatim* instruction rather than an auto-summary, and do not let a previous session's tone set the next one's.

**Not this.** Acting on a lossy auto-recap that compressed nuanced operator history into a ≤40-word summary — the agent then acts on the summary, not the original, losing the operator's exact vocabulary across rounds. Or letting a prior session's defensive-narration habit bleed into a fresh one via context/cache continuity.

**Why.** [CS-5](../case-studies.md#cs-agent-forensics). One long session held repeated "user stepped away" recaps, each forced "under 40 words," collapsing operator nuance into agent-summarized intent; a separate session inherited the prior one's deflection habit minutes later. The positional-attention curve that grounds Lesson 3 is also why a mid-context lossy summary is a poor substitute for the verbatim instruction. (The byte-level-cache vs behavioural-inheritance mechanism is explicitly *unresolved* — the audit flagged it NEEDS-EVIDENCE — so this lesson guards the symptom rather than asserting a cause.)

**Evidence.** `artifacts/session-evaluation/04-forensics-1d42e82c.md` (9 recaps in 5h30; "under 40 words") via `00-EXECUTIVE-SUMMARY.md:49` · `artifacts/session-evaluation/06-root-cause-audit.md:94-100` §H10 PARTIAL (TTL caveat).

---

## 14. Cap the dispatch brief at ~2× the operator's instruction — orchestrator bloat becomes the implementer's mandate `[thread: deterministic-enforcement]`

Brief amplification correlates with both budget overrun and coverage narrowing; brief-sizing is owned by [ch. 16 — Agent-assisted dev: scope & delegation](./16-T13a-agent-scope-delegation.md).

---

## Further reading

- [Liu et al. — "Lost in the Middle: How Language Models Use Long Contexts" (arXiv:2307.03172)](https://arxiv.org/abs/2307.03172) — the U-shaped positional-attention curve (models attend strongest to the start and end of a context, degrade in the middle); REINFORCES Lesson 3 as the mechanism behind a buried rule being ignored on first read, and secondarily Lesson 13 on why mid-context lossy recaps lose nuance. Transfer caveat: the paper measures fact *retrieval*, not instruction *adherence* — the curve is the model's, so the mechanism carries, but cite it as a transfer.

---

> **A caveat on the deny hook (Lesson 2/4).** A deterministic gate that *false-fires* is the worst outcome — it trains operators to ignore it. In temet-proxy the shipped weasel-gate `Stop`/`SubagentStop` hook fired on legitimate corpus-quotation work (use-vs-mention conflation) and on a `command not found` mistaken for a test failure. If you ship an executable gate, strip quoted/fenced spans before matching and early-return on shell-resolution errors, so the gate stays trustworthy.
