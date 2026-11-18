# Let a small model make mike's quick decisions

At many places mike has to answer a small question before it acts: is this nudge worth sending,
does this message correct the previous turn, does this mail need to wake a session now. Until
now mike answered those with keyword lists, thresholds or a larger model. With decisions
switched on, a small, fast model called Jev (`typesafe/jev-1.14`, served by OpenRouter) answers
the question instead. Each place where mike asks is a decision point.

mike supplies the facts and acts on the answer. The question is always narrow, a yes or no or a
choice from a short list, and Jev never writes a nudge or takes an action itself. A decision
changes what mike does, such as holding a nudge or waking a session. It does not change what you
type or what Claude Code sends to Anthropic.

Decisions send text from your sessions to OpenRouter, so they are off until you turn them on.
See [Privacy](privacy.md).

Open **Settings** in the menu, then **Steering**, <http://localhost:9990/settings/steering>. The
**Decisions** section at the top holds the switch and the limits. The **Decision points**
section below it lists every point with its mode and how well it has done.

This is a fictional example. The `docs` session in `~/projects/site` is a small demonstration;
nothing here comes from a real capture.

## Follow the example correction

Continue the example from [Quickstart](quickstart.md). In the `docs` session Claude has just
moved Homebrew to the top of the install page, as you asked, and ends its turn with "The install
page now lists Homebrew first." It did not preview the page.

mike's turn guard watches for turns that claim to be finished without anything having checked
the work, and it would normally send Claude a nudge saying so. A nudge costs Claude a turn, and
here it would not help: the change is one reordered list in a text file, and the final message
says exactly what changed.

On current post-alpha.5 source builds, inspect the pre-check rather than expecting this
fictional example to produce a particular answer. The `verifier_precheck_v4` question set
asks five questions about unfinished work, work still running, earlier completion, a gap
already disclosed, and whether the nudge matches the operator's ask. Code rules can also
hold a nudge without calling Jev, for example for an empty or machine-text ask or a mailbox
nudge. These are not the same as model judgements.

1. Open the `docs` session, then **Session controls**, then **Steering activity**, and open
   the **Intervention timeline**.
2. Find the pre-check for that turn. Read the recorded question set, answers or code rule,
   and the actual held/sent result, not just the score.
3. Check whether a later outcome exists before calling the decision right. No outcome
   means unlabelled, not correct. A held nudge does not by itself demonstrate what a reply
   to that nudge would have been.

In this source version, the model recommends sending only when the unfinished-work answer
is at least 0.8 and the strongest veto is below 0.6. The displayed composed score is not a
single 0.3 send threshold. A deterministic 10% holdback sample, where an evaluation UUID is present, lets an
otherwise-held nudge continue through the existing delivery path to collect later evidence.
This also applies to code-rule holds; it does not prove delivery succeeded. A hold
recommendation therefore does not always mean no nudge was sent. **FAILED OPEN** means the pre-check could not settle the call and
mike continued down its existing path. Check your running build and record: these details
are not a promise about alpha.5 or an unverified deployment.

## Switch decisions on

1. Add an OpenRouter key under **Settings**, then **Integrations**. Jev runs on OpenRouter, so
   there is nothing to call without one.
2. Open **Settings**, then **Steering**, and switch on **Jev decisions** in the **Decisions**
   section.
3. Look at the **Decision points** table. Each point shows its mode, and a point that has not
   yet made a call reads **not run**.

The switch applies to the next decision, with no restart. While it is off, no point calls Jev,
whatever the point's own mode says.

## What you see when decisions are off

Where a status line is shown for decisions, it says why. **off: no OpenRouter key** means no key
is set, so add one under Integrations. **off: the decisions toggle is off** means the key is
there and the switch is not. The table stays visible under a **Decisions off** note when earlier
calls exist, so you can still read their history.

mike works the same with decisions off. Every point falls back to the keyword list, threshold
or larger model it used before. The same happens for a single call that fails: if Jev times out
(the limit is 3 seconds), refuses, returns an answer mike cannot read, or the rate limit is
reached, mike takes the old path for that one call and carries on. A failed call never blocks a
session.

## Set a mode for one point

Each point has a mode, set with the selector in its row of the **Decision points** table:

- **On** is the default. The answer is acted on.
- **Shadow** records the answer and compares it with what happened, but mike does not act on
  it. Use it to watch a point before you trust it, or while you are debugging one.
- **Off** stops that point from calling Jev.

The five points that belong to fleets read **per fleet** instead of a selector. They are set in
the fleet's definition, under its `decisions` keys: `stop_check`, `wake_check`, `triage`,
`commit_message` and `effort`, each `off`, `shadow` or `on`. A worker can override them for
itself. See [Write a fleet definition](fleets/definition.md). The switch on the Steering page
still turns all of them off at once.

## See how well each point does

mike records every call: the question, the answer, how long it took, what it cost and whether
it changed anything. There are three places to read the results.

- **Decision points** on **Settings**, **Steering**. Each row is one point. **Calls** counts the
  answers in the last 14 days and **Applied** the ones that changed what mike did. **Fail-open**
  is the share of calls that failed and fell back. **Right** is how many applied answers a later
  review judged right, shown with a 95% interval so a small sample reads as a wide range.
  **Spend** is what the calls cost, and **Findings** counts problems the periodic review found;
  open a row to read them. A row that reads **not run** has had no calls in the window.
  For the turn-guard pre-check, **Right** counts held nudges that were labelled wrong, because
  a held nudge that would have been wrong means the hold was correct.
- The session's **Intervention timeline**, which shows the pre-check as in the example above.
- For a fleet, open the fleet, click a session, and read the **Decisions** group in the
  inspector.

### Do not mistake a score for proof

Compare a decision point with simple baselines, such as sending every eligible nudge or
holding every one, on the same labelled examples and question-set version. The
[verifier investigation](https://github.com/temet-ai/mike/issues/305#issuecomment-6907425570)
reported that its replay did not beat holding every fire; that is a reported result, not
an independent test of your installation. The later reply supplies evidence the pre-check
did not have when it acted. Better wording alone cannot supply a missing fact.

Read the label's evidence, sample size and whether it came from a replay or a later live
outcome. A high hold count is not measured accuracy. Do not turn a point off or change its
threshold solely because one replay looks poor; first identify the fact or failure mode
that would justify a change and compare like-for-like results.

## Replay without overclaiming

Current post-alpha.5 source builds provide `mike decisions replay` to ask today's questions
about historical evidence and compare the answers with labels. It is not an offline dry
run. It calls the configured decisions model through the configured OpenRouter endpoint,
transmits the reconstructed context and can incur charges. It does not send the replayed
nudge to a live session or establish how that session would respond. Switching off live
decisions is not a promise that an explicitly requested CLI replay makes no model calls.

[The replay report in PR #333](https://github.com/temet-ai/mike/pull/333) reports improvements
on the same `claude_review` examples used to investigate the weak points, including 50/50
for text origin and 45/45 for criteria source. Those are agreement results on that reviewed
set, not independent accuracy estimates or proof of live prompt behavior. The report also
notes turn-judge variation between 37/50 and 38/50 on the same code. Keep an untouched
labelled set for a separate evaluation before generalizing an improvement.

Before running a replay, check `mike decisions replay --help` for the installed point names
and options. Choose a bounded `--limit` and, where useful, `--since`; without them the source
implementation considers all available history. `--out` is required and overwrites an
existing file, so choose a fresh private output path. The JSONL includes request context,
labels and answers, not just aggregate scores. Review and redact it before sharing.

The replay may reconstruct context rather than reproduce the original request exactly:

- Text-origin replay reads the captured request body and recomputes its signals.
- Criteria-source replay combines stored thread criteria with opening and recent operator
  asks from the ledger, bounded by the original record's time.
- Leaf-judge replay supplements the report with captured command output and test names at
  its commit. `MIKE_REPLAY_REPOS` names a parent directory whose immediate children are
  searched for a repository containing that SHA, not a list of paths or a repository by
  itself. Missing repositories leave that reconstruction without freshly loaded test names.
  The output lookup uses a case-insensitive substring match on the command text within
  the previous 24 hours and is not scoped to the original session. Check that the selected output really belongs to the reported
  work before treating it as proof; a matching command is not unique provenance.

Read skipped-context counts, failures and any early-stop reason along with the score.
`agrees with current judge` is agreement with an earlier decision, not an independent
correctness label. Preserve the model, question-set version, source ids and reviewed
sample size when comparing runs. These are source-verified limitations, not a replay
performed for this guide or confirmation that an installed release includes the changes.

## What it costs and how it is limited

A call costs about $0.00005. Decisions have their own spend share, so they cannot use up the
budget the other models rely on, and their own limits, which are settings:

| Setting | Default | What it limits |
|---|---|---|
| `decisions.rate_per_minute` | 120 | Calls in any one minute |
| `decisions.max_calls_per_hour` | 3000 | Calls in any one hour |

A call over a limit takes the old path, as a failed call does. `decisions.timeout_ms` (3000) is
the time one call may take, and `decisions.model` names the model. Settings are listed in
[Settings](settings.md).

## If something looks wrong

| You see | What it means | What to do |
|---|---|---|
| **off: no OpenRouter key** | No key is set | Add one under Settings, Integrations |
| **off: the decisions toggle is off** | The switch is off | Switch on **Jev decisions** |
| A row reading **not run** | No calls in the last 14 days | Wait for the point to come up, or check it is not set to **Off** |
| A high **Fail-open** share | Calls are timing out or being refused | Check the key and the limits above; mike is using the old path meanwhile |
| A row reading **per fleet** | The mode comes from a fleet definition | Change it in the definition |

Related pages: [Steering](steering.md) for the nudges the pre-check holds, [The judge](judge.md)
for the labels that decide whether a hold was right, and [Fleets](fleets/README.md).
