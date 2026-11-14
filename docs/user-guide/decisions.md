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

With decisions on, the guard first runs its pre-check. Jev answers four questions about the ask,
the end of Claude's message, the files that changed and whether anything observed the change, and
mike combines the answers into a score between 0 and 1. A score under 0.3 holds the nudge.

1. Open the `docs` session, then **Session controls**, then **Steering activity**, and open the
   **Intervention timeline**.
2. Find the pre-check row for that turn. It reads **HELD** with the score and the four answers.
   A held nudge also gets its own **Held** row, so you can see what Claude was spared.
3. Later, when the nudge has been labelled as one that would not have helped, the row adds
   **hold correct**. If the same nudge would have been useful, it reads **hold wrong**.

When the score is 0.3 or higher the row reads **SENT** and the nudge goes out as before, with
**sent right** or **sent wrong** added once it has been labelled. **FAILED OPEN** means the
pre-check could not run, and the nudge was sent as if decisions were off.

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
