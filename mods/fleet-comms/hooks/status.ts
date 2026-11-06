import type {
  FleetCriterion,
  FleetPanel,
  FleetSeat,
  FleetTask,
  FleetVerdict,
} from "../types";
import { roleColour } from "./ink";
import type { InkKey } from "./ink";

export const STATUS_MS = 10_000;
export const WINDOW = 1_000_000;
export const REPLACE_AT = 600_000;
export const WARN_AT = 400_000;
export const CELLS = 20;
const AGO_SECONDS_UNTIL_MINUTES = 90;
const AGO_SECONDS_UNTIL_HOURS = 5400;
const NOT_STARTED = ["draft", "ready", "failed"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const NAME_MIN = 18;
const NAME_MAX = 40;
const VERDICT_MIN = 20;
const FIRST_MIN = 20;
const TASK_STATE = 7;
const COUNTS_MAX = 18;
const DISCIPLINE_MAX = 18;
const CRITERIA_MAX = 9;
const TASK_SEAT_MAX = 22;
const SEAT_NAME_MIN = 4;
const SEAT_NAME_MAX = 24;
const SEAT_STATE = 7;
const SEAT_LAST = 12;
const SEAT_REQUESTS = 8;
const MODEL_MIN = 5;
const ROLE_MAX = 20;
const NUMBER_CELLS = 5;
const REPLACE_WORD = 7;
const CLOCK_CHARS = 16;
const OWN_NOTE_CHARS = 22;
const BADGE_CHARS = 7;
export const SMALL_CELLS = 10;

export type Align = "start" | "end";
export type Column<K extends string> = { key: K; width: number; align: Align };
export type TaskKey =
  | "mark"
  | "id"
  | "name"
  | "state"
  | "discipline"
  | "criteria"
  | "seat"
  | "counts"
  | "verdict";
export type SeatKey =
  | "seat"
  | "state"
  | "context"
  | "last"
  | "requests"
  | "model"
  | "role"
  | "first";
export type TaskColumns = {
  layout: "line" | "stacked";
  columns: Column<TaskKey>[];
  dropped: boolean;
};
export type SeatColumns = {
  layout: "line" | "stacked";
  columns: Column<SeatKey>[];
  barCells: number;
  dropped: boolean;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function str(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function strOrNull(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function num(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function numOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function list(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

function toVerdict(v: Record<string, unknown>): FleetVerdict {
  return { at: str(v.at), verdict: str(v.verdict), title: str(v.title) };
}

function toTask(t: Record<string, unknown>): FleetTask {
  return {
    id: str(t.id),
    name: str(t.name),
    discipline: str(t.discipline),
    criteria: strings(t.criteria),
    seat: strOrNull(t.seat),
    state: str(t.state),
    passes: num(t.passes),
    fails: num(t.fails),
    last: isRecord(t.last) ? toVerdict(t.last) : null,
    ready: t.ready === true,
  };
}

function toSeat(s: Record<string, unknown>): FleetSeat {
  const cache = isRecord(s.first_request_cache) ? s.first_request_cache : null;
  return {
    seat: str(s.seat),
    role: str(s.role),
    model: str(s.model),
    state: str(s.state),
    session: strOrNull(s.session),
    goal: strOrNull(s.goal),
    contextTokens: numOrNull(s.context_tokens),
    requests: num(s.requests),
    lastRequestAt: strOrNull(s.last_request_at),
    closed: s.closed === true,
    cache:
      cache === null
        ? null
        : { read: num(cache.read), written: num(cache.written), cold: cache.cold === true },
  };
}

function isOpenObligation(o: Record<string, unknown>): boolean {
  const accepted = o.outcome === "accepted" && (o.issue ?? null) === null;
  return o.required === true && !accepted;
}

export function toPanel(json: unknown, isOwn: boolean): FleetPanel | null {
  if (
    !isRecord(json) ||
    typeof json.slug !== "string" ||
    typeof json.state !== "string" ||
    !Array.isArray(json.tasks) ||
    !Array.isArray(json.seats)
  )
    return null;
  return {
    fleet: str(json.fleet),
    slug: json.slug,
    state: json.state,
    goal: str(json.goal),
    startedAt: strOrNull(json.started_at),
    finishedAt: strOrNull(json.finished_at),
    now: str(json.now),
    missing: list(json.missing).map((m) => ({ item: str(m.item), problem: str(m.problem) })),
    accepted: json.accepted === true,
    openObligations: list(json.obligations)
      .filter(isOpenObligation)
      .map((o) => ({
        id: str(o.obligation_id),
        title: str(o.title),
        outcome: str(o.outcome),
        issue: strOrNull(o.issue),
      })),
    criteria: list(json.criteria).map((c) => ({
      id: str(c.id),
      tasks: num(c.tasks),
      passed: num(c.passed),
      ready: num(c.ready),
    })),
    tasks: list(json.tasks).map(toTask),
    seats: list(json.seats).map(toSeat),
    verdicts: list(json.verdicts).map(toVerdict),
    blocked: list(json.blocked).map((b) => ({ seat: str(b.seat), title: str(b.title) })),
    needsOperator: list(json.needs_operator).map((n) => ({
      at: str(n.at),
      seat: str(n.seat),
      title: str(n.title),
    })),
    everySeatIdleSeconds: numOrNull(json.every_seat_idle_seconds),
    isOwn,
  };
}

export function secondsBetween(from: string, to: string): number {
  const a = Date.parse(from);
  const b = Date.parse(to);
  if (Number.isNaN(a) || Number.isNaN(b)) return 0;
  return Math.max(0, Math.floor((b - a) / 1000));
}

export function hoursAndMinutes(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h${String(minutes).padStart(2, "0")}m`;
}

export function elapsed(since: string, now: string): string {
  return hoursAndMinutes(secondsBetween(since, now));
}

export function ago(at: string | null, now: string): string {
  if (at === null) return "never";
  const seconds = secondsBetween(at, now);
  if (seconds < AGO_SECONDS_UNTIL_MINUTES) return `${seconds}s ago`;
  if (seconds < AGO_SECONDS_UNTIL_HOURS) return `${Math.floor(seconds / 60)}m ago`;
  return `${hoursAndMinutes(seconds)} ago`;
}

function two(n: number): string {
  return String(n).padStart(2, "0");
}

export function dayAndTime(iso: string): string {
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return iso;
  return `${two(t.getUTCDate())} ${MONTHS[t.getUTCMonth()]} ${two(t.getUTCHours())}:${two(t.getUTCMinutes())} UTC`;
}

export function hourAndMinute(iso: string): string {
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return iso;
  return `${two(t.getUTCHours())}:${two(t.getUTCMinutes())}`;
}

export type Segment = { text: string; colour?: InkKey; bold?: boolean };

export type Summary = {
  running: Segment[];
  tasks: Segment[];
  acceptance: Segment[];
  verdicts: Segment[];
  seats: Segment[];
};

function label(text: string): Segment {
  return { text, colour: "dim" };
}

function figure(value: number | string): Segment {
  return { text: String(value), bold: true };
}

function count(n: number, colour: InkKey, word: string): Segment {
  return n > 0 ? { text: `${n} ${word}`, colour, bold: true } : { text: `${n} ${word}`, colour: "dim" };
}

function runningSegments(p: FleetPanel): Segment[] {
  if (p.startedAt === null) return [label("not started")];
  if (p.finishedAt !== null) return [label("ran for "), figure(elapsed(p.startedAt, p.finishedAt))];
  return [label("running for "), figure(elapsed(p.startedAt, p.now))];
}

export function summarise(p: FleetPanel): Summary {
  const accepted = p.tasks.filter((t) => t.state === "passed").length;
  const ready = p.tasks.filter((t) => t.ready).length;
  const open = p.seats.filter((s) => !s.closed).length;
  return {
    running: runningSegments(p),
    tasks: [
      figure(accepted),
      label(" of "),
      figure(p.tasks.length),
      label(" tasks accepted; "),
      figure(ready),
      label(" ready"),
    ],
    acceptance: [
      label("fleet acceptance "),
      p.accepted ? { text: "accepted", colour: "green" } : { text: "pending", colour: "yellow" },
    ],
    verdicts: [
      count(p.tasks.reduce((n, t) => n + t.passes, 0), "green", "pass"),
      { text: " " },
      count(p.tasks.reduce((n, t) => n + t.fails, 0), "red", "fail"),
    ],
    seats: [figure(open), label(" seats open, "), figure(p.seats.length), label(" launched")],
  };
}

export function taskCounts(t: FleetTask): Segment[] {
  if (t.passes === 0 && t.fails === 0) return [{ text: "none yet", colour: "dim" }];
  const passed: Segment = { text: `${t.passes} passed`, colour: "dim" };
  if (t.fails === 0) return [passed];
  return [passed, { text: ", ", colour: "dim" }, { text: `${t.fails} failed`, colour: "red" }];
}

export function taskCountsText(t: FleetTask): string {
  return taskCounts(t)
    .map((s) => s.text)
    .join("");
}

export function missingMeans(state: string): string {
  return NOT_STARTED.includes(state)
    ? "the fleet cannot start until each of these is fixed"
    : "the definition was changed after the fleet started and now has these problems; seats already open keep the task they were given";
}

export function criterionColour(c: FleetCriterion): InkKey {
  if (c.tasks === 0 || c.passed === 0) return "faint";
  return c.passed >= c.tasks ? "green" : "yellow";
}

export function taskMark(state: string): { mark: string; colour: InkKey } {
  if (state === "passed") return { mark: "✔", colour: "green" };
  if (state === "rework") return { mark: "✘", colour: "red" };
  if (state === "ready") return { mark: "◇", colour: "blue" };
  if (state === "working") return { mark: "●", colour: "blue" };
  if (state === "queued") return { mark: "○", colour: "dim" };
  return { mark: "•", colour: "ink" };
}

export function seatStateColour(state: string): InkKey {
  if (state === "working") return "green";
  if (state === "idle") return "yellow";
  if (state === "blocked" || state === "silent" || state === "quota") return "red";
  return "dim";
}

export function seatColour(p: FleetPanel, name: string): string | undefined {
  const found = p.seats.find((s) => s.seat === name);
  return found === undefined ? undefined : roleColour(found.role);
}

export type BarRun = { text: string; colour: InkKey; bold: boolean };
export type Bar = { runs: BarRun[]; number: string; colour: InkKey; replace: boolean };

function cellFor(tokens: number, cells: number): number {
  return Math.floor((tokens * cells + WINDOW / 2) / WINDOW);
}

export function contextBar(tokens: number | null, cells: number = CELLS): Bar | null {
  if (tokens === null) return null;
  const t = Math.max(0, tokens);
  const colour: InkKey = t < WARN_AT ? "green" : t < REPLACE_AT ? "yellow" : "red";
  const filled = Math.min(cellFor(t, cells), cells);
  const runs: BarRun[] = [];
  for (let i = 0; i < cells; i++) {
    const cell: BarRun =
      i < filled
        ? { text: "█", colour, bold: false }
        : { text: "░", colour: "faint", bold: false };
    const last = runs[runs.length - 1];
    if (last !== undefined && last.colour === cell.colour && last.bold === cell.bold)
      last.text += cell.text;
    else runs.push(cell);
  }
  return {
    runs,
    number: `${Math.floor((t + 500) / 1000)}k`.padStart(NUMBER_CELLS, " "),
    colour,
    replace: t >= REPLACE_AT,
  };
}

export function seatRows(
  seats: FleetSeat[],
  isExpanded: boolean,
): { shown: FleetSeat[]; closed: number } {
  const closed = seats.filter((s) => s.closed).length;
  return { shown: isExpanded ? allSeats(seats) : seats.filter((s) => !s.closed), closed };
}

export function closedSeatsLabel(closed: number, isShown: boolean): string {
  if (isShown) return "Hide closed seats";
  return `Show ${closed} closed ${closed === 1 ? "seat" : "seats"}`;
}

export function cacheStart(seat: FleetSeat): { text: string; cold: boolean } | null {
  if (seat.cache === null) return null;
  const read = Math.floor((seat.cache.read + 500) / 1000);
  const written = Math.floor((seat.cache.written + 500) / 1000);
  const marker = seat.cache.cold ? " COLD START" : "";
  return { text: `read ${read}k wrote ${written}k${marker}`, cold: seat.cache.cold };
}

export function allSeats(seats: FleetSeat[]): FleetSeat[] {
  return [...seats.filter((s) => !s.closed), ...seats.filter((s) => s.closed)];
}

function sum(widths: number[]): number {
  return widths.reduce((n, w) => n + w, 0);
}

function longest(values: string[], min: number, max: number): number {
  return Math.min(max, Math.max(min, ...values.map((v) => v.length)));
}

const TASK_ORDER: TaskKey[] = [
  "mark",
  "id",
  "name",
  "state",
  "discipline",
  "criteria",
  "seat",
  "counts",
  "verdict",
];
const TASK_DROPS: TaskKey[][] = [
  [],
  ["seat"],
  ["seat", "criteria"],
  ["seat", "criteria", "counts"],
  ["seat", "criteria", "counts", "discipline"],
];

export function taskColumns(tasks: FleetTask[], room: number): TaskColumns {
  const fixed: Partial<Record<TaskKey, number>> = {
    mark: 1,
    id: longest(tasks.map((t) => t.id), 1, Infinity),
    state: TASK_STATE,
    discipline: longest(tasks.map((t) => t.discipline), 1, DISCIPLINE_MAX),
    criteria: longest(tasks.map((t) => t.criteria.join(",")), 1, CRITERIA_MAX),
    seat: longest(["—", ...tasks.flatMap((t) => (t.seat === null ? [] : [t.seat]))], 1, TASK_SEAT_MAX),
    counts: longest(tasks.map(taskCountsText), 1, COUNTS_MAX),
  };
  const names = longest(tasks.map((t) => t.name), 0, Infinity);
  for (const drop of TASK_DROPS) {
    const keys = TASK_ORDER.filter((k) => !drop.includes(k));
    const widths = keys.flatMap((k) => (fixed[k] === undefined ? [] : [fixed[k]]));
    const x = room - sum(widths) - (keys.length - 1);
    if (x < NAME_MIN + VERDICT_MIN) continue;
    const name = Math.max(NAME_MIN, Math.min(names, NAME_MAX, x - VERDICT_MIN));
    const columns = keys.map(
      (key): Column<TaskKey> => ({
        key,
        width: key === "name" ? name : key === "verdict" ? x - name : (fixed[key] ?? 0),
        align: "start",
      }),
    );
    return { layout: "line", columns, dropped: drop.length > 0 };
  }
  return {
    layout: "stacked",
    columns: [
      { key: "mark", width: 1, align: "start" },
      { key: "id", width: fixed.id ?? 1, align: "start" },
    ],
    dropped: true,
  };
}

const SEAT_ORDER: SeatKey[] = [
  "seat",
  "state",
  "context",
  "last",
  "requests",
  "model",
  "role",
  "first",
];
const SEAT_DROPS: SeatKey[][] = [
  [],
  ["first"],
  ["first", "role"],
  ["first", "role", "model"],
  ["first", "role", "model", "requests"],
  ["first", "role", "model", "requests", "last"],
];

function barWidth(cells: number): number {
  return cells + 1 + NUMBER_CELLS + 1 + REPLACE_WORD;
}

export function seatColumns(seats: FleetSeat[], room: number): SeatColumns {
  const fixed = (cells: number): Partial<Record<SeatKey, number>> => ({
    seat: longest(seats.map((s) => s.seat), SEAT_NAME_MIN, SEAT_NAME_MAX),
    state: SEAT_STATE,
    context: barWidth(cells),
    last: SEAT_LAST,
    requests: SEAT_REQUESTS,
    model: longest(seats.map((s) => s.model), MODEL_MIN, Infinity),
    role: longest(seats.map((s) => s.role), SEAT_NAME_MIN, ROLE_MAX),
  });
  const attempts = [
    ...SEAT_DROPS.map((drop) => ({ drop, cells: CELLS })),
    { drop: SEAT_DROPS.at(-1) ?? [], cells: SMALL_CELLS },
  ];
  for (const { drop, cells } of attempts) {
    const widths = fixed(cells);
    const keys = SEAT_ORDER.filter((k) => !drop.includes(k));
    const used = sum(keys.flatMap((k) => (widths[k] === undefined ? [] : [widths[k]]))) + keys.length - 1;
    const rest = room - used;
    if (rest < (keys.includes("first") ? FIRST_MIN : 0)) continue;
    const columns = keys.map(
      (key): Column<SeatKey> => ({
        key,
        width: key === "first" ? rest : (widths[key] ?? 0),
        align: key === "requests" ? "end" : "start",
      }),
    );
    return { layout: "line", columns, barCells: cells, dropped: drop.length > 0 || cells < CELLS };
  }
  return { layout: "stacked", columns: [], barCells: SMALL_CELLS, dropped: true };
}

export type ButtonLabel = { key: "status" | "whole" | "refresh"; hotkey: string; label: string };

export function titleButtons(isShown: boolean, isWhole: boolean): ButtonLabel[] {
  return [
    { key: "status", hotkey: "s", label: isShown ? "Hide details" : "Show details" },
    ...(isShown
      ? [{ key: "whole" as const, hotkey: "w", label: isWhole ? "One line each" : "Show all" }]
      : []),
    { key: "refresh", hotkey: "r", label: "Refresh" },
  ];
}

export type TitleFit = "one" | "two" | "twoNoteBelow" | "alone";

export function titleFit(columns: number, p: FleetPanel, buttons: ButtonLabel[]): TitleFit {
  const identity = BADGE_CHARS + 1 + p.slug.length + 2 + p.state.length + 2;
  const own = p.isOwn ? OWN_NOTE_CHARS : 0;
  const widths = buttons.map((b) => b.hotkey.length + 2 + b.label.length);
  const row = sum(widths) + 2 * Math.max(0, widths.length - 1);
  const clock = Math.max(CLOCK_CHARS, dayAndTime(p.now).length);
  if (identity + own + 2 + row + 2 + clock <= columns) return "one";
  if (identity + own + 2 + clock <= columns) return "two";
  if (identity + 2 + clock <= columns) return "twoNoteBelow";
  return "alone";
}

export type NoSeat = { rework: number; checks: number };

export function noSeatOpen(p: FleetPanel): NoSeat | null {
  if (p.state !== "running" || p.accepted || p.seats.some((s) => !s.closed)) return null;
  return {
    rework: p.tasks.filter((t) => t.state === "rework").length,
    checks: p.openObligations.length,
  };
}

export function noSeatWords(n: NoSeat): string {
  if (n.rework === 0 && n.checks === 0) return "nothing else is waiting";
  const tasks = `${n.rework} ${n.rework === 1 ? "task" : "tasks"} still in rework`;
  const checks = `${n.checks} acceptance ${n.checks === 1 ? "check" : "checks"} open`;
  return `${tasks}, ${checks}`;
}

export function noSeatReason(): string {
  return "every seat has exited; mike keeps the fleet running until the newest commit on its branch has a passing gate";
}

export type StopAnswer = { status: number; ok: boolean; text: string };

function answerMessage(text: string): string | null {
  try {
    const message: unknown = JSON.parse(text)?.message;
    return typeof message === "string" && message !== "" ? message : null;
  } catch {
    return null;
  }
}

export function stopWords(slug: string, answer: StopAnswer | null): string {
  if (answer === null) return `mike did not stop ${slug}: it did not answer`;
  if (answer.ok) return `Fleet ${slug} stopped.`;
  return `mike did not stop ${slug}: ${answerMessage(answer.text) ?? answer.status}`;
}

export type Attention = {
  head: "NEEDS YOU" | "NO SEAT IS OPEN" | "NOTHING IS MOVING";
  words?: string;
  seat?: string;
  title?: string;
  more?: number;
  slug?: string;
  minutes?: number;
};

export function isLiveState(state: string): boolean {
  return state === "running" || state === "starting";
}

export function isLive(p: FleetPanel): boolean {
  return isLiveState(p.state);
}

export function attention(p: FleetPanel): Attention | null {
  if (!isLive(p)) return null;
  const need = p.needsOperator[0];
  if (need !== undefined)
    return {
      head: "NEEDS YOU",
      seat: need.seat,
      title: need.title,
      more: p.needsOperator.length - 1,
    };
  const none = noSeatOpen(p);
  if (none !== null) return { head: "NO SEAT IS OPEN", slug: p.slug, words: noSeatWords(none) };
  if (p.everySeatIdleSeconds !== null)
    return {
      head: "NOTHING IS MOVING",
      slug: p.slug,
      minutes: Math.floor(p.everySeatIdleSeconds / 60),
    };
  return null;
}

function attentionText(a: Attention): string {
  if (a.head === "NEEDS YOU")
    return `${a.seat}: ${a.title}${(a.more ?? 0) > 0 ? ` (+${a.more} more)` : ""}`;
  if (a.head === "NO SEAT IS OPEN") return `fleet ${a.slug}: ${a.words}`;
  return `fleet ${a.slug}: every open seat has been idle for ${a.minutes}m`;
}

export function alarmKey(p: FleetPanel, a: Attention): string {
  return `${p.fleet}|${a.head}|${attentionText(a)}`;
}

export function statusLine(
  p: FleetPanel | null,
  dismissed: string | null = null,
): string | undefined {
  if (p === null) return undefined;
  const a = attention(p);
  if (a === null || alarmKey(p, a) === dismissed) return undefined;
  return `${a.head} · ${attentionText(a)}`;
}
