import { expect, mock, test } from "claude-code/testing";
import type { Engine } from "claude-code/testing";
import type { On } from "claude-code";

import {
  ago,
  attention,
  contextBar,
  dayAndTime,
  elapsed,
  hourAndMinute,
  seatColour,
  seatColumns,
  seatStateColour,
  statusLine,
  summarise,
  taskColumns,
  taskMark,
  titleButtons,
  titleFit,
  toPanel,
  seatRows,
  closedSeatsLabel,
  noSeatOpen,
  noSeatWords,
  noSeatReason,
  alarmKey,
  stopWords,
  taskCounts,
  taskCountsText,
  cacheStart,
  allSeats,
  STATUS_MS,
} from "./status";
import type { FleetSeat } from "../types";
import { CUT, WHOLE } from "./panel";

const OWN = "aaaa1111-0000-4000-8000-000000000001";
const ROOT = "/repo";
const FLEET = "99230e8d-8bcc-4f59-b5d4-a3032b4cbf73";
const NOW_ISO = "2026-10-08T10:17:47Z";
const NOW = Date.parse(NOW_ISO);
const INSPECT = "http://localhost:9989";

const STATUS = {
  fleet: FLEET,
  slug: "no-more-issues",
  state: "running",
  run_id: "fleet-no-more-issues-99230e8d",
  goal: "The open defects in mike's fleet machinery   and capture are fixed on main.",
  started_at: "2026-10-08T07:26:20Z",
  finished_at: null,
  now: NOW_ISO,
  missing: [],
  accepted: false,
  obligations: [
    {
      obligation_id: "O-C6-final",
      title: "O-C6-final: final pushed head gate PASS",
      outcome: "failed",
      required: true,
      issue: null,
    },
    {
      obligation_id: "O-129-264",
      title: "O-129-264: liveness from activity",
      outcome: "accepted",
      required: true,
      issue: null,
    },
  ],
  criteria: [
    { id: "C1", text: "", tasks: 7, passed: 7, ready: 7 },
    { id: "C2", text: "", tasks: 3, passed: 2, ready: 3 },
    { id: "C7", text: "", tasks: 0, passed: 0, ready: 0 },
  ],
  tasks: [
    {
      id: "T1",
      name: "#517 a build brake without a declared wrapper",
      discipline: "backend",
      criteria: ["C1"],
      seat: "backend-T1",
      state: "passed",
      passes: 2,
      fails: 0,
      ready: true,
      last: {
        at: "2026-10-08T08:39:47Z",
        verdict: "pass",
        title: "T1 C1 (final): same verdict as the one just sent",
        tasks: ["T1"],
        criteria: ["C1"],
      },
    },
    {
      id: "T7",
      name: "#524 gate at the starting head",
      discipline: "backend",
      criteria: ["C1", "C6"],
      seat: "backend-T7",
      state: "rework",
      passes: 2,
      fails: 0,
      ready: true,
      last: {
        at: "2026-10-08T09:37:35Z",
        verdict: "fail",
        title: "T7 C6: the gate is red at the starting head",
        tasks: ["T7"],
        criteria: ["C6"],
      },
    },
    {
      id: "T15",
      name: "a task nobody has yet",
      discipline: "tooling",
      criteria: ["C2"],
      seat: null,
      state: "queued",
      passes: 0,
      fails: 0,
      ready: false,
      last: null,
    },
  ],
  seats: [
    {
      seat: "coordinator",
      role: "coordinator",
      model: "opus",
      state: "idle",
      context_tokens: 436601,
      requests: 373,
      last_request_at: "2026-10-08T10:08:21Z",
      opened_at: "2026-10-08T07:25:31Z",
      closed: false,
      first_request_cache: { read: 0, written: 79879, cold: false },
    },
    {
      seat: "acceptance",
      role: "acceptance",
      model: "opus",
      state: "idle",
      context_tokens: 698000,
      requests: 606,
      last_request_at: "2026-10-08T10:10:47Z",
      opened_at: null,
      closed: false,
      first_request_cache: null,
    },
    {
      seat: "backend-T2",
      role: "backend",
      model: "sonnet",
      state: "closed",
      context_tokens: 112000,
      requests: 24,
      last_request_at: "2026-10-08T09:12:00Z",
      opened_at: null,
      closed: true,
      first_request_cache: { read: 33000, written: 123000, cold: true },
    },
    {
      seat: "backend-T9",
      role: "backend",
      model: "sonnet",
      state: "working",
      context_tokens: null,
      requests: 0,
      last_request_at: null,
      opened_at: null,
      closed: false,
      first_request_cache: null,
    },
  ],
  verdicts: [
    {
      at: "2026-10-08T09:37:35Z",
      verdict: "pass",
      title: "T7 C6 (final): Final PASS T7 C6.",
      tasks: ["T7"],
      criteria: ["C6"],
    },
    {
      at: "2026-10-08T09:05:00Z",
      verdict: "fail",
      title: "T8 C2: the test is missing",
      tasks: ["T8"],
      criteria: ["C2"],
    },
  ],
  blocked: [{ seat: "backend-T9", title: "waiting on the gate slot" }],
  needs_operator: [
    {
      at: "2026-10-08T09:59:00Z",
      seat: "coordinator",
      title: "OPERATOR: commits are refused in a cycle",
    },
  ],
  every_seat_idle_seconds: 300,
};

const RUN = {
  command: "fleet-comms",
  args: "",
  origin: { kind: "composer" as const },
  presentation: { isFullscreen: true, columns: 120 },
};

function panel() {
  const p = toPanel(STATUS, false);
  if (p === null) throw new Error("the fixture is not a status");
  return p;
}

function seat(name: string, closed: boolean): FleetSeat {
  return {
    seat: name,
    role: "backend",
    model: "sonnet",
    state: closed ? "closed" : "idle",
    contextTokens: null,
    requests: 0,
    lastRequestAt: null,
    closed,
    cache: null,
  };
}

test("a body that is not a fleet status is refused; open obligations are the ones kept", () => {
  expect(toPanel({ threads: [] }, false)).toBeNull();
  expect(toPanel({ seats: [{ seat: "coordinator" }] }, false)).toBeNull();
  const p = panel();
  expect(p.openObligations.map((o) => o.id)).toEqual(["O-C6-final"]);
  expect(p.tasks[2]?.seat).toBeNull();
  expect(p.seats[2]?.cache?.cold).toBe(true);
  expect(toPanel(STATUS, true)?.isOwn).toBe(true);
});

test("clock words match the panel's", () => {
  expect(elapsed("2026-10-08T07:26:20Z", NOW_ISO)).toBe("2h51m");
  expect(ago(null, NOW_ISO)).toBe("never");
  expect(ago("2026-10-08T10:17:06Z", NOW_ISO)).toBe("41s ago");
  expect(ago("2026-10-08T10:08:21Z", NOW_ISO)).toBe("9m ago");
  expect(ago("2026-10-08T08:12:00Z", NOW_ISO)).toBe("2h05m ago");
  expect(dayAndTime(NOW_ISO)).toBe("08 Oct 10:17 UTC");
  expect(hourAndMinute("2026-10-08T09:37:35Z")).toBe("09:37");
});

test("the summary line counts as the panel does", () => {
  const s = summarise(panel());
  expect(s.running).toEqual([
    { text: "running for ", colour: "dim" },
    { text: "2h51m", bold: true },
  ]);
  expect(s.tasks).toEqual([
    { text: "1", bold: true },
    { text: " of ", colour: "dim" },
    { text: "3", bold: true },
    { text: " tasks accepted; ", colour: "dim" },
    { text: "2", bold: true },
    { text: " ready", colour: "dim" },
  ]);
  expect(s.acceptance).toEqual([
    { text: "fleet acceptance ", colour: "dim" },
    { text: "pending", colour: "yellow" },
  ]);
  expect(s.verdicts).toEqual([
    { text: "4 pass", colour: "green", bold: true },
    { text: " " },
    { text: "0 fail", colour: "dim" },
  ]);
  expect(s.seats.map((x) => x.text).join("")).toBe("3 seats open, 4 launched");
  const done = summarise({ ...panel(), finishedAt: "2026-10-08T10:00:00Z", accepted: true });
  expect(done.running.map((x) => x.text).join("")).toBe("ran for 2h33m");
  expect(done.acceptance[1]).toEqual({ text: "accepted", colour: "green" });
  const failing = summarise({
    ...panel(),
    tasks: panel().tasks.map((t) => ({ ...t, fails: 1 })),
  });
  expect(failing.verdicts[2]).toEqual({ text: "3 fail", colour: "red", bold: true });
  expect(summarise({ ...panel(), startedAt: null }).running).toEqual([
    { text: "not started", colour: "dim" },
  ]);
});

test("the context bar fills by the thousand and says replace from 600k, with no mark in it", () => {
  expect(contextBar(null)).toBeNull();
  const warm = contextBar(436601);
  const cells = warm?.runs.map((r) => r.text).join("") ?? "";
  expect(cells).toBe("█".repeat(9) + "░".repeat(11));
  expect(cells).not.toContain("▐");
  expect(warm?.number).toBe(" 437k");
  expect(warm?.colour).toBe("yellow");
  expect(warm?.replace).toBe(false);
  const over = contextBar(698000);
  expect(over?.colour).toBe("red");
  expect(over?.replace).toBe(true);
  expect(over?.runs.map((r) => r.colour)).toEqual(["red", "faint"]);
  expect(contextBar(12000)?.colour).toBe("green");
  expect(contextBar(1_000_000)?.number).toBe("1000k");
});

test("the small context bar has 10 cells", () => {
  const small = contextBar(436601, 10);
  expect(small?.runs.map((r) => r.text).join("")).toBe("████░░░░░░");
  expect(small?.number).toBe(" 437k");
  const over = contextBar(698000, 10);
  expect(over?.runs.map((r) => r.text).join("")).toBe("███████░░░");
  expect(over?.replace).toBe(true);
});

test("seat rows are the open seats; the closed ones are counted until they are asked for", () => {
  const live = seatRows(panel().seats, false);
  expect(live.shown.map((s) => s.seat)).toEqual(["coordinator", "acceptance", "backend-T9"]);
  expect(live.closed).toBe(1);
  const asked = seatRows(panel().seats, true);
  expect(asked.shown.map((s) => s.seat)).toEqual([
    "coordinator",
    "acceptance",
    "backend-T9",
    "backend-T2",
  ]);
  const closed = seatRows(Array.from({ length: 21 }, (_, i) => seat(`s${i}`, true)), false);
  expect(closed.shown).toEqual([]);
  expect(closed.closed).toBe(21);
  expect(closedSeatsLabel(21, false)).toBe("Show 21 closed seats");
  expect(closedSeatsLabel(1, false)).toBe("Show 1 closed seat");
  expect(closedSeatsLabel(1, true)).toBe("Hide closed seats");
});

test("a task's counts are words: passed, failed in red, or none yet", () => {
  const [t1, , t15] = panel().tasks;
  expect(taskCounts(t1!)).toEqual([{ text: "2 passed", colour: "dim" }]);
  expect(taskCounts(t15!)).toEqual([{ text: "none yet", colour: "dim" }]);
  const failing = { ...t1!, passes: 2, fails: 1 };
  expect(taskCounts(failing)).toEqual([
    { text: "2 passed", colour: "dim" },
    { text: ", ", colour: "dim" },
    { text: "1 failed", colour: "red" },
  ]);
  expect(taskCountsText(failing)).toBe("2 passed, 1 failed");
});

test("a running fleet with no open seat is an alarm that says what is still waiting", () => {
  const p = panel();
  const shut = { ...p, seats: p.seats.map((s) => ({ ...s, closed: true })) };
  expect(noSeatOpen(p)).toBeNull();
  expect(noSeatOpen({ ...shut, state: "done" })).toBeNull();
  expect(noSeatOpen({ ...shut, accepted: true })).toBeNull();
  const none = noSeatOpen(shut);
  expect(none).toEqual({ rework: 1, checks: 1 });
  expect(noSeatWords(none!)).toBe("1 task still in rework, 1 acceptance check open");
  expect(noSeatWords({ rework: 2, checks: 3 })).toBe(
    "2 tasks still in rework, 3 acceptance checks open",
  );
  expect(noSeatWords({ rework: 0, checks: 0 })).toBe("nothing else is waiting");
  const calm = { ...shut, needsOperator: [] };
  expect(attention(calm)).toEqual({
    head: "NO SEAT IS OPEN",
    slug: "no-more-issues",
    words: "1 task still in rework, 1 acceptance check open",
  });
  expect(statusLine(calm)).toBe(
    "NO SEAT IS OPEN · fleet no-more-issues: 1 task still in rework, 1 acceptance check open",
  );
  expect(attention(shut)?.head).toBe("NEEDS YOU");
  expect(attention({ ...calm, seats: [] })?.head).toBe("NO SEAT IS OPEN");
});

const room = (columns: number) => columns - 5;
const keys = (c: { key: string }[]) => c.map((x) => x.key);
const widths = (c: { width: number }[]) => c.map((x) => x.width);

test("task columns fit the fleet and drop seat, criteria, counts, discipline in that order", () => {
  const tasks = panel().tasks;
  const all = taskColumns(tasks, room(150));
  expect(all.layout).toBe("line");
  expect(all.dropped).toBe(false);
  expect(keys(all.columns)).toEqual([
    "mark",
    "id",
    "name",
    "state",
    "discipline",
    "criteria",
    "seat",
    "counts",
    "verdict",
  ]);
  expect(widths(all.columns)).toEqual([1, 3, 40, 7, 7, 5, 10, 8, 56]);
  expect(widths(all.columns).reduce((n, w) => n + w, 0) + 8).toBe(room(150));
  const hundred = taskColumns(tasks, room(100));
  expect(hundred.dropped).toBe(false);
  expect(widths(hundred.columns)).toEqual([1, 3, 26, 7, 7, 5, 10, 8, 20]);
  const eighty = taskColumns(tasks, room(80));
  expect(eighty.layout).toBe("line");
  expect(eighty.dropped).toBe(true);
  expect(keys(eighty.columns)).toEqual([
    "mark",
    "id",
    "name",
    "state",
    "discipline",
    "counts",
    "verdict",
  ]);
  expect(widths(eighty.columns)).toEqual([1, 3, 23, 7, 7, 8, 20]);
  const sixty = taskColumns(tasks, room(60));
  expect(sixty.layout).toBe("line");
  expect(keys(sixty.columns)).toEqual(["mark", "id", "name", "state", "verdict"]);
  expect(widths(sixty.columns)).toEqual([1, 3, 20, 7, 20]);
  expect(taskColumns(tasks, room(40)).layout).toBe("stacked");
  expect(taskColumns([], room(150)).layout).toBe("line");
});

test("seat columns fit the fleet and drop first request, role, model, requests, last request, then the bar halves", () => {
  const seats = seatRows(panel().seats, false).shown;
  const all = seatColumns(seats, room(150));
  expect(all.layout).toBe("line");
  expect(all.dropped).toBe(false);
  expect(all.barCells).toBe(20);
  expect(keys(all.columns)).toEqual([
    "seat",
    "state",
    "context",
    "last",
    "requests",
    "model",
    "role",
    "first",
  ]);
  expect(widths(all.columns)).toEqual([11, 7, 34, 12, 8, 6, 11, 49]);
  expect(all.columns.find((c) => c.key === "requests")?.align).toBe("end");
  const hundred = seatColumns(seats, room(100));
  expect(keys(hundred.columns)).toEqual([
    "seat",
    "state",
    "context",
    "last",
    "requests",
    "model",
    "role",
  ]);
  expect(hundred.dropped).toBe(true);
  expect(keys(seatColumns(seats, room(80)).columns)).toEqual([
    "seat",
    "state",
    "context",
    "last",
  ]);
  const sixty = seatColumns(seats, room(60));
  expect(keys(sixty.columns)).toEqual(["seat", "state", "context"]);
  expect(sixty.barCells).toBe(20);
  const tight = seatColumns(seats, 44);
  expect(tight.layout).toBe("line");
  expect(tight.barCells).toBe(10);
  expect(widths(tight.columns)).toEqual([11, 7, 24]);
  const stacked = seatColumns(seats, 30);
  expect(stacked.layout).toBe("stacked");
  expect(stacked.columns).toEqual([]);
});

test("the title fits on one row, then two rows, then two rows with the note below", () => {
  const p = panel();
  const buttons = titleButtons(true, false);
  expect(buttons.map((b) => `${b.hotkey}: ${b.label}`)).toEqual([
    "s: Hide details",
    "w: Show all",
    "r: Refresh",
  ]);
  expect(titleFit(93, p, buttons)).toBe("one");
  expect(titleFit(92, p, buttons)).toBe("two");
  expect(titleFit(51, p, buttons)).toBe("two");
  expect(titleFit(50, p, buttons)).toBe("alone");
  expect(titleFit(39, p, buttons)).toBe("alone");
  const own = { ...p, isOwn: true };
  expect(titleFit(115, own, buttons)).toBe("one");
  expect(titleFit(114, own, buttons)).toBe("two");
  expect(titleFit(73, own, buttons)).toBe("two");
  expect(titleFit(72, own, buttons)).toBe("twoNoteBelow");
  expect(titleFit(51, own, buttons)).toBe("twoNoteBelow");
  expect(titleFit(50, own, buttons)).toBe("alone");
  expect(titleButtons(true, true)[1]?.label).toBe("One line each");
  expect(titleButtons(false, false).map((b) => b.label)).toEqual(["Show details", "Refresh"]);
});

test("a seat takes its role's colour, else none", () => {
  const p = panel();
  expect(seatColour(p, "coordinator")).toBe("rgb(245,138,200)");
  expect(seatColour(p, "acceptance")).toBe("rgb(224,175,104)");
  expect(seatColour(p, "backend-T9")).toBeUndefined();
  expect(seatColour(p, "nobody")).toBeUndefined();
});

test("a reworked task is marked ✘ and a closed seat reads dim", () => {
  expect(taskMark("rework")).toEqual({ mark: "✘", colour: "red" });
  expect(taskMark("passed").mark).toBe("✔");
  expect(seatStateColour("closed")).toBe("dim");
  expect(seatStateColour("idle")).toBe("yellow");
  expect(seatStateColour("quota")).toBe("red");
});

test("the first request reads without its prefix and flags a cold start", () => {
  const seats = panel().seats;
  expect(cacheStart(seats[0]!)).toEqual({ text: "read 0k wrote 80k", cold: false });
  expect(cacheStart(seats[2]!)).toEqual({
    text: "read 33k wrote 123k COLD START",
    cold: true,
  });
  expect(cacheStart(seats[1]!)).toBeNull();
});

test("every seat is listed with the open ones first", () => {
  expect(allSeats(panel().seats).map((s) => s.seat)).toEqual([
    "coordinator",
    "acceptance",
    "backend-T9",
    "backend-T2",
  ]);
});

test("the status line carries NEEDS YOU first, then NOTHING IS MOVING, else nothing", () => {
  const p = panel();
  expect(attention(p)).toEqual({
    head: "NEEDS YOU",
    seat: "coordinator",
    title: "OPERATOR: commits are refused in a cycle",
    more: 0,
  });
  expect(statusLine(p)).toBe("NEEDS YOU · coordinator: OPERATOR: commits are refused in a cycle");
  const idle = { ...p, needsOperator: [] };
  expect(attention(idle)).toEqual({ head: "NOTHING IS MOVING", slug: "no-more-issues", minutes: 5 });
  expect(statusLine(idle)).toBe(
    "NOTHING IS MOVING · fleet no-more-issues: every open seat has been idle for 5m",
  );
  expect(statusLine({ ...idle, everySeatIdleSeconds: null })).toBeUndefined();
  expect(statusLine(null)).toBeUndefined();
  const two = { ...p, needsOperator: [...p.needsOperator, { at: NOW_ISO, seat: "x", title: "y" }] };
  expect(statusLine(two)).toContain("(+1 more)");
});

test("a fleet that is not starting or running raises no alarm, whatever it left behind", () => {
  const p = panel();
  const idle = { ...p, needsOperator: [] };
  for (const state of ["done", "failed", "draft", "ready"]) {
    expect(attention({ ...p, state })).toBeNull();
    expect(statusLine({ ...p, state })).toBeUndefined();
    expect(statusLine({ ...idle, state })).toBeUndefined();
  }
  expect(attention({ ...p, state: "starting" })?.head).toBe("NEEDS YOU");
  expect(attention({ ...p, state: "running" })?.head).toBe("NEEDS YOU");
});

type World = {
  isSeat: boolean; // started as a fleet seat (MIKE_RUN_ID set)
  isFailing: boolean;
  urls: string[];
  statuses: (string | undefined)[];
  toasts: string[];
  opens: { id: string; title?: string }[];
  registered: string[];
  status: object;
  stops: string[];
  stopFails: boolean;
};

function world(isSeat: boolean, status: object = STATUS): World {
  return {
    isSeat,
    isFailing: false,
    urls: [],
    statuses: [],
    toasts: [],
    opens: [],
    registered: [],
    status,
    stops: [],
    stopFails: false,
  };
}

function boot($: Engine, on: On, w: World, root = ROOT) {
  const clock = mock.clock(on, { now: NOW });
  const ok = (body: unknown) => ({
    value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) },
  });
  const notFound = { value: { status: 404, ok: false, headers: {}, text: "{}" } };
  on("ui.render", ($, e) => $.ui.resolve(e).Text({ children: "ENGINE DRAWS THIS" }));
  on("session.id", () => ({ value: OWN }));
  on("env.get", (_, e) => ({ value: e.name === "MIKE_RUN_ID" && w.isSeat ? "fleet-no-more-issues-1" : undefined }));
  on("session.root", () => ({ value: root }));
  on("session.start", (_, e) => ({ cwd: e.cwd }));
  on("command.register", (_, e) => {
    w.registered.push(e.name);
    return { value: { command: e.name } };
  });
  on("ui.open", (_, e) => {
    w.opens.push(e);
    return { value: { isPlaced: true as const } };
  });
  on("ui.panes", () => ({ value: [] }));
  on("ui.status", (_, e) => {
    w.statuses.push(e.text);
    return { value: undefined };
  });
  on("ui.toast", (_, e) => {
    w.toasts.push(e.text);
    return { value: undefined };
  });
  on("http.fetch", (_, e) => {
    w.urls.push(e.url);
    if (w.isFailing) return { value: { status: 500, ok: false, headers: {}, text: "" } };
    if (e.url.endsWith("/stop") && e.init?.method === "POST") {
      w.stops.push(e.url);
      if (w.stopFails) {
        const text = JSON.stringify({ error: "not_running", message: "the fleet is not running" });
        return { value: { status: 409, ok: false, headers: {}, text } };
      }
      w.status = { ...w.status, state: "done" };
      return ok({ slug: "no-more-issues", state: "done", seats: [] });
    }
    if (e.url.includes("/api/fleets/by-session/")) return w.isSeat ? ok(w.status) : notFound;
    if (e.url.includes("/api/threads")) return ok({ threads: [] });
    if (e.url.endsWith("/messages")) return ok({ inbox: [], outbox: [] });
    if (e.url.endsWith("/roster")) return ok({ roster: [] });
    return notFound;
  });
  return clock;
}

async function start($: Engine, on: On, w: World) {
  const clock = boot($, on, w);
  await $.session.start({ cwd: ROOT, surface: "terminal", isInteractive: true });
  await clock.settle();
  return clock;
}

function paneProps(bodyColumns: number, isFocused = false) {
  return {
    title: "Fleet no-more-issues",
    isFocused,
    bodyColumns,
    placement: "dock" as const,
    scroll: { offset: 0, bodyRows: 40 },
    view: {},
  };
}

async function mountPane($: Engine, surface: "terminal" | "desktop", bodyColumns: number) {
  return $.ui.mount({
    plugin: "fleet-comms",
    surface,
    component: "Pane",
    props: paneProps(bodyColumns),
    requestId: "fleet-comms",
  });
}

async function mountBand($: Engine) {
  return $.ui.mount({
    plugin: "fleet-comms",
    surface: "terminal",
    component: "AbovePrompt",
    props: {
      hasSurvey: false,
      isWorking: false,
      maxRows: 10,
      bodyColumns: 100,
      scroll: { offset: 0, bodyRows: 9 },
      view: {},
    },
    requestId: "band",
  });
}

const exact = (text: string) =>
  new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);

test("/fleet-comms in a seat draws its fleet as the panel does", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  const out = await $.command.run(RUN);
  expect(out.text).toBe("Fleet no-more-issues opened.");
  expect(w.opens.at(-1)?.title).toBe("Fleet no-more-issues");
  expect(w.urls).toContain(`${INSPECT}/api/fleets/by-session/${OWN}/status`);
  expect(w.urls.filter((u) => u.split("?")[0].endsWith("/api/fleets"))).toHaveLength(0);
  for (const surface of ["terminal", "desktop"] as const) {
    const pane = await mountPane($, surface, 150);
    const shows = async (text: string) => {
      if ((await pane.find({ type: "Text", text: exact(text) })) === undefined)
        throw new Error(`no Text reads ${JSON.stringify(text)} on ${surface}`);
    };
    await shows(" FLEET ");
    await shows("no-more-issues");
    await shows(" RUNNING ");
    await shows("The open defects in mike's fleet machinery and capture are fixed on main.");
    await shows("running for ");
    await shows("2h51m");
    await shows(" tasks accepted; ");
    await shows("pending");
    await shows("4 pass");
    await shows("0 fail");
    await shows(" seats open, ");
    await shows("criteria");
    await shows(" C1 7/7 accepted ");
    await shows(" C2 2/3 accepted ");
    await shows(" C7 0/0 accepted ");
    await shows("ACCEPTANCE OPEN");
    await shows("O-C6-final: failed — O-C6-final: final pushed head gate PASS");
    expect(await pane.find({ type: "Text", text: /O-129-264/ })).toBeUndefined();
    await shows(" NEEDS YOU ".trim());
    await shows("OPERATOR: commits are refused in a cycle");
    await shows("NOTHING IS MOVING");
    await shows("every open seat has been idle for 5m");
    await shows("TASKS");
    await shows("#517 a build brake without a declared wrapper");
    await shows("—");
    await shows("no verdict yet");
    await shows("SEATS");
    await shows("context of 1000k");
    expect(await pane.find({ type: "Text", text: /▐/ })).toBeUndefined();
    await shows("coordinator");
    await shows(" 437k");
    await shows("replace");
    await shows("read 0k wrote 80k");
    expect(await pane.find({ key: "closed-seats" })).toMatchObject({
      props: { label: "Show 1 closed seat" },
    });
    expect(await pane.find({ type: "Text", text: /backend-T2/ })).toBeUndefined();
    await shows("LATEST FROM ACCEPTANCE");
    await shows("T8 C2: the test is missing");
    await shows("BLOCKED");
    await shows("waiting on the gate slot");
    await shows("No fleet messages yet.");
    await pane.unmount();
  }
  expect(w.statuses.at(-1)).toBe(
    "NEEDS YOU · coordinator: OPERATOR: commits are refused in a cycle",
  );
  expect(w.toasts).toEqual([]);
});

test("what needed the operator at start is not toasted; a need that arrives later is, once", async ($, on) => {
  const w = world(true, { ...STATUS, needs_operator: [] });
  await start($, on, w);
  await $.command.run(RUN);
  expect(w.toasts).toEqual([]);
  w.status = STATUS;
  await $.command.run(RUN);
  await $.command.run(RUN);
  expect(w.toasts).toEqual(["NEEDS YOU: coordinator: OPERATOR: commits are refused in a cycle"]);
});

test("a docked pane keeps one line per task and drops columns as it narrows; only a very narrow one stacks", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const has = async (pane: Awaited<ReturnType<typeof mountPane>>, text: string) =>
    (await pane.find({ type: "Text", text: exact(text) })) !== undefined;
  const docked = await mountPane($, "terminal", 80);
  expect(await has(docked, "passed")).toBe(true);
  expect(await has(docked, "2 passed")).toBe(true);
  expect(await has(docked, "backend-T1")).toBe(false);
  expect(await has(docked, "last request")).toBe(true);
  expect(await has(docked, "requests")).toBe(false);
  expect(await has(docked, "─".repeat(76))).toBe(true);
  await docked.unmount();
  const narrow = await mountPane($, "terminal", 60);
  expect(await has(narrow, "passed")).toBe(true);
  expect(await has(narrow, "2 passed")).toBe(false);
  expect(await has(narrow, "last request")).toBe(false);
  expect(await has(narrow, "context of 1000k")).toBe(true);
  await narrow.unmount();
  const tiny = await mountPane($, "terminal", 40);
  expect(await has(tiny, "backend-T1")).toBe(true);
  expect(await has(tiny, "context of 1000k")).toBe(false);
  await tiny.unmount();
  const wide = await mountPane($, "terminal", 170);
  expect(await has(wide, "passed")).toBe(true);
  expect(await has(wide, "backend-T1")).toBe(true);
  expect(await has(wide, "2 passed")).toBe(true);
  expect(await has(wide, "last request")).toBe(true);
  expect(await has(wide, "first request")).toBe(true);
  await wide.unmount();
});

test("the title and the alarm panel read the same at any width; the title moves its buttons under it when narrow", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const wide = await mountPane($, "terminal", 150);
  expect(await wide.find({ type: "Text", text: exact("  this session's fleet") })).toBeDefined();
  expect(await wide.find({ key: "status" })).toMatchObject({ props: { label: "Hide details" } });
  await wide.unmount();
  const narrow = await mountPane($, "terminal", 50);
  expect(await narrow.find({ type: "Text", text: exact("this session's fleet") })).toBeDefined();
  expect(await narrow.find({ type: "Text", text: exact("NOTHING IS MOVING") })).toBeDefined();
  await narrow.unmount();
});

test("a panel is filled with the ground on the desktop and never in the terminal", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const fill = '"backgroundColor":"rgb(26,27,38)"';
  const terminal = await mountPane($, "terminal", 100);
  expect(JSON.stringify(await terminal.drawn())).not.toContain(fill);
  await terminal.unmount();
  const desktop = await mountPane($, "desktop", 100);
  expect(JSON.stringify(await desktop.drawn())).toContain(fill);
  await desktop.unmount();
});

test("Hide details drops the tables and keeps the title, summary, criteria, alarms, open acceptance and blocks", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const pane = await mountPane($, "terminal", 80);
  const has = async (text: string) =>
    (await pane.find({ type: "Text", text: exact(text) })) !== undefined;
  expect(await pane.find({ key: "status" })).toMatchObject({ props: { label: "Hide details" } });
  await pane.press({ key: "status" });
  expect(await has("no-more-issues")).toBe(true);
  expect(await has("running for ")).toBe(true);
  expect(await has("criteria")).toBe(true);
  expect(await has("NEEDS YOU")).toBe(true);
  expect(await has("NOTHING IS MOVING")).toBe(true);
  expect(await has("ACCEPTANCE OPEN")).toBe(true);
  expect(await has("BLOCKED")).toBe(true);
  expect(await has("TASKS")).toBe(false);
  expect(await has("SEATS")).toBe(false);
  expect(await has("LATEST FROM ACCEPTANCE")).toBe(false);
  expect(await has("─".repeat(76))).toBe(true);
  expect(await pane.find({ key: "status" })).toMatchObject({ props: { label: "Show details" } });
  expect(await pane.find({ key: "whole" })).toBeUndefined();
  await pane.press({ key: "status" });
  expect(await has("TASKS")).toBe(true);
  await pane.unmount();
});

test("rows are cut to their columns until Show all shows every cell whole", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const pane = await mountPane($, "terminal", 170);
  const name = () => pane.find({ type: "Text", text: exact("#517 a build brake without a declared wrapper") });
  expect((await name())?.props.wrap).toBe(CUT);
  expect(await pane.find({ key: "whole" })).toMatchObject({ props: { label: "Show all" } });
  expect(await pane.find({ type: "Text", text: exact("NEEDS YOU") })).toBeDefined();
  await pane.press({ key: "whole" });
  expect((await name())?.props.wrap).toBe("wrap");
  expect(await pane.find({ key: "whole" })).toMatchObject({ props: { label: "One line each" } });
  expect(await pane.find({ type: "Text", text: exact("backend-T2") })).toBeDefined();
  expect(await pane.find({ key: "closed-seats" })).toBeUndefined();
  await pane.unmount();
});

test("Show all in a docked pane stacks the rows so no cell is left out", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const pane = await mountPane($, "terminal", 80);
  const has = async (text: string) =>
    (await pane.find({ type: "Text", text: exact(text) })) !== undefined;
  expect(await has("backend-T1")).toBe(false);
  expect(await has("backend-T2")).toBe(false);
  await pane.press({ key: "whole" });
  expect(await has("backend-T1")).toBe(true);
  expect(await has("backend-T2")).toBe(true);
  expect(await has("last request")).toBe(false);
  await pane.unmount();
});

test("a seat reads its own fleet's status and never lists fleets", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  expect(w.urls).toContain(`${INSPECT}/api/fleets/by-session/${OWN}/status`);
  expect(w.urls.filter((u) => u.split("?")[0].endsWith("/api/fleets"))).toHaveLength(0);
  const pane = await mountPane($, "terminal", 80);
  expect(await pane.find({ type: "Text", text: exact("  this session's fleet") })).toBeDefined();
  await pane.unmount();
});

test("the band above the prompt says NEEDS YOU for a session with no fleet thread, and Hide never hides it", async ($, on) => {
  const w = world(true);
  await start($, on, w);
  await $.command.run(RUN);
  const band = await mountBand($);
  const chip = await band.find({ type: "Text", text: exact(" NEEDS YOU ") });
  expect(chip?.props.backgroundColor).toBe("rgb(247,118,142)");
  expect(chip?.props.color).toBe("rgb(26,27,38)");
  const seat = await band.find({ type: "Text", text: exact("coordinator") });
  expect(seat?.props.bold).toBe(true);
  expect(seat?.props.color).toBe("rgb(245,138,200)");
  expect(
    await band.find({ type: "Text", text: exact("OPERATOR: commits are refused in a cycle") }),
  ).toBeDefined();
  expect(await band.find({ key: "open" })).toBeDefined();
  await band.press({ key: "hide" });
  expect(await band.find({ type: "Text", text: exact(" NEEDS YOU ") })).toBeDefined();
  expect(await band.find({ key: "hide" })).toBeUndefined();
  expect(await band.find({ key: "open" })).toBeDefined();
  await band.unmount();
});

test("a done fleet's leftover needs raise no band, status line or toast; the pane alone lists them", async ($, on) => {
  const w = world(true, { ...STATUS, state: "done", finished_at: NOW_ISO });
  await start($, on, w);
  await $.command.run(RUN);
  expect(w.statuses).toEqual([undefined]);
  expect(w.toasts).toEqual([]);
  const band = await mountBand($);
  expect(await band.find({ type: "Text", text: exact(" NEEDS YOU ") })).toBeUndefined();
  expect(await band.find({ type: "Text", text: "ENGINE DRAWS THIS" })).toBeDefined();
  await band.unmount();
  const pane = await mountPane($, "terminal", 150);
  expect(
    await pane.find({ type: "Text", text: exact("OPERATOR: commits are refused in a cycle") }),
  ).toBeDefined();
  await pane.unmount();
});

test("when mike stops answering the pane keeps the last status and says when it failed", async ($, on) => {
  const w = world(true);
  const clock = await start($, on, w);
  await $.command.run(RUN);
  w.isFailing = true;
  await clock.advance(10_000);
  const pane = await mountPane($, "terminal", 80);
  expect(await pane.find({ type: "Text", text: exact("no-more-issues") })).toBeDefined();
  expect(
    await pane.find({
      type: "Text",
      text: exact(
        "mike did not answer at 10:17 UTC; this is the status it sent at 10:17 UTC.",
      ),
    }),
  ).toBeDefined();
  await pane.unmount();
});

test("a session that was not started as a fleet seat registers no command, starts no timer and asks mike nothing", async ($, on) => {
  const w = world(false);
  const clock = boot($, on, w);
  await $.session.start({ cwd: ROOT, surface: "terminal", isInteractive: true });
  await clock.advance(STATUS_MS * 3);
  await clock.settle();
  expect(w.urls).toEqual([]);
  expect(w.registered).toEqual([]);
  expect(w.statuses).toEqual([]);
  expect(w.opens).toEqual([]);
  const band = await mountBand($);
  expect(await band.find({ type: "Text", text: "ENGINE DRAWS THIS" })).toBeDefined();
  expect(await band.find({ type: "Text", text: /NEEDS YOU/ })).toBeUndefined();
  await band.unmount();
});

const DESCRIBE = {
  command: "fleet-comms",
  description: "Show the fleet in a pane",
  isHidden: false,
  immediate: false,
  provider: { plugin: "fleet-comms", tier: "user" as const },
};

function menu(on: On): string[] {
  const invalidated: string[] = [];
  on("command.describe", (_, e) => ({ description: e.description, argumentHint: e.argumentHint, isHidden: e.isHidden }));
  on("ui.invalidate", (_, e) => {
    invalidated.push(e.event);
    return { value: undefined };
  });
  return invalidated;
}

test("with no live fleet and no mail /fleet-comms is hidden from the menu, and still runs typed in full", async ($, on) => {
  const w = world(true, { ...STATUS, state: "done", finished_at: NOW_ISO });
  const invalidated = menu(on);
  await start($, on, w);
  expect((await $.command.describe(DESCRIBE)).isHidden).toBe(true);
  expect(invalidated).toEqual([]);
  const out = await $.command.run(RUN);
  expect(out.text).toBe("Fleet no-more-issues opened.");
});

test("with a running fleet /fleet-comms shows in the menu; when the fleet ends it is hidden again", async ($, on) => {
  const w = world(true);
  const invalidated = menu(on);
  const clock = await start($, on, w);
  expect((await $.command.describe(DESCRIBE)).isHidden).toBe(false);
  expect(invalidated).toEqual(["command.describe"]);
  w.status = { ...STATUS, state: "done" };
  await clock.advance(STATUS_MS);
  await clock.settle();
  expect((await $.command.describe(DESCRIBE)).isHidden).toBe(true);
  expect(invalidated).toEqual(["command.describe", "command.describe"]);
});

type Drawn = {
  type?: string;
  key?: string;
  props?: Record<string, unknown>;
  hover?: Record<string, unknown>;
  children?: (Drawn | string)[];
};

const ROW_HOVER = "rgb(41,46,66)";
const PURPLE = "rgb(187,154,247)";

function kids(node: Drawn): Drawn[] {
  return (node.children ?? []).filter((c): c is Drawn => typeof c !== "string");
}

function walk(node: Drawn): Drawn[] {
  return [node, ...kids(node).flatMap(walk)];
}

function flat(node: Drawn | string): string {
  return typeof node === "string" ? node : (node.children ?? []).map(flat).join("");
}

function cellsOf(node: Drawn): Drawn[] {
  return kids(node).flatMap((c) =>
    c.type === "Text" ? [c] : c.type === "Button" ? [] : cellsOf(c),
  );
}

type Pane = Awaited<ReturnType<typeof mountPane>>;

async function drawn(pane: Pane): Promise<Drawn> {
  return (await pane.drawn()) as Drawn;
}

async function row(pane: Pane, key: string): Promise<Drawn> {
  const found = (await pane.find({ key })) as Drawn | undefined;
  if (found === undefined) throw new Error(`no element is keyed ${key}`);
  return found;
}

async function button(pane: Pane, key: string): Promise<Drawn> {
  const found = walk(await drawn(pane)).find((n) => n.type === "Button" && n.props?.key === key);
  if (found === undefined) throw new Error(`no Button is keyed ${key}`);
  return found;
}

async function noButton(pane: Pane, key: string): Promise<boolean> {
  return !walk(await drawn(pane)).some((n) => n.type === "Button" && n.props?.key === key);
}

const withTasks = (change: (t: (typeof STATUS.tasks)[number]) => object) => ({
  ...STATUS,
  tasks: STATUS.tasks.map((t) => ({ ...t, ...change(t) })),
});

const ALL_CLOSED = {
  ...STATUS,
  every_seat_idle_seconds: null,
  needs_operator: [],
  seats: STATUS.seats.map((s) => ({ ...s, closed: true })),
};

async function fleet($: Engine, on: On, status: object) {
  await start($, on, world(true, status));
  await $.command.run(RUN);
  return (columns: number) => mountPane($, "terminal", columns);
}

test("counts read as words: passed, then failed in red, or none yet", async ($, on) => {
  const at = await fleet($, on, withTasks((t) => (t.id === "T7" ? { fails: 1 } : {})));
  const wide = await at(150);
  const coloured = async (text: string) =>
    (await wide.findAll({ type: "Text", text: exact(text) })).map((t) => t.props.color);
  expect(await coloured("1 failed")).toContain("rgb(247,118,142)");
  expect(await coloured("2 passed")).toContain("rgb(138,147,189)");
  expect(await coloured("none yet")).toContain("rgb(138,147,189)");
  expect(await wide.find({ type: "Text", text: exact("2✔ 0✘") })).toBeUndefined();
  await wide.unmount();
  const stacked = await at(40);
  expect(await stacked.find({ type: "Text", text: exact("1 failed") })).toBeDefined();
  expect(await stacked.find({ type: "Text", text: exact("none yet") })).toBeDefined();
  await stacked.unmount();
});

test("pressing a task id opens that task whole under its row, and pressing again closes it", async ($, on) => {
  const name = "#524 gate at the starting head";
  const title = "T7 C6: the gate is red at the starting head";
  const pane = await (await fleet($, on, STATUS))(150);
  const all = (t: string) => pane.findAll({ type: "Text", text: exact(t) });
  expect(await button(pane, "task-T7")).toMatchObject({
    props: { label: "T7", plain: true },
    hover: { underline: true, color: PURPLE },
  });
  expect(await all(name)).toHaveLength(1);
  await pane.press({ key: "task-T7" });
  const opened = await all(name);
  expect(opened.map((t) => t.props.wrap).sort()).toEqual([CUT, WHOLE]);
  const verdicts = await pane.findAll({ type: "Text", text: /^FAIL T7 C6: the gate is red/ });
  expect(verdicts.map((t) => t.props.wrap).sort()).toEqual([CUT, WHOLE]);
  expect(await all(title)).toHaveLength(2);
  await pane.press({ key: "whole" });
  expect((await all(name)).map((t) => t.props.wrap)).toEqual(["wrap", "wrap"]);
  await pane.press({ key: "task-T7" });
  expect(await all(name)).toHaveLength(1);
  expect(await all(title)).toHaveLength(1);
  await pane.unmount();
});

test("every task row and seat row is a keyed Box whose cells light under the pointer", async ($, on) => {
  const twin = { ...STATUS, seats: [...STATUS.seats, { ...STATUS.seats[0]!, seat: "coordinator" }] };
  const at = await fleet($, on, twin);
  const wide = await at(170);
  for (const id of ["T1", "T7", "T15"]) {
    const box = await row(wide, `task-row-${id}`);
    expect(box.type).toBe("Box");
    const cells = cellsOf(box);
    expect(cells.length).toBeGreaterThanOrEqual(8);
    for (const c of cells) expect(c.hover).toEqual({ backgroundColor: ROW_HOVER });
  }
  for (const key of ["seat-row-coordinator", "seat-row-acceptance", "seat-row-backend-T9"]) {
    const cells = cellsOf(await row(wide, key));
    expect(cells.length).toBeGreaterThanOrEqual(6);
    for (const c of cells) expect(c.hover).toEqual({ backgroundColor: ROW_HOVER });
  }
  expect((await row(wide, "seat-row-coordinator#3")).type).toBe("Box");
  const keys = walk(await drawn(wide)).flatMap((n) => (n.type === "Box" && n.key ? [n.key] : []));
  expect(new Set(keys).size).toBe(keys.length);
  await wide.unmount();
  const stacked = await at(40);
  for (const key of ["task-row-T7", "seat-row-acceptance"]) {
    const cells = cellsOf(await row(stacked, key));
    expect(cells.length).toBeGreaterThan(2);
    for (const c of cells) expect(c.hover).toEqual({ backgroundColor: ROW_HOVER });
  }
  await stacked.unmount();
});

test("the summary's separators lead the segment they belong to", async ($, on) => {
  const pane = await (await fleet($, on, STATUS))(39);
  const lead = (await pane.findAll({ type: "Text", text: /^ {2}· {2}./ })) as Drawn[];
  expect(lead.map(flat)).toEqual([
    "  ·  1 of 3 tasks accepted; 2 ready",
    "  ·  fleet acceptance pending",
    "  ·  4 pass 0 fail",
    "  ·  3 seats open, 4 launched",
  ]);
  expect(await pane.find({ type: "Text", text: /^running for / })).toBeDefined();
  expect(await pane.find({ type: "Text", text: /^ {2}· {2}running/ })).toBeUndefined();
  await pane.unmount();
});

test("at 39 columns the identity sits alone and the clock moves whole with the buttons", async ($, on) => {
  const pane = await (await fleet($, on, STATUS))(39);
  const boxes = walk(await drawn(pane)).filter((n) => n.type === "Box");
  const identity = boxes.find((b) => b.props?.flexWrap === "wrap" && flat(b).startsWith(" FLEET "));
  expect(identity).toBeDefined();
  expect(flat(identity!)).not.toContain("UTC");
  const clockBoxes = boxes.filter((b) => flat(b) === "08 Oct 10:17 UTC");
  expect(clockBoxes.some((b) => b.props?.flexShrink === 0)).toBe(true);
  const second = boxes.find(
    (b) => b.props?.columnGap === 2 && flat(b).startsWith("08 Oct 10:17 UTC"),
  );
  expect(second?.props?.flexWrap).toBe("wrap");
  expect(walk(second!).some((n) => n.props?.label === "Hide details")).toBe(true);
  for (const key of ["status", "whole", "refresh"])
    expect((await button(pane, key)).props).toMatchObject({ variant: "secondary" });
  await pane.unmount();
});

test("closed seats collapse to one Button that shows and hides their rows", async ($, on) => {
  const pane = await (await fleet($, on, STATUS))(150);
  expect(await pane.find({ key: "seat-row-backend-T2" })).toBeUndefined();
  expect(await button(pane, "closed-seats")).toMatchObject({
    props: { label: "Show 1 closed seat", hotkey: "c", variant: "secondary" },
  });
  await pane.press({ key: "closed-seats" });
  expect((await button(pane, "closed-seats")).props?.label).toBe("Hide closed seats");
  const closed = cellsOf(await row(pane, "seat-row-backend-T2"));
  expect(closed.length).toBeGreaterThan(5);
  const leaves = closed.flatMap(walk).filter(
    (n) => n.type === "Text" && (n.children ?? []).every((k) => typeof k === "string"),
  );
  expect(leaves.length).toBeGreaterThan(5);
  for (const n of leaves) expect(n.props?.color).toBe("rgb(138,147,189)");
  await pane.press({ key: "closed-seats" });
  expect(await pane.find({ key: "seat-row-backend-T2" })).toBeUndefined();
  await pane.unmount();
});

test("with every seat closed the table says so and offers the Button, with no header over nothing", async ($, on) => {
  const pane = await (await fleet($, on, ALL_CLOSED))(150);
  expect((await button(pane, "closed-seats")).props?.label).toBe("Show 4 closed seats");
  expect(await pane.find({ type: "Text", text: exact("no seat is open") })).toBeDefined();
  expect(await pane.find({ type: "Text", text: exact("context of 1000k") })).toBeUndefined();
  expect(await pane.find({ key: "seat-row-coordinator" })).toBeUndefined();
  await pane.press({ key: "closed-seats" });
  expect(await pane.find({ type: "Text", text: exact("no seat is open") })).toBeUndefined();
  expect(await pane.find({ type: "Text", text: exact("context of 1000k") })).toBeDefined();
  expect(await pane.find({ key: "seat-row-coordinator" })).toBeDefined();
  await pane.unmount();
});

test("a fleet with no seat open at all has no Button to press", async ($, on) => {
  const pane = await (await fleet($, on, { ...STATUS, seats: [] }))(150);
  expect(await noButton(pane, "closed-seats")).toBe(true);
  await pane.unmount();
});

test("a running fleet with no open seat draws NO SEAT IS OPEN in the alarm and the band", async ($, on) => {
  const w = world(true, ALL_CLOSED);
  await start($, on, w);
  await $.command.run(RUN);
  const pane = await mountPane($, "terminal", 150);
  const head = await pane.find({ type: "Text", text: exact("NO SEAT IS OPEN") });
  expect(head).toMatchObject({ props: { bold: true, color: "rgb(255,158,100)" } });
  expect(
    await pane.find({
      type: "Text",
      text: exact("1 task still in rework, 1 acceptance check open"),
    }),
  ).toBeDefined();
  expect(await pane.find({ type: "Text", text: /every open seat has been idle/ })).toBeUndefined();
  await pane.unmount();
  expect(w.statuses.at(-1)).toBe(
    "NO SEAT IS OPEN · fleet no-more-issues: 1 task still in rework, 1 acceptance check open",
  );
  const band = await mountBand($);
  expect(await band.find({ type: "Text", text: exact(" NO SEAT IS OPEN ") })).toBeDefined();
  expect(
    await band.find({
      type: "Text",
      text: exact("  1 task still in rework, 1 acceptance check open"),
    }),
  ).toBeDefined();
  await band.unmount();
});

const DIM = "rgb(138,147,189)";
const SLUG = "no-more-issues";

async function noSeatWorld($: Engine, on: On, status: object = ALL_CLOSED) {
  const w = world(true, status);
  const clock = await start($, on, w);
  await $.command.run(RUN);
  return { w, clock };
}

test("the pane's alarm row says why no seat is open, in dim, and the band row does not", async ($, on) => {
  const { w } = await noSeatWorld($, on);
  const pane = await mountPane($, "terminal", 150);
  const reason = await pane.find({ type: "Text", text: exact(noSeatReason()) });
  expect(reason?.props.color).toBe(DIM);
  await pane.unmount();
  const band = await mountBand($);
  expect(await band.find({ type: "Text", text: /passing gate/ })).toBeUndefined();
  await band.unmount();
  expect(w.statuses.at(-1)).not.toContain("passing gate");
});

test("Stop fleet asks first: Keep it puts it away without a POST", async ($, on) => {
  const { w } = await noSeatWorld($, on);
  const pane = await mountPane($, "terminal", 150);
  expect((await button(pane, "stop-fleet")).props?.label).toBe("Stop fleet");
  expect((await button(pane, "stop-fleet")).props?.hotkey).toBeUndefined();
  await pane.press({ key: "stop-fleet" });
  expect((await button(pane, "stop-fleet-yes")).props?.label).toBe(`Yes, stop ${SLUG}`);
  expect((await button(pane, "stop-fleet-no")).props?.label).toBe("Keep it");
  expect(await noButton(pane, "stop-fleet")).toBe(true);
  await pane.press({ key: "stop-fleet-no" });
  expect(await noButton(pane, "stop-fleet-yes")).toBe(true);
  expect((await button(pane, "stop-fleet")).props?.label).toBe("Stop fleet");
  expect(w.stops).toEqual([]);
  expect(w.toasts).toEqual([]);
  await pane.unmount();
});

test("Yes, stop POSTs to the fleet's stop route, says so, disarms and the alarm goes", async ($, on) => {
  const { w, clock } = await noSeatWorld($, on);
  const pane = await mountPane($, "terminal", 150);
  await pane.press({ key: "stop-fleet" });
  await pane.press({ key: "stop-fleet-yes" });
  await clock.settle();
  expect(w.stops).toEqual([`${INSPECT}/api/fleets/${FLEET}/stop`]);
  expect(w.toasts).toEqual([`Fleet ${SLUG} stopped.`]);
  expect(await noButton(pane, "stop-fleet-yes")).toBe(true);
  expect(await pane.find({ type: "Text", text: exact("NO SEAT IS OPEN") })).toBeUndefined();
  expect(w.statuses.at(-1)).toBeUndefined();
  await pane.unmount();
});

test("a stop mike refuses is told with mike's own words, and the Button is back", async ($, on) => {
  const { w, clock } = await noSeatWorld($, on);
  w.stopFails = true;
  const pane = await mountPane($, "terminal", 150);
  await pane.press({ key: "stop-fleet" });
  await pane.press({ key: "stop-fleet-yes" });
  await clock.settle();
  expect(w.toasts).toEqual([`mike did not stop ${SLUG}: the fleet is not running`]);
  expect((await button(pane, "stop-fleet")).props?.label).toBe("Stop fleet");
  expect(await pane.find({ type: "Text", text: exact("NO SEAT IS OPEN") })).toBeDefined();
  await pane.unmount();
});

test("an armed stop is not carried to another fleet", async ($, on) => {
  const { w, clock } = await noSeatWorld($, on);
  const pane = await mountPane($, "terminal", 150);
  await pane.press({ key: "stop-fleet" });
  expect(await noButton(pane, "stop-fleet-yes")).toBe(false);
  w.status = { ...ALL_CLOSED, fleet: "11111111-2222-4333-8444-555555555555" };
  await clock.advance(10_000);
  expect((await button(pane, "stop-fleet")).props?.label).toBe("Stop fleet");
  expect(await noButton(pane, "stop-fleet-yes")).toBe(true);
  await pane.unmount();
});

test("the band offers Stop fleet and Dismiss for no seat open", async ($, on) => {
  const { w, clock } = await noSeatWorld($, on);
  const band = await mountBand($);
  expect((await band.find({ key: "stop-fleet" }))?.props.label).toBe("Stop fleet");
  expect((await band.find({ key: "dismiss" }))?.props).toMatchObject({
    label: "Dismiss",
    hotkey: "d",
    variant: "secondary",
    dimColor: true,
  });
  await band.press({ key: "stop-fleet" });
  expect((await band.find({ key: "stop-fleet-yes" }))?.props.label).toBe(`Yes, stop ${SLUG}`);
  await band.press({ key: "stop-fleet-yes" });
  await clock.settle();
  expect(w.stops).toEqual([`${INSPECT}/api/fleets/${FLEET}/stop`]);
  expect(await band.find({ type: "Text", text: exact(" NO SEAT IS OPEN ") })).toBeUndefined();
  await band.unmount();
});

test("Stop fleet is offered only for no seat open", async ($, on) => {
  await noSeatWorld($, on, STATUS);
  const band = await mountBand($);
  expect(await band.find({ key: "stop-fleet" })).toBeUndefined();
  expect(await band.find({ key: "dismiss" })).toBeDefined();
  await band.unmount();
});

test("Dismiss takes the alarm off the band and the status line, and the pane still draws it", async ($, on) => {
  const { w } = await noSeatWorld($, on, STATUS);
  const band = await mountBand($);
  expect(w.statuses.at(-1)).toContain("NEEDS YOU");
  await band.press({ key: "dismiss" });
  expect(await band.find({ type: "Text", text: exact(" NEEDS YOU ") })).toBeUndefined();
  expect(await band.find({ type: "Text", text: "ENGINE DRAWS THIS" })).toBeDefined();
  expect(w.statuses.at(-1)).toBeUndefined();
  await band.unmount();
  const pane = await mountPane($, "terminal", 150);
  expect(await pane.find({ type: "Text", text: exact("NEEDS YOU") })).toBeDefined();
  expect(
    await pane.find({ type: "Text", text: exact(" (dismissed from the band)") }),
  ).toMatchObject({ props: { color: DIM } });
  await pane.unmount();
});

test("a dismissed no-seat alarm is still drawn by the pane, with the note", async ($, on) => {
  await noSeatWorld($, on);
  const band = await mountBand($);
  await band.press({ key: "dismiss" });
  expect(await band.find({ type: "Text", text: exact(" NO SEAT IS OPEN ") })).toBeUndefined();
  await band.unmount();
  const pane = await mountPane($, "terminal", 150);
  expect(await pane.find({ type: "Text", text: exact("NO SEAT IS OPEN") })).toBeDefined();
  expect(
    await pane.find({ type: "Text", text: exact(" (dismissed from the band)") }),
  ).toBeDefined();
  await pane.unmount();
});

test("a changed alarm comes back after a dismiss", async ($, on) => {
  const { w, clock } = await noSeatWorld($, on, STATUS);
  const band = await mountBand($);
  await band.press({ key: "dismiss" });
  expect(await band.find({ type: "Text", text: exact(" NEEDS YOU ") })).toBeUndefined();
  await clock.advance(10_000);
  expect(await band.find({ type: "Text", text: exact(" NEEDS YOU ") })).toBeUndefined();
  w.status = {
    ...STATUS,
    needs_operator: [
      ...STATUS.needs_operator,
      { at: NOW_ISO, seat: "acceptance", title: "OPERATOR: a second thing" },
    ],
  };
  await clock.advance(10_000);
  expect(await band.find({ type: "Text", text: exact(" NEEDS YOU ") })).toBeDefined();
  expect(w.statuses.at(-1)).toContain("(+1 more)");
  await band.unmount();
});

test("the same fleet with other counts is a new no-seat alarm", async ($, on) => {
  const { w, clock } = await noSeatWorld($, on);
  const band = await mountBand($);
  await band.press({ key: "dismiss" });
  expect(await band.find({ type: "Text", text: exact(" NO SEAT IS OPEN ") })).toBeUndefined();
  w.status = { ...ALL_CLOSED, tasks: ALL_CLOSED.tasks.map((t) => ({ ...t, state: "rework" })) };
  await clock.advance(10_000);
  expect(await band.find({ type: "Text", text: exact(" NO SEAT IS OPEN ") })).toBeDefined();
  await band.unmount();
});

test("alarmKey names the fleet, the head and the words; statusLine honours a dismissed key", () => {
  const p = panel();
  const need = attention(p)!;
  expect(alarmKey(p, need)).toBe(
    `${FLEET}|NEEDS YOU|coordinator: OPERATOR: commits are refused in a cycle`,
  );
  const same = statusLine(p);
  expect(same).toBe("NEEDS YOU · coordinator: OPERATOR: commits are refused in a cycle");
  expect(statusLine(p, null)).toBe(same);
  expect(statusLine(p, "something else")).toBe(same);
  expect(statusLine(p, alarmKey(p, need))).toBeUndefined();
  const two = { ...p, needsOperator: [...p.needsOperator, { at: NOW_ISO, seat: "x", title: "y" }] };
  expect(statusLine(two, alarmKey(p, need))).toContain("(+1 more)");
  expect(alarmKey(two, attention(two)!)).not.toBe(alarmKey(p, need));
  expect(statusLine(null, alarmKey(p, need))).toBeUndefined();
});

test("the reason and the stop words", () => {
  expect(noSeatReason()).toBe(
    "every seat has exited; mike keeps the fleet running until the newest commit on its branch has a passing gate",
  );
  const body = JSON.stringify({ error: "not_running", message: "the fleet is not running" });
  expect(stopWords(SLUG, { status: 200, ok: true, text: "{}" })).toBe(`Fleet ${SLUG} stopped.`);
  expect(stopWords(SLUG, { status: 409, ok: false, text: body })).toBe(
    `mike did not stop ${SLUG}: the fleet is not running`,
  );
  expect(stopWords(SLUG, { status: 404, ok: false, text: "" })).toBe(
    `mike did not stop ${SLUG}: 404`,
  );
  expect(stopWords(SLUG, { status: 500, ok: false, text: "{}" })).toBe(
    `mike did not stop ${SLUG}: 500`,
  );
  expect(stopWords(SLUG, null)).toBe(`mike did not stop ${SLUG}: it did not answer`);
});

test("criteria are chips: green when all accepted, yellow when some, faint when none", async ($, on) => {
  const pane = await (await fleet($, on, STATUS))(150);
  const chip = (t: string) => pane.find({ type: "Text", text: exact(t) });
  expect((await chip(" C1 7/7 accepted "))?.props).toMatchObject({
    bold: true,
    backgroundColor: "rgb(158,206,106)",
    color: "rgb(26,27,38)",
  });
  expect((await chip(" C2 2/3 accepted "))?.props).toMatchObject({
    backgroundColor: "rgb(224,175,104)",
    color: "rgb(26,27,38)",
  });
  expect((await chip(" C7 0/0 accepted "))?.props).toMatchObject({
    backgroundColor: "rgb(86,95,137)",
    color: "rgb(192,202,245)",
  });
  expect(await pane.find({ type: "Text", text: /accepted, \d+ ready/ })).toBeUndefined();
  await pane.unmount();
});
