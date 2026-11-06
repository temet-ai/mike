import { expect, mock, test } from "claude-code/testing";
import type { Engine } from "claude-code/testing";
import type { On } from "claude-code";

import { GOAL_MAX_CHARS, isControlSeat, ownGoal } from "./goal";
import { continuationLimit, goalBudgetKey } from "./goal-budget";
import type { FleetPanel, FleetSeat } from "../types";

const OWN = "aaaa1111-0000-4000-8000-000000000001";
const COORD = "e74ba661-0000-4000-8000-000000000002";
const NOW = Date.parse("2026-10-08T10:00:00Z");
const TURN_DONE = {
  answer: "",
  durationMs: 1,
  isAborted: false,
  turnId: "t1",
  reason: "answer",
} as const;

function seat(name: string, session: string, goal: string | null): FleetSeat {
  return {
    seat: name,
    role: name.replace(/-[0-9a-f]{8}$/, ""),
    model: "opus",
    state: "working",
    session,
    goal,
    contextTokens: null,
    requests: 1,
    lastRequestAt: null,
    closed: false,
    cache: null,
  };
}

function panel(seats: FleetSeat[], isOwn = true): FleetPanel {
  return {
    fleet: "f",
    slug: "core",
    state: "running",
    goal: "the fleet goal",
    startedAt: null,
    finishedAt: null,
    now: "2026-10-08T10:00:00Z",
    missing: [],
    accepted: false,
    openObligations: [],
    criteria: [],
    tasks: [],
    seats,
    verdicts: [],
    blocked: [],
    needsOperator: [],
    everySeatIdleSeconds: null,
    isOwn,
  };
}

test("the own seat's goal is read by session, within the limit, from the own fleet only", () => {
  const seats = [seat("acceptance-aaaa1111", OWN, "every task has a verdict"), seat("coordinator-e74ba661", COORD, "the fleet is closed")];
  expect(ownGoal(panel(seats), OWN)).toBe("every task has a verdict");
  expect(ownGoal(panel(seats), COORD)).toBe("the fleet is closed");
  expect(ownGoal(panel(seats, false), OWN)).toBe(null);
  expect(ownGoal(null, OWN)).toBe(null);
  expect(ownGoal(panel(seats), "")).toBe(null);
  expect(ownGoal(panel([seat("acceptance-aaaa1111", OWN, null)]), OWN)).toBe(null);
  expect(ownGoal(panel([seat("acceptance-aaaa1111", OWN, "   ")]), OWN)).toBe(null);
  expect(ownGoal(panel([seat("acceptance-aaaa1111", OWN, "x".repeat(GOAL_MAX_CHARS))]), OWN)).toBe("x".repeat(GOAL_MAX_CHARS));
  expect(ownGoal(panel([seat("acceptance-aaaa1111", OWN, "x".repeat(GOAL_MAX_CHARS + 1))]), OWN)).toBe(null);
});

type World = { goal: string | null; ran: string[]; role?: string; seatName?: string; saved?: Record<string, unknown>; failStore?: boolean; dropPrompt?: boolean; notes?: string[]; failNotes?: boolean };

function status(w: World) {
  return {
    fleet: "f",
    slug: "core",
    state: "running",
    goal: "the fleet goal",
    now: "2026-10-08T10:00:00Z",
    missing: [],
    accepted: false,
    obligations: [],
    criteria: [],
    tasks: [],
    seats: [
      { seat: w.seatName ?? "db-dev-T2", role: w.role ?? "db-dev", model: "opus", state: "working", session: OWN, goal: w.goal, requests: 1, closed: false },
      { seat: "coordinator-e74ba661", role: "coordinator", model: "opus", state: "working", session: COORD, goal: "the fleet is closed", requests: 1, closed: false },
    ],
    verdicts: [],
    blocked: [],
    needs_operator: [],
  };
}

function boot($: Engine, on: On, w: World) {
  const clock = mock.clock(on, { now: NOW });
  const saved = w.saved ?? {};
  on("store.get", (_, e) => ({ value: saved[e.key] }));
  on("store.set", (_, e) => {
    if (w.failStore) throw new Error("store unavailable");
    saved[e.key] = e.value;
    return { value: undefined };
  });
  mock.env(on, { MIKE_FLEET_GOAL_MAX_CONTINUATIONS: "2", MIKE_RUN_ID: "fleet-core-1" });
  on("session.id", () => ({ value: OWN }));
  on("session.root", () => ({ value: "/repo" }));
  on("session.start", (_, e) => ({ cwd: e.cwd }));
  on("command.register", (_, e) => ({ value: { command: e.name } }));
  on("turn.complete", () => ({ text: "" }));
  on("classic.Stop", () => ({ block: "The goal is not met; continue working." }));
  on("prompt.submit", (_, e) => w.dropPrompt ? { drop: "not submitted" } : { text: e.text });
  on("ui.panes", () => ({ value: [] }));
  on("command.run", (_, e, next) => {
    if (e.command !== "goal") return next(e);
    w.ran.push(e.args);
    return { text: `Goal set: ${e.args}` };
  });
  on("http.fetch", (_, e) => {
    const ok = (body: unknown) => ({
      value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) },
    });
    if (e.url.endsWith("/notes")) {
      w.notes?.push(String(e.init?.body));
      if (w.failNotes) throw new Error("notes unavailable");
    }
    if (e.url.includes("/api/fleets/by-session/")) return ok(status(w));
    if (e.url.endsWith("/messages")) return ok({ inbox: [], outbox: [] });
    if (e.url.endsWith("/roster")) return ok({ roster: [] });
    if (e.url.includes("/api/threads")) return ok({ threads: [] });
    return ok({});
  });
  return clock;
}

async function start($: Engine, clock: { settle: () => Promise<void> }) {
  await $.session.start({ cwd: "/repo", surface: "terminal", isInteractive: true });
  await clock.settle();
}

test("a seat with a goal in the definition arms /goal once after a turn, and again only when the text changes", async ($, on) => {
  const w: World = { goal: "T2 is reported with fleet_report; or stop after 20 turns", ran: [] };
  const clock = boot($, on, w);
  await start($, clock);
  await $.turn.complete(TURN_DONE);
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual(["T2 is reported with fleet_report; or stop after 20 turns"]);
  w.goal = "T2 is accepted; or stop after 10 turns";
  await clock.advance(10_000);
  await clock.settle();
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual([
    "T2 is reported with fleet_report; or stop after 20 turns",
    "T2 is accepted; or stop after 10 turns",
  ]);
});

test("a seat without a goal in the definition runs no /goal", async ($, on) => {
  const w: World = { goal: null, ran: [] };
  const clock = boot($, on, w);
  await start($, clock);
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual([]);
});

test("a control seat clears /goal once, whatever an older definition gave it", async ($, on) => {
  const w: World = { goal: "every task has a final verdict; or stop after 80 turns", ran: [], role: "acceptance", seatName: "acceptance-aaaa1111" };
  const clock = boot($, on, w);
  await start($, clock);
  await $.turn.complete(TURN_DONE);
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual(["clear"]);
});

test("only coordinator, acceptance and observer of the own fleet are control seats", () => {
  const seats = [seat("acceptance-aaaa1111", OWN, null), seat("db-dev-T1", COORD, "T1")];
  expect(isControlSeat(panel(seats), OWN)).toBe(true);
  expect(isControlSeat(panel(seats), COORD)).toBe(false);
  expect(isControlSeat(panel(seats, false), OWN)).toBe(false);
  expect(isControlSeat(null, OWN)).toBe(false);
});

const STOP = {
  hook_event_name: "Stop",
  session_id: OWN,
  transcript_path: "/tmp/session.jsonl",
  cwd: "/repo",
  stop_hook_active: true,
} as const;

test("forced continuations stop at the limit, clear the goal, and stay stopped", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], notes: [] };
  const clock = boot($, on, w);
  await start($, clock);
  await $.turn.complete(TURN_DONE);
  expect((await $.classic.Stop({ ...STOP, stop_hook_active: false })).block).toBeDefined();
  expect((await $.classic.Stop(STOP)).block).toBeDefined();
  const exhausted = await $.classic.Stop(STOP);
  expect(exhausted.preventContinuation).toBe(true);
  expect(exhausted.block).toBeUndefined();
  expect(exhausted.stopReason).toContain("2 automatic continuations");
  expect(exhausted.stopReason).toContain("unfinished");
  await $.turn.complete(TURN_DONE);
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual(["Finish T2", "clear"]);
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  expect(w.notes?.length).toBe(1);
  expect(JSON.parse(w.notes![0]).data.schema).toBe("goal-budget-event-v1");
});

test("a resumed session keeps the saved continuation count and exhausted budget", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], saved: {
    [goalBudgetKey(OWN)]: { goal: "Finish T2", continuations: 1, exhausted: false },
  } };
  const clock = boot($, on, w);
  await start($, clock);
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  await start($, clock);
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual(["clear"]);
});

test("mail can run work without renewing the budget, while an operator request renews it", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], saved: {
    [goalBudgetKey(OWN)]: { goal: "Finish T2", continuations: 2, exhausted: true },
  } };
  const clock = boot($, on, w);
  await start($, clock);
  for (const kind of ["peer", "task-notification", "scheduled-trigger", "auto-continuation", "bridge", "sdk"] as const) {
    const result = await $.prompt.submit({ text: "New notification", wait: false, origin: { kind } });
    expect(result.text).toBe("New notification");
    expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  }
  await $.turn.complete(TURN_DONE);
  await $.prompt.submit({ text: "The blocker is fixed; finish T2", wait: false, origin: { kind: "composer" } });
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual(["clear", "Finish T2"]);
  expect((await $.classic.Stop(STOP)).block).toBeDefined();
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
});

test("a changed fleet goal renews the budget", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], saved: {
    [goalBudgetKey(OWN)]: { goal: "Finish T2", continuations: 2, exhausted: true },
  } };
  const clock = boot($, on, w);
  await start($, clock);
  await $.turn.complete(TURN_DONE);
  w.goal = "Finish the revised T2";
  await clock.advance(10_000);
  await clock.settle();
  await $.turn.complete(TURN_DONE);
  expect(w.ran).toEqual(["clear", "Finish the revised T2"]);
  expect((await $.classic.Stop(STOP)).block).toBeDefined();
});

test("child agents cannot arm the main goal or spend its continuation budget", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [] };
  const clock = boot($, on, w);
  await start($, clock);
  await $.turn.complete({ ...TURN_DONE, agentId: "child" });
  expect(w.ran).toEqual([]);
  for (let i = 0; i < 3; i++) {
    expect((await $.classic.Stop({ ...STOP, agent_id: "child" })).block).toBeDefined();
  }
  expect((await $.classic.Stop(STOP)).block).toBeDefined();
});

test("control seats do not get a worker continuation budget", async ($, on) => {
  const w: World = { goal: "Close fleet", ran: [], role: "coordinator" };
  const clock = boot($, on, w);
  await start($, clock);
  for (let i = 0; i < 3; i++) expect((await $.classic.Stop(STOP)).block).toBeDefined();
});

test("the continuation limit is always a positive bounded integer", () => {
  expect(continuationLimit("3")).toBe(3);
  expect(continuationLimit("200")).toBe(200);
  for (const value of [undefined, "", "0", "-1", "201", "Infinity", "2.5", "no"]) {
    expect(continuationLimit(value)).toBe(20);
  }
});

test("a rejected or meta operator prompt does not renew an exhausted budget", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], dropPrompt: true, saved: {
    [goalBudgetKey(OWN)]: { goal: "Finish T2", continuations: 2, exhausted: true },
  } };
  const clock = boot($, on, w);
  await start($, clock);
  await $.prompt.submit({ text: "Resume", wait: false, origin: { kind: "composer" } });
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  w.dropPrompt = false;
  await $.prompt.submit({ text: "Resume", isMeta: true, wait: false, origin: { kind: "composer" } });
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
});

test("failure to persist the budget stops automatic continuation", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], failStore: true };
  const clock = boot($, on, w);
  await start($, clock);
  const result = await $.classic.Stop(STOP);
  expect(result.preventContinuation).toBe(true);
  expect(result.block).toBeUndefined();
  expect(result.stopReason).toContain("could not be saved");
});


test("failed pause telemetry cannot restart continuations or retry every stop", async ($, on) => {
  const w: World = { goal: "Finish T2", ran: [], notes: [], failNotes: true, saved: {
    [goalBudgetKey(OWN)]: { goal: "Finish T2", continuations: 2, exhausted: true },
  } };
  const clock = boot($, on, w);
  await start($, clock);
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  expect((await $.classic.Stop(STOP)).preventContinuation).toBe(true);
  expect(w.notes?.length).toBe(1);
});
