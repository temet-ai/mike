import { expect, mock, test } from "claude-code/testing";
import type { Engine } from "claude-code/testing";
import type { On } from "claude-code";

import {
  bandFit,
  bandWidth,
  chunkPieces,
  isApplicable,
  isMailQuiet,
  chunks,
  clockTime,
  goalOf,
  isSelf,
  kindOf,
  nameColour,
  nameOf,
  newerAnchor,
  normalise,
  olderAnchor,
  pageFor,
  pagesOf,
  pickFleetThread,
  pickMailThread,
  headOf,
  roleOf,
  scrollFor,
  commandScroll,
  shortName,
  shouldFollow,
  stateColour,
  tagOf,
  toFleetThread,
  when,
} from "./register";
import type { FleetMessage, FleetThread } from "../types";
import {
  CLOSE_FRAMES,
  OPEN_FRAMES,
  SLOPE,
  TAIL,
  frontAt,
  glow,
  revealCell,
  revealLine,
  rgb,
  shownRows,
  startFrame,
  wrapLines,
} from "./mailrow";
import { letterOf, localClock } from "./mail";
import { headWidth } from "./head";

const CHIP = headWidth("fleet core") - "fleet core".length;

const SEAT = "1a2b3c4d-0000-4000-8000-000000000001";
const LAUNCHER = "9f8e7d6c-0000-4000-8000-000000000002";
const identities = [{ session_uuid: SEAT, label: "worker-T3-1a2b3c4d" }];

const OWN = "aaaa1111-0000-4000-8000-000000000001";
const COORD = "e74ba661-0000-4000-8000-000000000002";
const T3 = "9f90d652-0000-4000-8000-000000000003";
const OUTSIDE = "4cc98d77-0000-4000-8000-000000000004";
const FLEET_IDENTITIES = [
  { session_uuid: OWN, label: "acceptance-aaaa1111" },
  { session_uuid: COORD, label: "coordinator-e74ba661" },
  { session_uuid: T3, label: "rust-developer-T3-9f90d652" },
];

const PURPLE = "rgb(187,154,247)";
const GREEN = "rgb(158,206,106)";
const YELLOW = "rgb(224,175,104)";
const RED = "rgb(247,118,142)";
const PINK = "rgb(245,138,200)";
const ORANGE = "rgb(255,158,100)";

function longThread(count: number, bodyChars: number): FleetMessage[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `m${i}`,
    at: `2026-10-06T06:${String(i % 60).padStart(2, "0")}:00Z`,
    from: i % 7 === 0 ? "mike" : "worker-T3-1a2b3c4d",
    to: "everyone",
    body: `message ${i} ` + "x".repeat(bodyChars),
  }));
}

function local(day: number, hour: number, minute: number): string {
  return new Date(2026, 9, day, hour, minute).toISOString();
}

const NOW = new Date(2026, 9, 6, 12, 0).getTime();

const RUN = {
  command: "fleet-comms",
  args: "",
  origin: { kind: "composer" as const },
  presentation: { isFullscreen: true, columns: 120 },
};

type Raw = {
  message_uuid: string;
  from_session_uuid: string | null;
  to_session_uuid: string | null;
  body: string;
  queued_at: string;
};

function raw(
  id: string,
  from: string | null,
  to: string | null,
  body: string,
  at: string,
): Raw {
  return {
    message_uuid: id,
    from_session_uuid: from,
    to_session_uuid: to,
    body,
    queued_at: at,
  };
}

function fillerThread(count: number, bodyChars: number): Raw[] {
  return Array.from({ length: count }, (_, i) =>
    raw(
      `m${i}`,
      i % 7 === 0 ? null : T3,
      null,
      `message ${i} ` + "x".repeat(bodyChars),
      local(6, 5, i % 60),
    ),
  );
}

type World = {
  messages: Raw[];
  isFailing: boolean;
  isInFleet: boolean;
  isMail: boolean;
  fleetSeats: string[] | null;
  state: string;
  subject: string;
  isFocused: boolean;
  paneReads: number;
  opens: { id: string; title?: string; focus?: true; closeOnEscape?: true }[];};

function world(messages: Raw[]): World {
  return {
    messages,
    isFailing: false,
    isInFleet: true,
    isMail: false,
    fleetSeats: null,
    state: "running",
    subject: "Fleet core: the goal text",
    isFocused: false,
    paneReads: 0,
    opens: [],
  };
}

function boot($: Engine, on: On, w: World) {
  const clock = mock.clock(on, { now: NOW });
  const summary = () => ({
    thread_uuid: "b",
    subject: w.subject,
    message_count: w.messages.length,
    participant_identities: FLEET_IDENTITIES,
    last_message_at: "2026-10-06T10:00:00Z",
    fleet: w.isMail ? null : { slug: "core", state: w.state },
  });
  on("ui.render", ($, e) =>
    $.ui.resolve(e).Text({ children: "ENGINE DRAWS THIS" }),
  );
  on("session.id", () => ({ value: OWN }));
  on("env.get", (_, e) => ({ value: e.name === "MIKE_RUN_ID" ? "fleet-core-1" : undefined }));
  on("session.start", (_, e) => ({ cwd: e.cwd }));
  on("command.register", (_, e) => ({ value: { command: e.name } }));
  on("ui.open", (_, e) => {
    w.opens.push(e);
    return { value: { isPlaced: true as const } };
  });
  on("ui.panes", () => {
    w.paneReads += 1;
    return {
      value: [
        {
          id: "fleet-comms",
          title: "Fleet core",
          isShown: true,
          isFocused: w.isFocused,
          isPlaced: true,
        },
      ],
    };
  });
  on("http.fetch", (_, e) => {
    if (w.isFailing)
      return { value: { status: 500, ok: false, headers: {}, text: "" } };
    if (e.url.includes("/api/fleets/") && w.fleetSeats !== null)
      return {
        value: {
          status: 200,
          ok: true,
          headers: {},
          text: JSON.stringify({ seats: w.fleetSeats.map((seat) => ({ seat })) }),
        },
      };
    if (e.url.endsWith("/goal"))
      return {
        value: {
          status: 200,
          ok: true,
          headers: {},
          text: JSON.stringify({ name: "c4-s3" }),
        },
      };
    if (e.url.endsWith("/roster"))
      return {
        value: { status: 200, ok: true, headers: {}, text: '{"roster":[]}' },
      };
    if (e.url.includes("/api/sessions/") && e.url.endsWith("/messages"))
      return {
        value: {
          status: 200,
          ok: true,
          headers: {},
          text: JSON.stringify({
            inbox: w.isMail || w.fleetSeats !== null ? w.messages : [],
            outbox: [],
          }),
        },
      };
    const text = !w.isInFleet
      ? { threads: [] }
      : w.isMail
        ? { threads: [] }
        : e.url.includes("/api/threads/")
        ? { thread: summary(), messages: w.messages }
        : { threads: [summary()] };
    return {
      value: { status: 200, ok: true, headers: {}, text: JSON.stringify(text) },
    };
  });
  return clock;
}

async function start($: Engine, clock: { settle: () => Promise<void> }) {
  await $.session.start({
    cwd: "/tmp",
    surface: "terminal",
    isInteractive: true,
  });
  await clock.settle();
}

function paneProps(isFocused = false, bodyColumns = 80) {
  return {
    title: "Fleet core",
    isFocused,
    bodyColumns,
    placement: "dock" as const,
    scroll: { offset: 0, bodyRows: 30 },
    view: {},
  };
}

type Query = { type?: string; key?: string; text?: string | RegExp };

function exact(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
}

// find({ text }) matches a container whose descendants show the text, so a plain
// string is read as "a Text whose own text is exactly this".
function exactQuery(q: Query): Query {
  if (typeof q.text !== "string") return q;
  return { ...q, type: q.type ?? "Text", text: exact(q.text) };
}

function exactly<
  M extends {
    find: (q: Query) => unknown;
    findAll: (q: Query) => unknown;
  },
>(ui: M): M {
  return {
    ...ui,
    find: (q: Query) => ui.find(exactQuery(q)),
    findAll: (q: Query) => ui.findAll(exactQuery(q)),
  };
}

async function mountPane($: Engine, isFocused = false) {
  return exactly(
    await $.ui.mount({
      plugin: "fleet-comms",
      surface: "terminal",
      component: "Pane",
      props: paneProps(isFocused),
      requestId: "fleet-comms",
    }),
  );
}

async function mountBand(
  $: Engine,
  bodyColumns: number,
  hasSurvey = false,
  requestId = "band",
) {
  return exactly(
    await $.ui.mount({
      plugin: "fleet-comms",
      surface: "terminal",
      component: "AbovePrompt",
      props: {
        hasSurvey,
        isWorking: false,
        maxRows: 10,
        bodyColumns,
        scroll: { offset: 0, bodyRows: 9 },
        view: {},
      },
      requestId,
    }),
  );
}

type Drawn = {
  type?: string;
  props?: Record<string, unknown>;
  children?: unknown[];
};

function plain(node: unknown): string {
  if (typeof node === "string") return node;
  if (node === null || typeof node !== "object") return "";
  const el = node as Drawn;
  if (el.type === "Button") return `${el.props?.hotkey}: ${el.props?.label}`;
  if (el.type === "Client")
    return String((el.props?.props as { label?: string } | undefined)?.label ?? "");
  return (el.children ?? []).map(plain).join("");
}

async function bandRoot(band: Awaited<ReturnType<typeof mountBand>>): Promise<Drawn> {
  const drawn = (await band.drawn()) as Drawn & { part?: unknown };
  const wrapped = drawn.children?.[0] as (Drawn & { part?: unknown }) | undefined;
  return drawn.part === undefined && wrapped?.part !== undefined ? wrapped : drawn;
}

async function bandLine(band: Awaited<ReturnType<typeof mountBand>>) {
  const first = (await bandRoot(band)).children?.[0];
  const isStacked =
    typeof first === "object" && first !== null && (first as Drawn).type === "Box";
  return plain(isStacked ? (first as Drawn).children?.[0] : first);
}

async function buttonKeys(pane: Awaited<ReturnType<typeof mountPane>>) {
  const found = await pane.findAll({ type: "Button" });
  return found.map((b) => b.key);
}

test("the fleet thread is the first thread that carries a fleet", () => {
  const picked = pickFleetThread([
    { thread_uuid: "a", subject: "plain", message_count: 2, fleet: null },
    {
      thread_uuid: "b",
      subject: "Fleet core",
      message_count: 9,
      fleet: { slug: "core", state: "running" },
    },
  ]);
  expect(picked?.thread_uuid).toBe("b");
  expect(
    pickFleetThread([{ thread_uuid: "a", subject: "s", message_count: 1 }]),
  ).toBe(null);
  const done = { slug: "old", state: "done" };
  const running = { slug: "core", state: "running" };
  expect(
    pickFleetThread([
      { thread_uuid: "a", subject: "Fleet old", message_count: 3, fleet: done },
      { thread_uuid: "b", subject: "Fleet core", message_count: 1, fleet: running },
    ])?.thread_uuid,
  ).toBe("b");
  expect(
    pickFleetThread([{ thread_uuid: "a", subject: "Fleet old", message_count: 3, fleet: done }])
      ?.thread_uuid,
  ).toBe("a");
});

test("senders are named by their seat name, mike, or their session prefix", () => {
  expect(nameOf(SEAT, identities)).toBe("worker-T3-1a2b3c4d");
  expect(nameOf(null, identities)).toBe("mike");
  expect(nameOf(LAUNCHER, identities)).toBe("9f8e7d6c");
});

test("a post with no recipient is shown as to everyone, bodies are kept whole, times are local", () => {
  const long = "x".repeat(5000);
  const fleet = toFleetThread(
    {
      thread_uuid: "b",
      subject: "Fleet core",
      message_count: 2,
      participant_identities: identities,
      fleet: { slug: "core", state: "running" },
    },
    [
      {
        message_uuid: "m1",
        from_session_uuid: LAUNCHER,
        to_session_uuid: SEAT,
        body: "relay from the launcher",
        queued_at: "2026-10-05T19:30:00Z",
      },
      {
        message_uuid: "m2",
        from_session_uuid: SEAT,
        to_session_uuid: null,
        body: long,
        queued_at: "2026-10-05T19:31:00Z",
      },
    ],
    SEAT,
  );
  expect(fleet.slug).toBe("core");
  expect(fleet.messages[0]).toEqual({
    id: "m1",
    at: "2026-10-05T19:30:00Z",
    from: "9f8e7d6c",
    to: "worker-T3-1a2b3c4d",
    body: "relay from the launcher",
  });
  expect(fleet.messages[1]!.to).toBe("everyone");
  expect(fleet.messages[1]!.body.length).toBe(5000);
  expect(fleet.ownName).toBe("worker-T3-1a2b3c4d");
  expect(fleet.seatNames).toEqual(["worker-T3-1a2b3c4d"]);
  const at = new Date("2026-10-05T19:31:00Z");
  const two = (n: number) => String(n).padStart(2, "0");
  expect(clockTime("2026-10-05T19:31:00Z")).toBe(
    `${two(at.getHours())}:${two(at.getMinutes())}`,
  );
  expect(clockTime("not a time")).toBe("not a time");
});

test("the own seat name is its label, or its session prefix when it has none", () => {
  const summary = {
    thread_uuid: "b",
    subject: "Fleet core",
    message_count: 0,
    participant_identities: identities,
    fleet: { slug: "core", state: "running" },
  };
  expect(toFleetThread(summary, [], SEAT).ownName).toBe("worker-T3-1a2b3c4d");
  const stranger = toFleetThread(summary, [], LAUNCHER);
  expect(stranger.ownName).toBe("9f8e7d6c");
  expect(stranger.seatNames).toContain("9f8e7d6c");
});

test("pages are cut from the newest message back, each under the budget, every message in one page", () => {
  const messages = longThread(300, 1000);
  const pages = pagesOf(messages, 30000);
  expect(pages[0]!.end).toBe(300);
  expect(pages.at(-1)!.start).toBe(0);
  for (const [i, p] of pages.entries()) {
    const chars = messages
      .slice(p.start, p.end)
      .reduce((n, m) => n + m.body.length + 80, 0);
    expect(chars).toBeLessThanOrEqual(30000);
    if (i > 0) expect(p.end).toBe(pages[i - 1]!.start);
  }
  expect(pagesOf([], 30000)).toEqual([]);
  const [one] = pagesOf(longThread(1, 90000), 30000);
  expect(one).toEqual({ start: 0, end: 1 });
});

test("shortName drops one trailing dash and eight hex characters", () => {
  expect(shortName("rust-developer-T3-9f90d652")).toBe("rust-developer-T3");
  expect(shortName("coordinator-e74ba661")).toBe("coordinator");
  expect(shortName("mike")).toBe("mike");
  expect(shortName("4cc98d77")).toBe("4cc98d77");
  expect(shortName("coordinator-e74ba66")).toBe("coordinator-e74ba66");
  expect(shortName("coordinator-E74BA661")).toBe("coordinator-E74BA661");
});

test("roleOf drops the task number from the short name", () => {
  expect(roleOf("rust-developer-T3-9f90d652")).toBe("rust-developer");
  expect(roleOf("rust-opus-T4-0a1b2c3d")).toBe("rust-opus");
  expect(roleOf("coordinator-e74ba661")).toBe("coordinator");
  expect(roleOf("coordinator")).toBe("coordinator");
  expect(roleOf("reference-T12")).toBe("reference");
});

test("kindOf tells mike, everyone, a seat of the fleet and an outside session apart", () => {
  const seats = ["coordinator-e74ba661"];
  expect(kindOf("mike", seats)).toBe("mike");
  expect(kindOf("everyone", seats)).toBe("everyone");
  expect(kindOf("coordinator-e74ba661", seats)).toBe("seat");
  expect(kindOf("4cc98d77", seats)).toBe("outside");
});

test("nameColour follows the status pane: mike purple, known roles coloured, the rest plain", () => {
  expect(nameColour("mike")).toBe(PURPLE);
  expect(nameColour("coordinator-e74ba661")).toBe(PINK);
  expect(nameColour("rust-developer-T3-9f90d652")).toBe(ORANGE);
  expect(nameColour("frontend-developer-T1-12345678")).toBe(GREEN);
  expect(nameColour("rust-opus-T4-0a1b2c3d")).toBeUndefined();
  expect(nameColour("reference-0a1b2c3d")).toBeUndefined();
  expect(nameColour("everyone")).toBeUndefined();
  expect(nameColour("4cc98d77")).toBeUndefined();
  expect(nameColour("constructor")).toBeUndefined();
});

test("a seat is itself only by its full name, suffix included", () => {
  expect(isSelf("coordinator-e74ba661", "coordinator-e74ba661")).toBe(true);
  expect(isSelf("coordinator-e74ba662", "coordinator-e74ba661")).toBe(false);
  expect(isSelf("coordinator", "coordinator-e74ba661")).toBe(false);
  expect(isSelf("anything", "")).toBe(false);
});

test("tagOf reads Blocked: and Help: at the start, case-sensitively", () => {
  expect(tagOf("Blocked: the gate fails")).toBe("Blocked");
  expect(tagOf("  \nHelp: need a decision")).toBe("Help");
  expect(tagOf("blocked: lower case")).toBe(null);
  expect(tagOf("Not Blocked: later in the line")).toBe(null);
  expect(tagOf("[mike digest] Blocked: inside")).toBe(null);
  expect(tagOf("Blocked without a colon")).toBe(null);
});

test("when is the local time today, the date and time on another day, or the text as given", () => {
  expect(when(local(6, 6, 23), NOW)).toBe("06:23");
  expect(when(local(6, 6, 23), new Date(NOW))).toBe("06:23");
  expect(when(local(5, 23, 41), NOW)).toBe("5 Oct 23:41");
  expect(when(local(6, 0, 5), NOW)).toBe("00:05");
  expect(when("soon", NOW)).toBe("soon");
});

test("state colours follow the fleet status pane and the goal drops the fleet prefix", () => {
  expect(stateColour("running")).toBe(GREEN);
  expect(stateColour("done")).toBe(GREEN);
  expect(stateColour("failed")).toBe(RED);
  expect(stateColour("starting")).toBe(YELLOW);
  expect(goalOf("Fleet core: build it", "core")).toBe("build it");
  expect(goalOf("A subject of its own", "core")).toBe("A subject of its own");
});

const NO_CONTROL = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F\r]/;

function stripBreaks(text: string): string {
  return text.replace(/ +(?=\n|$)/g, "");
}

function joinBack(body: string): string {
  const pieces = chunkPieces(body);
  return pieces
    .map((p, i) => p.text + (i < pieces.length - 1 ? p.gap : ""))
    .join("");
}

function fenceLines(text: string): number {
  return text.split("\n").filter((l) => /^ {0,3}```/.test(l)).length;
}

test("chunks turns line endings to newlines and other control characters to U+FFFD", () => {
  const out = chunks("a\u0001b\u007Fc\r\nd\re\tf\u0000");
  expect(out).toHaveLength(1);
  expect(NO_CONTROL.test(out[0]!)).toBe(false);
  expect(out[0]).toContain("a\uFFFDb\uFFFDc");
  expect(out[0]).toContain("e\tf\uFFFD");
  expect(normalise("x\r\ny\rz")).toBe("x\ny\nz");
});

test("chunks keeps a 5000 character body as one chunk and draws line breaks as hard breaks", () => {
  const body = "x".repeat(5000);
  expect(chunks(body)).toEqual([body]);
  expect(chunks("one\ntwo\n- three\n- four")).toEqual([
    "one  \ntwo  \n- three  \n- four",
  ]);
  expect(chunks("para one\n\npara two")).toEqual(["para one\n\npara two"]);
  expect(chunks("```\na\nb\n```\nafter")).toEqual(["```\na\nb\n```\nafter"]);
  expect(chunks("")).toEqual([]);
});

test("chunks cuts at the last blank line before 9000 and drops only the newline at the cut", () => {
  const a = "a".repeat(5000);
  const b = "b".repeat(5000);
  const body = `${a}\n\n${b}`;
  const out = chunks(body);
  expect(out).toEqual([a, `\n${b}`]);
  expect(joinBack(body)).toBe(body);
});

test("chunks falls back to the last newline, then the last space, then 9000", () => {
  const lines = Array.from({ length: 12 }, (_, i) =>
    String(i % 10).repeat(999),
  );
  const byLine = lines.join("\n");
  const outLines = chunks(byLine);
  expect(outLines.length).toBeGreaterThan(1);
  for (const c of outLines) expect(c.length).toBeLessThanOrEqual(9000);
  expect(outLines[0]!.trimEnd().endsWith("7".repeat(999))).toBe(true);
  expect(stripBreaks(joinBack(byLine))).toBe(byLine);

  const words = Array.from({ length: 1200 }, () => "abcdefghi").join(" ");
  const outWords = chunks(words);
  expect(outWords.length).toBe(2);
  expect(outWords[0]!.length).toBeLessThanOrEqual(9000);
  expect(outWords[0]!.endsWith("abcdefghi")).toBe(true);
  expect(outWords[1]!.startsWith("abcdefghi")).toBe(true);
  expect(joinBack(words)).toBe(words);

  const solid = "z".repeat(25000);
  expect(chunks(solid).map((c) => c.length)).toEqual([9000, 9000, 7000]);
  expect(joinBack(solid)).toBe(solid);
});

test("chunks keeps a fenced block intact across a cut and joins back to the body", () => {
  const code = Array.from({ length: 12 }, () => "f".repeat(1000)).join("\n");
  const body = `intro\n\`\`\`ts\n${code}\n\`\`\`\noutro`;
  const out = chunks(body);
  expect(out.length).toBe(2);
  expect(out[0]!.endsWith("\n```")).toBe(true);
  expect(out[1]!.startsWith("```ts\n")).toBe(true);
  for (const c of out) {
    expect(fenceLines(c) % 2).toBe(0);
    expect(c.length).toBeLessThanOrEqual(10000);
  }
  expect(stripBreaks(joinBack(body))).toBe(body);
  const pieces = chunkPieces(body);
  expect(pieces[0]!.closer).toBe("\n```");
  expect(pieces[1]!.opener).toBe("```ts");
  expect(pieces[1]!.closer).toBe("");
});

test("every chunk of a large mixed body is under the Markdown limit and the body joins back", () => {
  const body = Array.from(
    { length: 400 },
    (_, i) => `- T${i} (seat-${i}): working, idle since 06:22:33Z, ready false`,
  ).join("\n");
  const out = chunks(body);
  expect(out.length).toBeGreaterThan(1);
  for (const c of out) expect(c.length).toBeLessThanOrEqual(10000);
  expect(stripBreaks(joinBack(body))).toBe(body);
});

test("paging by anchor: Older then Newer returns to the same page, a new message does not move it, an unknown anchor reads as newest", () => {
  const messages = longThread(300, 1000);
  const newest = pageFor(messages, null);
  expect(newest).toEqual(pagesOf(messages)[0]);

  const toSecond = olderAnchor(messages, newest);
  expect(toSecond).toBe(messages[newest.start - 1]!.id);
  const second = pageFor(messages, toSecond);
  expect(second.end).toBe(newest.start);

  const toThird = olderAnchor(messages, second);
  const third = pageFor(messages, toThird);
  expect(third.end).toBe(second.start);

  expect(newerAnchor(messages, third)).toBe(toSecond);
  expect(pageFor(messages, newerAnchor(messages, third))).toEqual(second);
  expect(newerAnchor(messages, second)).toBe(null);
  expect(pageFor(messages, newerAnchor(messages, second))).toEqual(newest);

  const arrived = [
    ...messages,
    {
      id: "m300",
      at: messages[0]!.at,
      from: "mike",
      to: "everyone",
      body: "new",
    },
  ];
  expect(pageFor(arrived, toThird)).toEqual(third);
  expect(pageFor(arrived, null).end).toBe(301);

  expect(pageFor(messages, "no-such-message")).toEqual(newest);
  const oldest = pagesOf(messages).at(-1)!;
  expect(olderAnchor(messages, oldest)).toBe(null);
});

test("the pane scrolls to the end on open, Older and Latest and to the start on Newer, and follows new messages only while unfocused on the newest page", () => {
  expect(scrollFor("open")).toBe("end");
  expect(scrollFor("older")).toBe("end");
  expect(scrollFor("latest")).toBe("end");
  expect(scrollFor("arrival")).toBe("end");
  expect(scrollFor("newer")).toBe("start");
  expect(scrollFor("status")).toBe("start");
  expect(scrollFor(commandScroll(true))).toBe("start");
  expect(scrollFor(commandScroll(false))).toBe("end");
  expect(shouldFollow(true, null, false)).toBe(true);
  expect(shouldFollow(true, null, true)).toBe(false);
  expect(shouldFollow(true, "m12", false)).toBe(false);
  expect(shouldFollow(false, null, false)).toBe(false);
  expect(shouldFollow(true, null, undefined)).toBe(false);
});

test("the pane draws one page under the engine limit, opens on the newest page and pages back to older messages", async ($, on) => {
  const w = world(fillerThread(300, 1000));
  const clock = boot($, on, w);
  await start($, clock);
  const ran = await $.command.run(RUN);
  expect(ran).toEqual({ text: "Fleet core opened.", ref: undefined });
  expect(w.opens).toEqual([
    {
      id: "fleet-comms",
      title: "Fleet core",
      focus: true,
      closeOnEscape: true,
    },
  ]);

  const messages = fillerThread(300, 1000).map((m): FleetMessage => ({
    id: m.message_uuid,
    at: m.queued_at,
    from: "x",
    to: "x",
    body: m.body,
  }));
  const first = pagesOf(messages)[0]!;
  const pane = await mountPane($);
  expect(JSON.stringify(await pane.drawn()).length).toBeLessThan(100000);
  expect(
    await pane.find({
      text: new RegExp(`^messages ${first.start + 1}–300 of 300$`),
    }),
  ).toBeDefined();
  expect(await pane.find({ text: /^message 299 / })).toBeDefined();
  expect(await pane.find({ text: /^message 0 / })).toBeUndefined();
  expect(await pane.find({ text: "MESSAGES" })).toBeDefined();
  expect(
    await pane.find({ text: new RegExp(`^${first.start + 1}–300 of 300$`) }),
  ).toBeDefined();
  expect(await buttonKeys(pane)).toEqual(["goal", "older-top", "older-bottom"]);
  expect(await pane.find({ key: "older-top" })).toBeDefined();

  await pane.press({ key: "older-top" });
  expect(JSON.stringify(await pane.drawn()).length).toBeLessThan(100000);
  expect(await pane.find({ text: /^message 299 / })).toBeUndefined();
  expect(
    await pane.find({ text: new RegExp(`^message ${first.start - 1} `) }),
  ).toBeDefined();
  expect(await buttonKeys(pane)).toEqual([
    "goal",
    "older-top",
    "newer-top",
    "latest-top",
    "older-bottom",
    "newer-bottom",
    "latest-bottom",
  ]);

  await pane.press({ key: "newer-bottom" });
  expect(await pane.find({ text: /^message 299 / })).toBeDefined();
  expect(await buttonKeys(pane)).toEqual(["goal", "older-top", "older-bottom"]);

  await pane.press({ key: "older-top" });
  await pane.press({ key: "older-top" });
  await pane.press({ key: "latest-top" });
  expect(await pane.find({ text: /^message 299 / })).toBeDefined();
  expect(await pane.find({ text: /^message 0 / })).toBeUndefined();
});

test("a page that was reached by Older does not move when a new message arrives, and a new one scrolls an unfocused pane to the end", async ($, on) => {
  const w = world(fillerThread(300, 1000));
  const clock = boot($, on, w);
  await start($, clock);
  const pane = await mountPane($);
  await pane.press({ key: "older-top" });
  const before = await pane.find({ text: /^messages \d+–\d+ of 300$/ });
  expect(before).toBeDefined();
  const range = before!.text.replace(" of 300", "");

  w.messages = [
    ...w.messages,
    raw("m300", T3, OWN, "a late one", local(6, 7, 0)),
  ];
  await clock.advance(5000);
  expect(await pane.find({ text: `${range} of 301` })).toBeDefined();
  expect(await pane.find({ text: /^a late one/ })).toBeUndefined();
  expect(w.paneReads).toBe(0);

  await pane.press({ key: "latest-top" });
  expect(await pane.find({ text: /^a late one/ })).toBeDefined();
  w.messages = [...w.messages, raw("m301", T3, OWN, "another", local(6, 7, 1))];
  await clock.advance(5000);
  expect(w.paneReads).toBe(1);

  w.isFocused = true;
  w.messages = [
    ...w.messages,
    raw("m302", T3, OWN, "and another", local(6, 7, 2)),
  ];
  await clock.advance(5000);
  expect(w.paneReads).toBe(2);
});

test("the header shows the FLEET badge, slug and state, and the goal stays hidden until g", async ($, on) => {
  const w = world(fillerThread(3, 40));
  const clock = boot($, on, w);
  await start($, clock);
  const pane = await mountPane($);
  const badge = await pane.find({ text: " FLEET " });
  expect(badge?.props).toMatchObject({
    bold: true,
    backgroundColor: PURPLE,
    color: "rgb(26,27,38)",
  });
  expect((await pane.find({ text: "core" }))?.props).toMatchObject({
    bold: true,
  });
  expect((await pane.find({ text: "running" }))?.props.color).toBe(GREEN);
  expect(await pane.find({ text: "the goal text" })).toBeUndefined();
  expect((await pane.find({ key: "goal" }))?.props.label).toBe("Show goal");
  expect((await pane.find({ key: "goal" }))?.props.hotkey).toBe("g");

  await pane.press({ key: "goal" });
  expect(await pane.find({ text: "the goal text" })).toBeDefined();
  expect((await pane.find({ key: "goal" }))?.props.label).toBe("Hide goal");

  w.state = "failed";
  await clock.advance(5000);
  expect((await pane.find({ text: "failed" }))?.props.color).toBe(RED);
  expect(await pane.find({ text: "the goal text" })).toBeDefined();
  await pane.press({ key: "goal" });
  expect(await pane.find({ text: "the goal text" })).toBeUndefined();
});

test("a message shows time, who to whom, tags, marks and mike's dim body", async ($, on) => {
  const w = world([
    raw(
      "m0",
      null,
      OWN,
      "[mike digest] Fleet core at 06:30:13Z.\n- T1: working",
      local(6, 6, 30),
    ),
    raw("m1", T3, OWN, "Blocked: waiting on report.rs", local(6, 6, 31)),
    raw("m2", OUTSIDE, null, "hello from outside", local(5, 23, 41)),
    raw("m3", OWN, COORD, "Help: need a decision", local(6, 6, 33)),
  ]);
  const clock = boot($, on, w);
  await start($, clock);
  const pane = await mountPane($);

  const marks = await pane.findAll({ text: " (this seat)" });
  expect(marks).toHaveLength(3);
  expect(await pane.find({ text: " (outside the fleet)" })).toBeDefined();
  expect(await pane.find({ text: "4cc98d77" })).toBeDefined();
  expect((await pane.find({ text: "mike" }))?.props.color).toBe(PURPLE);
  expect((await pane.find({ text: "rust-developer-T3" }))?.props).toMatchObject(
    { color: ORANGE, bold: true },
  );
  expect((await pane.find({ text: "coordinator" }))?.props.color).toBe(PINK);
  expect(await pane.find({ text: "everyone" })).toBeDefined();
  expect(await pane.find({ text: "06:30" })).toBeDefined();
  expect(await pane.find({ text: "5 Oct 23:41" })).toBeDefined();

  const blocked = await pane.findAll({ text: "  Blocked" });
  expect(blocked).toHaveLength(1);
  expect(blocked[0]!.props).toMatchObject({ bold: true, color: RED });
  const help = await pane.findAll({ text: "  Help" });
  expect(help).toHaveLength(1);
  expect(help[0]!.props).toMatchObject({ bold: true, color: YELLOW });

  const bodies = await pane.findAll({ type: "Markdown" });
  expect(bodies).toHaveLength(4);
  expect(bodies[0]!.props.dimColor).toBe(true);
  expect(bodies[1]!.props.dimColor).toBeFalsy();
  expect(bodies[1]!.props.text).toBe("Blocked: waiting on report.rs");
  expect(bodies[0]!.props.text).toBe(
    "[mike digest] Fleet core at 06:30:13Z.  \n- T1: working",
  );
});

test("a pane with no messages says so once, and the other states read as the spec says", async ($, on) => {
  const w = world([]);
  const clock = boot($, on, w);
  const pane = await mountPane($);
  expect((await pane.drawn()) as Drawn).toMatchObject({
    props: { dimColor: true },
    children: ["Loading fleet messages…"],
  });

  await start($, clock);
  expect(await pane.findAll({ text: "no messages yet" })).toHaveLength(1);
  expect(await buttonKeys(pane)).toEqual(["goal"]);
  const ran = await $.command.run(RUN);
  expect(ran).toEqual({ text: "Fleet core opened.", ref: undefined });

  w.isFailing = true;
  await clock.advance(5000);
  const error = await pane.find({ text: /^mike did not answer at / });
  expect(error?.text).toMatch(
    /^mike did not answer at \d\d:\d\d\. Showing what it sent at \d\d:\d\d\.$/,
  );
  expect(error?.props.color).toBe(RED);
  w.isFailing = false;
  await clock.advance(5000);
  expect(await pane.find({ text: /^mike did not answer/ })).toBeUndefined();

  w.isInFleet = false;
  await clock.advance(5000);
  expect(
    (await pane.find({ text: /^mike has no fleet status or mail/ }))?.text,
  ).toBe(
    "mike has no fleet status or mail for this seat yet.",
  );
  w.opens.length = 0;
  expect(await $.command.run(RUN)).toEqual({
    text: "mike has no fleet status or mail for this seat yet.",
    ref: undefined,
  });
  expect(w.opens).toEqual([]);
});

test("when the first fetch fails the pane and the command say mike did not answer, and open nothing", async ($, on) => {
  const w = world(fillerThread(2, 10));
  w.isFailing = true;
  const clock = boot($, on, w);
  await start($, clock);
  const pane = await mountPane($);
  expect((await pane.drawn()) as Drawn).toMatchObject({
    props: { color: RED },
    children: ["mike did not answer on http://localhost:9989."],
  });
  const ran = await $.command.run(RUN);
  expect(ran).toEqual({
    text: "mike did not answer on http://localhost:9989.",
    ref: undefined,
  });
  expect(w.opens).toEqual([]);
  const band = await mountBand($, 120);
  expect(await bandLine(band)).toBe("ENGINE DRAWS THIS");
});

const BAND_FULL =
  "fleet core · 253 messages · latest 06:23 coordinator → rust-developer-T3  Blocked";
const BAND_NO_COUNT =
  "fleet core · latest 06:23 coordinator → rust-developer-T3  Blocked";
const BAND_NO_TO = "fleet core · latest 06:23 coordinator  Blocked";
const BAND_NAME_ONLY = "fleet core";

function bandWorld(): World {
  const messages = fillerThread(252, 20);
  messages.push(
    raw(
      "last",
      COORD,
      T3,
      "Blocked: the gate fails at 6e9d0f48",
      local(6, 6, 23),
    ),
  );
  return world(messages);
}

test("the band reads as the spec at full width and drops the count, then the recipient, as the column narrows", async ($, on) => {
  const w = bandWorld();
  const clock = boot($, on, w);
  await start($, clock);
  const buttons = "o: Openh: Hide";

  const wide = await mountBand($, 200);
  expect(await bandLine(wide)).toBe(BAND_FULL);
  expect((await bandRoot(wide)).children?.[0]).toMatchObject({
    type: "Box",
    props: { columnGap: 2 },
  });
  expect((await wide.find({ key: "open" }))?.props).toMatchObject({
    label: "Open",
    hotkey: "o",
    variant: "secondary",
  });
  expect((await wide.find({ key: "hide" }))?.props).toMatchObject({
    label: "Hide",
    hotkey: "h",
    variant: "secondary",
    dimColor: true,
  });
  expect((await wide.find({ text: "coordinator" }))?.props).toMatchObject({
    color: PINK,
    bold: true,
  });
  expect((await wide.find({ text: "  Blocked" }))?.props.color).toBe(RED);

  await wide.unmount();

  const cases: [number, string][] = [
    [BAND_FULL.length + CHIP + 18, BAND_FULL],
    [BAND_FULL.length + CHIP + 17, BAND_NO_COUNT],
    [BAND_NO_COUNT.length + CHIP + 18, BAND_NO_COUNT],
    [BAND_NO_COUNT.length + CHIP + 17, BAND_NO_TO],
    [BAND_NO_TO.length + CHIP + 18, BAND_NO_TO],
    [BAND_NO_TO.length + CHIP + 17, BAND_NAME_ONLY],
    [40, BAND_NAME_ONLY],
    [10, BAND_NAME_ONLY],
  ];
  for (const [columns, expected] of cases) {
    const band = await mountBand($, columns);
    expect(await bandLine(band)).toBe(expected);
    expect(await band.find({ key: "open" })).toBeDefined();
    expect(await band.find({ key: "hide" })).toBeDefined();
    await band.unmount();
  }
  expect(buttons.length).toBe(14);

  const thread = (await buildThread()) as FleetThread;
  expect(bandWidth(thread, NOW, "full")).toBe(BAND_FULL.length + CHIP);
  expect(bandWidth(thread, NOW, "noCount")).toBe(BAND_NO_COUNT.length + CHIP);
  expect(bandWidth(thread, NOW, "noTo")).toBe(BAND_NO_TO.length + CHIP);
  expect(bandWidth(thread, NOW, "noLatest")).toBe(BAND_NAME_ONLY.length + CHIP);
  expect(bandFit(thread, NOW, BAND_FULL.length + CHIP + 18)).toBe("full");
  expect(bandFit(thread, NOW, BAND_FULL.length + CHIP + 17)).toBe("noCount");
  expect(bandFit(thread, NOW, BAND_NO_COUNT.length + CHIP + 17)).toBe("noTo");
  expect(bandFit(thread, NOW, BAND_NO_TO.length + CHIP + 17)).toBe("noLatest");
  expect(bandFit(thread, NOW, 5)).toBe("noLatest");
});

async function buildThread(): Promise<FleetThread> {
  const w = bandWorld();
  return toFleetThread(
    {
      thread_uuid: "b",
      subject: w.subject,
      message_count: w.messages.length,
      participant_identities: FLEET_IDENTITIES,
      fleet: { slug: "core", state: "running" },
    },
    w.messages,
    OWN,
  );
}

test("the band names this seat and an outside sender, and reads 'no messages yet' for an empty fleet", async ($, on) => {
  const w = world([raw("a", OWN, null, "Help: anyone?", local(6, 6, 23))]);
  const clock = boot($, on, w);
  await start($, clock);
  const band = await mountBand($, 200);
  expect(await bandLine(band)).toBe(
    "fleet core · 1 message · latest 06:23 acceptance (this seat) → everyone  Help",
  );
  expect((await band.find({ text: "  Help" }))?.props.color).toBe(YELLOW);

  w.messages = [raw("b", OUTSIDE, OWN, "ping", local(5, 23, 41))];
  await clock.advance(5000);
  expect(await bandLine(band)).toBe(
    "fleet core · 1 message · latest 5 Oct 23:41 4cc98d77 (outside the fleet) → acceptance (this seat)",
  );

  w.messages = [];
  await clock.advance(5000);
  expect(await bandLine(band)).toBe("fleet core · no messages yet");
});

test("the band is not drawn while loading, hidden or under a survey, and /fleet-comms shows it again", async ($, on) => {
  const w = bandWorld();
  const clock = boot($, on, w);
  const loading = await mountBand($, 200);
  expect(await bandLine(loading)).toBe("ENGINE DRAWS THIS");
  expect(await loading.find({ key: "open" })).toBeUndefined();
  await loading.unmount();

  await start($, clock);
  const band = await mountBand($, 200);
  expect(await band.find({ key: "open" })).toBeDefined();
  const surveyed = await mountBand($, 200, true, "survey");
  expect(await bandLine(surveyed)).toBe("ENGINE DRAWS THIS");

  await band.press({ key: "hide" });
  expect(await bandLine(band)).toBe("ENGINE DRAWS THIS");
  expect(await band.find({ key: "open" })).toBeUndefined();

  await $.command.run(RUN);
  expect(await band.find({ key: "open" })).toBeDefined();
  expect(await bandLine(band)).toBe(BAND_FULL);
});

test("the band's Open button opens the pane and takes the keyboard", async ($, on) => {
  const w = bandWorld();
  const clock = boot($, on, w);
  await start($, clock);
  const band = await mountBand($, 200);
  await band.press({ key: "open" });
  expect(w.opens).toEqual([
    {
      id: "fleet-comms",
      title: "Fleet core",
      focus: true,
      closeOnEscape: true,
    },
  ]);
});

test("pickMailThread picks the newest mailbox thread: older threads lose, a fleet thread wins", () => {
  const old = { thread_uuid: "a", subject: "old", message_count: 1, last_message_at: "2026-10-01T00:00:00Z" };
  const fresh = { thread_uuid: "b", subject: "fresh", message_count: 2, last_message_at: "2026-10-05T00:00:00Z" };
  const fleet = { thread_uuid: "c", subject: "Fleet x", message_count: 9, last_message_at: "2026-09-01T00:00:00Z", fleet: { slug: "x", state: "running" } };
  expect(pickMailThread([old, fresh])?.thread_uuid).toBe("b");
  expect(pickMailThread([old, fresh, fleet])?.thread_uuid).toBe("c");
  expect(pickMailThread([])).toBe(null);
  expect(headOf(toFleetThread(fresh, [], OWN))).toBe("mail");
  expect(headOf(toFleetThread(fleet, [], OWN))).toBe("fleet x");
});

test("a seat with no fleet thread sees its mail in the band and a MAIL pane, and /fleet-comms opens it", async ($, on) => {
  const w = world(fillerThread(3, 40));
  w.isMail = true;
  w.subject = "Rebase please";
  const clock = boot($, on, w);
  await start($, clock);

  const band = await mountBand($, 120);
  expect(await bandLine(band)).toMatch(/^mail · 3 messages · latest /);

  const pane = await mountPane($);
  expect((await pane.find({ text: " MAIL " }))?.props).toMatchObject({ bold: true });
  expect(await pane.find({ text: " FLEET " })).toBeUndefined();
  expect(await pane.find({ text: "Mailbox" })).toBeDefined();
  expect(await pane.findAll({ text: /\(this seat\)|outside the fleet/ })).toEqual([]);
  expect(await pane.find({ key: "goal" })).toBeUndefined();

  const ran = await $.command.run(RUN);
  expect(ran).toEqual({ text: "Mail opened.", ref: undefined });
  expect(w.opens.at(-1)?.title).toBe("Mail");
});

test("a seat mailed in another fleet's thread sees plain mail, with no fleet, seat or state words", async ($, on) => {
  const w = world(fillerThread(2, 40));
  w.fleetSeats = ["coordinator", "backend-T1"];
  const clock = boot($, on, w);
  await start($, clock);
  const band = await mountBand($, 120);
  expect(await bandLine(band)).toMatch(/^mail · /);
  const pane = await mountPane($);
  expect(await pane.find({ text: " FLEET " })).toBeUndefined();
  expect(await pane.find({ text: "running" })).toBeUndefined();
});

test("a seat of the fleet thread keeps the fleet band", async ($, on) => {
  const w = world(fillerThread(2, 40));
  w.fleetSeats = ["acceptance", "coordinator"];
  const clock = boot($, on, w);
  await start($, clock);
  const band = await mountBand($, 120);
  expect(await bandLine(band)).toMatch(/^fleet core · /);
});

test("mail sent without a subject, so in no thread, shows in the band and the pane", async ($, on) => {
  const w = world([raw("u1", T3, OWN, "ping, no subject", local(6, 9, 0))]);
  w.isMail = true;
  const clock = boot($, on, w);
  await start($, clock);
  const band = await mountBand($, 120);
  expect(await bandLine(band)).toMatch(/^mail · 1 message · latest /);
  const pane = await mountPane($);
  expect(await pane.find({ text: /ping, no subject/ })).toBeDefined();
  expect(await $.command.run(RUN)).toEqual({ text: "Mail opened.", ref: undefined });
});

async function mountMailRow($: Engine, text: string, requestId: string) {
  return $.ui.mount({
    plugin: "fleet-comms",
    surface: "terminal",
    component: "UserMessage",
    props: {
      text,
      origin: { kind: "channel", server: "mike" },
      isExpanded: false,
      from: { name: "mike" },
    },
    requestId,
  });
}

test("a mail row in the conversation opens in place, and the band's head opens the latest one there, never under the band", async ($, on) => {
  const long = "Schedules and charges manager role.\n\n" + "word ".repeat(400) + "the end";
  const w = world([raw("u1", T3, OWN, long, local(6, 9, 0))]);
  w.isMail = true;
  const clock = boot($, on, w);
  await start($, clock);

  const row = await mountMailRow($, long, "row-1");
  expect(await row.find({ type: "Client", key: "mail-row" })).toMatchObject({
    props: { module: "hooks/mailrow.tsx", props: { body: long, from: "mike", isOpen: false } },
  });
  await row.post({ toggle: true }, { in: "mail-row" });
  expect(await row.find({ type: "Client", key: "mail-row" })).toMatchObject({
    props: { props: { isOpen: true } },
  });
  await row.post({ toggle: true }, { in: "mail-row" });
  expect(await row.find({ type: "Client", key: "mail-row" })).toMatchObject({
    props: { props: { isOpen: false } },
  });

  const band = await mountBand($, 80);
  expect(await band.find({ type: "Client", key: "head" })).toMatchObject({
    props: { module: "hooks/head.tsx", props: { label: "mail", isOpen: false } },
  });
  await band.post({ toggle: true }, { in: "head" });
  expect(await row.find({ type: "Client", key: "mail-row" })).toMatchObject({
    props: { props: { isOpen: true } },
  });
  expect(await band.find({ type: "Client", key: "head" })).toBeUndefined();
  expect(await band.find({ text: /word word/ })).toBeUndefined();
});

test("plain mail leaves the band once it is read or hidden, comes back with new mail, and mail this session sent never raises it", () => {
  const thread = (from: string, id: string): FleetThread => ({
    threadUuid: "t",
    slug: "",
    state: "",
    subject: "s",
    messageCount: 1,
    messages: [{ id, at: "2026-10-16T13:00:00Z", from, to: "me", body: "hi" }],
    ownName: "me",
    seatNames: ["me", "them"],
  });
  expect(isMailQuiet(thread("them", "m1"), null)).toBe(false);
  expect(isMailQuiet(thread("them", "m1"), "m1")).toBe(true);
  expect(isMailQuiet(thread("them", "m2"), "m1")).toBe(false);
  expect(isMailQuiet(thread("me", "m3"), null)).toBe(true);
  expect(isMailQuiet({ ...thread("them", "m1"), slug: "core", state: "running" }, "m1")).toBe(false);
  expect(isMailQuiet({ ...thread("them", "m1"), slug: "core", state: "done" }, null)).toBe(true);
});

test("/fleet-comms applies while there is unread mail or a live fleet, and not before mike has answered", () => {
  const mail: FleetThread = {
    threadUuid: "t",
    slug: "",
    state: "",
    subject: "s",
    messageCount: 1,
    messages: [{ id: "m1", at: "2026-10-16T13:00:00Z", from: "them", to: "me", body: "hi" }],
    ownName: "me",
    seatNames: ["me", "them"],
  };
  expect(isApplicable("loading", null, null)).toBe(false);
  expect(isApplicable(null, null, null)).toBe(false);
  expect(isApplicable(mail, null, null)).toBe(true);
  expect(isApplicable(mail, "m1", null)).toBe(false);
});

test("a row that is not mike's mail is drawn the usual way, and the band opens the pane when the mail has no row", async ($, on) => {
  const w = world([raw("u1", T3, OWN, "no row for this one", local(6, 9, 0))]);
  w.isMail = true;
  const clock = boot($, on, w);
  await start($, clock);
  const prompt = await $.ui.mount({
    plugin: "fleet-comms",
    surface: "terminal",
    component: "UserMessage",
    props: { text: "hello", origin: { kind: "composer" }, isExpanded: false },
    requestId: "row-2",
  });
  expect(await prompt.find({ type: "Client", key: "mail-row" })).toBeUndefined();
  const band = await mountBand($, 80);
  const opened = w.opens.length;
  await band.post({ toggle: true }, { in: "head" });
  expect(w.opens.length).toBe(opened + 1);
});

test("a mail row shows the letter without mike's delivery wrapper, and its sender and local time in the header", () => {
  const now = new Date(2026, 9, 16, 15, 0);
  const text = [
    "<system-reminder>",
    "Cross-session message from session 09051656-cc15-4e96-91f6-f11164b85d70 (message fc076da0-564b-4ad7-bf60-189c04e400af), sent 13:22:48 UTC.",
    "",
    "Schedules and charges manager role → Caretaker.",
    "",
    "Second paragraph.",
    "",
    'Reply: `send_session_message` to="09051656-cc15-4e96-91f6-f11164b85d70" request_reply=true.',
    "</system-reminder>",
  ].join("\n");
  const letter = letterOf(text, now);
  expect(letter.body).toBe("Schedules and charges manager role → Caretaker.\n\nSecond paragraph.");
  expect(letter.from).toBe("09051656");
  expect(letter.at).toBe(localClock(13, 22, now));
  expect(letterOf("[mike mail] plain", now)).toEqual({ body: "[mike mail] plain", from: null, at: null });
  const first = letterOf(
    "<system-reminder>\nMail from another session arrives as 'Mail from <name> (<id>): <text>'. Reply with send_session_message to that id.\n\nMail from judge-backlog (session 09051656-aaaa-4bbb-8ccc-000000000001): okay\n</system-reminder>",
    now,
  );
  expect(first).toEqual({ body: "okay", from: "09051656", at: localClock(now.getUTCHours(), now.getUTCMinutes(), now) });
  expect(letterOf("Mail from session 09051656-aaaa-4bbb-8ccc-000000000001: okay", now).from).toBe("09051656");
  expect(letterOf("Mail from alpha (1a2b3c4d-0000-4000-8000-000000000001): go", now).from).toBe("1a2b3c4d");
  expect(letterOf("Mail from an unknown sender: hi", now)).toMatchObject({ body: "hi", from: "anonymous" });
  expect(startFrame(true, 1000, 1500)).toBe(0);
  expect(startFrame(true, 1000, 5000)).toBe(OPEN_FRAMES);
  expect(startFrame(false, 0, 5000)).toBe(CLOSE_FRAMES);
});

test("the mail row wraps every word, sweeps the letters in from scrambled glyphs and settles to plain ink", () => {
  const body = "alpha beta gamma\n\n" + "x".repeat(25);
  const lines = wrapLines(body, 10);
  expect(lines).toEqual(["alpha beta", "gamma", "", "xxxxxxxxxx", "xxxxxxxxxx", "xxxxx"]);
  expect(lines.join("").replace(/\s/g, "")).toBe(body.replace(/\s/g, ""));
  const opening = { isOpen: true, frame: 0, hover: false };
  expect(shownRows(opening, 6)).toBe(1);
  expect(shownRows({ ...opening, frame: OPEN_FRAMES }, 6)).toBe(6);
  expect(shownRows({ isOpen: false, frame: CLOSE_FRAMES, hover: false }, 6)).toBe(0);
  const front = frontAt(OPEN_FRAMES / 3, 10, 6);
  const cells = revealLine("alpha beta", 0, 10, front, 4);
  expect(cells.map((c) => c.text).join("").length).toBe(10);
  expect(revealCell("a", 100, 10, 0, 0, 1).text).toBe(" ");
  expect(revealCell("a", 12, 10, 0, 0, 1).text).not.toBe("a");
  expect(revealCell("a", 10, 10, 0, 0, 1)).toMatchObject({ text: "a", colour: rgb(glow(0)), bold: true });
  expect(glow(1000)).toEqual(glow(TAIL));
  expect(frontAt(OPEN_FRAMES, 10, 6)).toBe(10 + 6 * SLOPE + TAIL);
});

test("the sender of mail from another project is named from mike, not by an id prefix", async ($, on) => {
  const w = world([raw("u2", OUTSIDE, OWN, "hello from afar", local(6, 9, 5))]);
  w.isMail = true;
  const clock = boot($, on, w);
  await start($, clock);
  const band = await mountBand($, 120);
  expect(await bandLine(band)).toMatch(/latest \d\d:\d\d c4-s3/);
});
