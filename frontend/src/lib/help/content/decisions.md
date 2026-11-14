## Where to find it

Open **Settings** in the left navigation, then **Steering**. The **Decisions** section at the top holds the master switch and the limits. The **Decision points** section below it lists every decision point with its mode and its track record. For a single session, open the session, open **Session controls**, then **Steering activity**, and open the **Intervention timeline**. For a fleet seat, open the fleet, click the seat, and read the **Decisions** group in the inspector.

## What a decision point is

At a number of places mike has to answer a small question before it acts: is this nudge worth sending, does this message correct the previous turn, does this mail need to wake the seat now. A decision point is one of those places. At each one a small model called Jev answers a narrow, typed question, and mike's own code decides what to do with the answer. Jev never writes the nudge or takes the action itself.

Every call is recorded, with the question, the answer, how long it took and what it cost. When the call fails, times out or is refused, mike acts as it would with decisions off. A failed call never blocks a session.

## The switch

The **Jev decisions** switch in the Decisions section turns the whole layer on or off. It applies to the next decision with no restart. With the switch off, no decision point calls Jev, whatever its own mode says.

The layer is also off when there is no OpenRouter key, because Jev runs on OpenRouter. Where mike shows **off: no OpenRouter key**, add a key under **Settings**, then **Integrations**. Where it shows **off: the decisions toggle is off**, turn the switch on. The two reasons look alike in the lists but are fixed in different places.

## Modes

Each decision point has its own mode.

- **On**: the answer is acted on.
- **Shadow**: the answer is recorded and compared with what happened, but mike does not act on it. Use it to watch a new point before trusting it.
- **Off**: the point does not call Jev.

Change a mode with the selector in the Decision points table. The five fleet points are the exception. Their mode comes from the fleet's definition, so the table shows **per fleet** instead of a selector.

## Reading the Decision points table

Each row is one decision point. **Calls** counts the answers in the last 14 days and **Applied** counts the ones that changed what mike did. **Fail-open** is the share of calls that failed and fell back to the off behaviour. **Right** shows how many of the applied answers a later review judged right, with a 95% interval, so a small sample shows as a wide range instead of a false certainty. **Spend** is the cost of the calls. **Findings** counts problems the periodic review found; open the row to read them.

A row that reads **not run** has had no calls in the window. A row that reads **off** is switched off. When the layer is off but earlier calls exist, the table stays visible under a **Decisions off** note so the history can still be read.

For the turn-guard pre-check, **Right** counts the held nudges that were later judged wrong, because a wrong nudge that was held means the hold was correct.

## The pre-check on a session

The turn-guard pre-check decides whether a nudge about an unchecked claim of being done is worth sending. In the intervention timeline its row shows **SENT** or **HELD** with a score, and four answers from Jev, each between 0 and 1. A nudge scoring under the hold threshold is held, and a held nudge appears in the timeline as its own **Held** row. **FAILED OPEN** means the pre-check could not run and the nudge was sent as if decisions were off.

When the nudge was later labelled, the row says whether the decision was right: **sent right**, **sent wrong**, **hold correct** or **hold wrong**.
