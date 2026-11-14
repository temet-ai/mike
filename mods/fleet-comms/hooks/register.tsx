import { atom, read, update } from "claude-code";
import type {
  BoxProps,
  ButtonProps,
  ClientProps,
  ElementConstructor,
  EngineInterface,
  MarkdownProps,
  Register,
  RenderElement,
  TextProps,
} from "claude-code";

import type { FleetMessage, FleetPanel, FleetThread, Link } from "../types";
import { INK, fleetStateColour, roleColour } from "./ink";
import { isControlSeat, ownGoal } from "./goal";
import { askStopCheck, probeStopCheck } from "./stop-check";
import { countGoalContinuation, freshGoalBudget, goalBudgetKey, goalBudgetStopReason, readGoalBudget, renewsGoalBudget, reportGoalPause } from "./goal-budget";
import { drawNeed, drawPanel } from "./panel";
import { headWidth } from "./head";
import { isRowOpen, isToggle, openInPlace, registerMailRows } from "./mail";
import { fetchFresh } from "./fresh";

export { isToggle };
import {
  STATUS_MS,
  alarmKey,
  attention,
  isLive,
  isLiveState,
  statusLine,
  stopWords,
  toPanel,
} from "./status";
import type { Attention } from "./status";

const INSPECT = "http://localhost:9989";
const PANE = "fleet-comms";
const POLL_MS = 5000;
const PAGE_CHARS = 30000;
const ROW_CHARS = 80;
const CHUNK_CHARS = 9000;
const BUTTONS_WIDTH = 18;

const thread = atom(
  { plugin: "fleet-comms", key: "thread" } as const,
  "loading",
);
const seenMail = atom(
  { plugin: "fleet-comms", key: "seenMail" } as const,
  null as string | null,
);
const isBandHidden = atom(
  { plugin: "fleet-comms", key: "isBandHidden" } as const,
  false,
);
const goalShown = atom(
  { plugin: "fleet-comms", key: "goalShown" } as const,
  false,
);
const anchor = atom({ plugin: "fleet-comms", key: "anchor" } as const, null);
const link = atom({ plugin: "fleet-comms", key: "link" } as const, {
  failedAt: null,
  goodAt: null,
});
const fleetStatus = atom({ plugin: "fleet-comms", key: "status" } as const, {
  panel: null,
  failedAt: null,
  isLoaded: false,
});
const statusShown = atom(
  { plugin: "fleet-comms", key: "statusShown" } as const,
  true,
);
const goalArmed = atom(
  { plugin: "fleet-comms", key: "goalArmed" } as const,
  null,
);
const goalCleared = atom(
  { plugin: "fleet-comms", key: "goalCleared" } as const,
  false,
);
const statusWhole = atom(
  { plugin: "fleet-comms", key: "statusWhole" } as const,
  false,
);
const openTasks = atom(
  { plugin: "fleet-comms", key: "openTasks" } as const,
  [] as string[],
);
const closedShown = atom(
  { plugin: "fleet-comms", key: "closedShown" } as const,
  false,
);
const stopArmed = atom(
  { plugin: "fleet-comms", key: "stopArmed" } as const,
  null,
);
const dismissed = atom(
  { plugin: "fleet-comms", key: "dismissed" } as const,
  null,
);

const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

type Identity = { session_uuid: string; label: string };
type Summary = {
  thread_uuid: string;
  subject: string;
  message_count: number;
  participant_identities?: Identity[];
  last_message_at?: string;
  fleet?: { slug: string; state: string; fleet_uuid?: string } | null;
};
type Message = {
  message_uuid: string;
  from_session_uuid: string | null;
  to_session_uuid: string | null;
  body: string;
  queued_at: string;
};

export type PageBounds = { start: number; end: number };
export type Kind = "mike" | "everyone" | "seat" | "outside";
export type Tag = "Blocked" | "Help";
export type Who = { ownName: string; seatNames: string[]; isPlain?: boolean };
export type BandFit = "full" | "noCount" | "noTo" | "noLatest";
export type ChunkPiece = {
  opener: string;
  text: string;
  closer: string;
  gap: string;
};
type Tags = {
  Box: ElementConstructor<BoxProps>;
  Text: ElementConstructor<TextProps>;
  Button: ElementConstructor<ButtonProps>;
  Markdown: ElementConstructor<MarkdownProps>;
  Client: ElementConstructor<ClientProps>;
};

let isHeadFaulted = false;

// A fleet's thread raises the band while the fleet is starting or running; one
// that is done, failed or not yet started raises nothing.
export function isMailQuiet(t: FleetThread, seen: string | null): boolean {
  if (isFleetThread(t)) return !isLiveState(t.state);
  const last = t.messages.at(-1);
  return last === undefined || last.id === seen || isSelf(last.from, t.ownName);
}

// /fleet-comms is offered (typeahead, /help, the mods chip) only while there
// is something for it: the seat's fleet is live, or a thread has mail not yet
// seen. Until mike has answered there is neither.
export function isApplicable(
  t: FleetThread | null | "loading",
  seen: string | null,
  panel: FleetPanel | null,
): boolean {
  if (panel !== null && isLive(panel)) return true;
  return typeof t === "object" && t !== null && !isMailQuiet(t, seen);
}

let isOffered = false;

async function offerIfApplicable($: EngineInterface): Promise<void> {
  try {
    const now = isApplicable(
      await read($, thread),
      await read($, seenMail),
      (await read($, fleetStatus)).panel,
    );
    if (now === isOffered) return;
    isOffered = now;
    $.ui.invalidate("command.describe");
  } catch {
    return;
  }
}

async function markSeen($: EngineInterface, t: FleetThread): Promise<void> {
  const last = t.messages.at(-1);
  if (!isFleetThread(t) && last !== undefined) await update($, seenMail, () => last.id);
  await offerIfApplicable($);
}

async function openLatest($: EngineInterface, t: FleetThread): Promise<void> {
  const last = t.messages.at(-1);
  await markSeen($, t);
  if (last === undefined) return openPane($, t);
  await openInPlace($, last.body, () => openPane($, t));
}

export function shortName(name: string): string {
  return name.replace(/-[0-9a-f]{8}$/, "");
}

export function roleOf(name: string): string {
  return shortName(name).replace(/-T\d+$/, "");
}

export function kindOf(name: string, seatNames: string[]): Kind {
  if (name === "mike") return "mike";
  if (name === "everyone") return "everyone";
  return seatNames.includes(name) ? "seat" : "outside";
}

export function nameColour(name: string): string | undefined {
  if (name === "mike") return INK.purple;
  return roleColour(roleOf(name));
}

export function isSelf(name: string, ownName: string): boolean {
  return ownName !== "" && name === ownName;
}

export function tagOf(body: string): Tag | null {
  const head = body.trimStart();
  if (head.startsWith("Blocked:")) return "Blocked";
  if (head.startsWith("Help:")) return "Help";
  return null;
}

export function stateColour(state: string): string {
  return fleetStateColour(state);
}

export function goalOf(subject: string, slug: string): string {
  const prefix = `Fleet ${slug}: `;
  return subject.startsWith(prefix) ? subject.slice(prefix.length) : subject;
}

export function clockTime(iso: string): string {
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return iso;
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(t.getHours())}:${two(t.getMinutes())}`;
}

export function when(iso: string, now: number | Date): string {
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return iso;
  const today = new Date(now);
  const clock = clockTime(iso);
  const sameDay =
    t.getFullYear() === today.getFullYear() &&
    t.getMonth() === today.getMonth() &&
    t.getDate() === today.getDate();
  return sameDay ? clock : `${t.getDate()} ${MONTHS[t.getMonth()]} ${clock}`;
}

// A live fleet's thread first; a finished one only when no fleet is live.
export function pickFleetThread(threads: Summary[]): Summary | null {
  const fleets = threads.filter((t) => (t.fleet ?? null) !== null);
  return fleets.find((t) => isLiveState(t.fleet?.state ?? "")) ?? fleets[0] ?? null;
}

export function pickMailThread(threads: Summary[]): Summary | null {
  const fleet = pickFleetThread(threads);
  if (fleet !== null) return fleet;
  let newest: Summary | null = null;
  for (const t of threads) {
    if (newest === null || (t.last_message_at ?? "") > (newest.last_message_at ?? ""))
      newest = t;
  }
  return newest;
}

export function isFleetThread(t: FleetThread): boolean {
  return t.slug !== "";
}

export function headOf(t: FleetThread): string {
  return isFleetThread(t) ? `fleet ${t.slug}` : "mail";
}

export function paneTitle(t: FleetThread): string {
  return isFleetThread(t) ? `Fleet ${t.slug}` : "Mail";
}

export function nameOf(uuid: string | null, identities: Identity[]): string {
  if (uuid === null) return "mike";
  return (
    identities.find((i) => i.session_uuid === uuid)?.label ??
    uuid.split("-")[0] ??
    uuid
  );
}

export function toFleetThread(
  summary: Summary,
  messages: Message[],
  ownUuid: string = "",
): FleetThread {
  const identities = summary.participant_identities ?? [];
  const fleet = summary.fleet ?? { slug: "", state: "" };
  const ownName = ownUuid === "" ? "" : nameOf(ownUuid, identities);
  const seatNames = identities.map((i) => i.label);
  if (ownName !== "" && !seatNames.includes(ownName)) seatNames.push(ownName);
  return {
    threadUuid: summary.thread_uuid,
    slug: fleet.slug,
    state: fleet.state,
    subject: summary.subject,
    messageCount: summary.message_count,
    messages: messages.map((m): FleetMessage => ({
      id: m.message_uuid,
      at: m.queued_at,
      from: nameOf(m.from_session_uuid, identities),
      to:
        m.to_session_uuid === null
          ? "everyone"
          : nameOf(m.to_session_uuid, identities),
      body: m.body,
    })),
    ownName,
    seatNames,
  };
}

function whoOf(t: FleetThread): Who {
  return {
    ownName: t.ownName,
    seatNames: t.seatNames,
    isPlain: !isFleetThread(t),
  };
}

type WhoParts = {
  text: string;
  colour: string | undefined;
  isBold: boolean;
  mark: string;
};

export function whoParts(who: Who, name: string, isSender: boolean): WhoParts {
  const kind = kindOf(name, who.seatNames);
  if (kind === "everyone")
    return { text: "everyone", colour: undefined, isBold: false, mark: "" };
  if (kind === "mike")
    return { text: "mike", colour: INK.purple, isBold: isSender, mark: "" };
  if (who.isPlain === true)
    return {
      text: shortName(name),
      colour: nameColour(name),
      isBold: isSender || isSelf(name, who.ownName),
      mark: "",
    };
  if (isSelf(name, who.ownName))
    return {
      text: shortName(name),
      colour: nameColour(name),
      isBold: true,
      mark: " (this seat)",
    };
  if (kind === "outside")
    return {
      text: name,
      colour: undefined,
      isBold: isSender,
      mark: " (outside the fleet)",
    };
  return {
    text: shortName(name),
    colour: nameColour(name),
    isBold: isSender,
    mark: "",
  };
}

function whoWidth(who: Who, name: string): number {
  const p = whoParts(who, name, false);
  return p.text.length + p.mark.length;
}

function drawWho(tags: Tags, who: Who, name: string, isSender: boolean) {
  const { Text } = tags;
  const p = whoParts(who, name, isSender);
  const nameText = (
    <Text bold={p.isBold} color={p.colour}>
      {p.text}
    </Text>
  );
  if (p.mark === "") return nameText;
  return (
    <Text>
      {nameText}
      <Text dimColor>{p.mark}</Text>
    </Text>
  );
}

export function bandWidth(
  t: FleetThread,
  now: number | Date,
  fit: BandFit,
): number {
  const head = headWidth(headOf(t));
  const last = t.messages.at(-1);
  if (last === undefined) return head + " · no messages yet".length;
  const who = whoOf(t);
  const count = ` · ${plural(t.messages.length)}`.length;
  const tag = tagOf(last.body);
  const latest =
    ` · latest ${when(last.at, now)} `.length + whoWidth(who, last.from);
  const to = " → ".length + whoWidth(who, last.to);
  const mark = tag === null ? 0 : `  ${tag}`.length;
  if (fit === "full") return head + count + latest + to + mark;
  if (fit === "noCount") return head + latest + to + mark;
  if (fit === "noTo") return head + latest + mark;
  return head;
}

export function bandFit(
  t: FleetThread,
  now: number | Date,
  columns: number,
): BandFit {
  const order: BandFit[] = ["full", "noCount", "noTo", "noLatest"];
  for (const fit of order) {
    if (bandWidth(t, now, fit) + BUTTONS_WIDTH <= columns) return fit;
  }
  return "noLatest";
}

function plural(n: number): string {
  return `${n} ${n === 1 ? "message" : "messages"}`;
}

export function pageBefore(
  messages: FleetMessage[],
  end: number,
  budget: number = PAGE_CHARS,
): PageBounds {
  let start = end;
  let chars = 0;
  while (start > 0) {
    const size = messages[start - 1]!.body.length + ROW_CHARS;
    if (start < end && chars + size > budget) break;
    chars += size;
    start -= 1;
  }
  return { start, end };
}

export function pageAfter(
  messages: FleetMessage[],
  start: number,
  budget: number = PAGE_CHARS,
): PageBounds {
  let end = start;
  let chars = 0;
  while (end < messages.length) {
    const size = messages[end]!.body.length + ROW_CHARS;
    if (end > start && chars + size > budget) break;
    chars += size;
    end += 1;
  }
  return { start, end };
}

export function pagesOf(
  messages: FleetMessage[],
  budget: number = PAGE_CHARS,
): PageBounds[] {
  const pages: PageBounds[] = [];
  let end = messages.length;
  while (end > 0) {
    const page = pageBefore(messages, end, budget);
    pages.push(page);
    end = page.start;
  }
  return pages;
}

export function pageFor(
  messages: FleetMessage[],
  at: string | null,
  budget: number = PAGE_CHARS,
): PageBounds {
  let end = messages.length;
  if (at !== null) {
    const i = messages.findIndex((m) => m.id === at);
    if (i >= 0) end = i + 1;
  }
  return pageBefore(messages, end, budget);
}

export function olderAnchor(
  messages: FleetMessage[],
  page: PageBounds,
): string | null {
  return page.start > 0 ? messages[page.start - 1]!.id : null;
}

export function newerAnchor(
  messages: FleetMessage[],
  page: PageBounds,
  budget: number = PAGE_CHARS,
): string | null {
  const next = pageAfter(messages, page.end, budget);
  return next.end >= messages.length ? null : messages[next.end - 1]!.id;
}

const FENCE = /^ {0,3}```/;
const CONTROL = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g;

export function normalise(body: string): string {
  return body.replace(/\r\n?/g, "\n").replace(CONTROL, "�");
}

export function hardBreaks(text: string): string {
  const lines = text.split("\n");
  let isFenced = false;
  return lines
    .map((line, i) => {
      const isFence = FENCE.test(line);
      const inFence = isFenced || isFence;
      if (isFence) isFenced = !isFenced;
      const next = lines[i + 1];
      if (inFence || next === undefined) return line;
      if (next.trim() === "" || line.trim() === "") return line;
      return line + "  ";
    })
    .join("\n");
}

function cutAt(text: string): { at: number; gap: string } {
  const blank = text.lastIndexOf("\n\n", CHUNK_CHARS);
  if (blank > 0) return { at: blank, gap: "\n" };
  const newline = text.lastIndexOf("\n", CHUNK_CHARS);
  if (newline > 0) return { at: newline, gap: "\n" };
  const space = text.lastIndexOf(" ", CHUNK_CHARS);
  if (space > 0) return { at: space, gap: " " };
  const high = text.charCodeAt(CHUNK_CHARS - 1);
  const splitsPair = high >= 0xd800 && high <= 0xdbff;
  return { at: splitsPair ? CHUNK_CHARS - 1 : CHUNK_CHARS, gap: "" };
}

function openFence(text: string): string | null {
  let open: string | null = null;
  for (const line of text.split("\n")) {
    if (!FENCE.test(line)) continue;
    open = open === null ? line : null;
  }
  return open;
}

function carriedFence(line: string): string {
  return line.length <= 200 ? line : (line.match(/^ {0,3}`+/)?.[0] ?? "```");
}

export function chunkPieces(body: string): ChunkPiece[] {
  let rest = hardBreaks(normalise(body));
  const pieces: ChunkPiece[] = [];
  let opener = "";
  for (;;) {
    const isLast = rest.length <= CHUNK_CHARS;
    const cut = isLast ? { at: rest.length, gap: "" } : cutAt(rest);
    const text = rest.slice(0, cut.at);
    const open = openFence(opener === "" ? text : `${opener}\n${text}`);
    const carries = !isLast && open !== null;
    pieces.push({
      opener,
      text,
      closer: carries ? "\n```" : "",
      gap: cut.gap,
    });
    if (isLast) return pieces;
    rest = rest.slice(cut.at + cut.gap.length);
    opener = carries ? carriedFence(open) : "";
  }
}

// Markdown takes at most 10,000 characters and only tab and newline as control
// characters, so a body is drawn as several Markdown elements, each at most 9,000
// characters (plus a carried fence line). Order: normalise (CRLF and CR to LF,
// other control characters to U+FFFD), then hard breaks (step 1b), then cut at the
// last blank line, newline or space before 9,000, then keep fences intact.
// Line breaks (spec 4.7): step 1b is applied unconditionally. Outside fenced
// blocks every line followed by a non-blank line gets two trailing spaces, so
// `one\ntwo` and a `- a\n- b` list never run together. Joined back, the pieces'
// `text` and `gap` equal the normalised body once those added spaces are stripped.
export function chunks(body: string): string[] {
  return chunkPieces(body)
    .map((p) =>
      p.opener === "" ? p.text + p.closer : `${p.opener}\n${p.text}${p.closer}`,
    )
    .filter((c) => c !== "");
}

let session = "";
// The mod works only in a fleet seat. mike sets MIKE_RUN_ID in every session
// it starts from a fleet definition; a session without it was not started as
// a seat, never becomes one, and gets no timer, request or UI.
let seatRun: string | null = null;
const goalNames = new Map<string, string>();
let inFlight: Promise<void> | null = null;

function refresh($: EngineInterface): Promise<void> {
  if (session === "" || seatRun === null) return Promise.resolve();
  if (inFlight === null) {
    inFlight = fetchThread($)
      .then(() => offerIfApplicable($))
      .finally(() => {
        inFlight = null;
      });
  }
  return inFlight;
}

async function nowIso($: EngineInterface): Promise<string> {
  return new Date(await $.clock.now()).toISOString();
}

async function noteFailure($: EngineInterface): Promise<void> {
  const failedAt = await nowIso($);
  await update($, link, (l: Link) => ({ ...l, failedAt }));
}

export type ScrollAction = "open" | "status" | "older" | "newer" | "latest" | "arrival";

export function scrollFor(action: ScrollAction): "start" | "end" {
  return action === "newer" || action === "status" ? "start" : "end";
}

export function commandScroll(hasStatus: boolean): ScrollAction {
  return hasStatus ? "status" : "open";
}

export function shouldFollow(
  grew: boolean,
  at: string | null,
  paneIsFocused: boolean | undefined,
): boolean {
  return grew && at === null && paneIsFocused === false;
}

async function scrollPane(
  $: EngineInterface,
  action: ScrollAction,
): Promise<void> {
  try {
    await $.ui.scroll({ in: PANE, to: scrollFor(action) });
  } catch {
    return;
  }
}

async function followNewest($: EngineInterface, grew: boolean): Promise<void> {
  const at = await read($, anchor);
  if (!grew || at !== null) return;
  try {
    const open = (await $.ui.panes()).find((p) => p.id === PANE);
    if (shouldFollow(grew, at, open?.isFocused)) await scrollPane($, "arrival");
  } catch {
    return;
  }
}

type Seats = { seat: string }[];

function polled($: EngineInterface, url: string) {
  return fetchFresh((at, init) => $.http.fetch(at, init), url);
}

async function isFleetSeat(
  $: EngineInterface,
  summary: Summary,
  fleetUuid: string,
): Promise<boolean> {
  const own = nameOf(session, summary.participant_identities ?? []);
  const res = await polled($, `${INSPECT}/api/fleets/${fleetUuid}/status`);
  if (!res.ok) return true;
  const seats: Seats | undefined = JSON.parse(res.text).seats;
  if (!Array.isArray(seats)) return true;
  return seats.some((s) => s.seat === shortName(own));
}

async function loadFleetThread(
  $: EngineInterface,
  summary: Summary,
): Promise<FleetThread | "failed"> {
  const detail = await polled(
    $,
    `${INSPECT}/api/threads/${summary.thread_uuid}`,
  );
  if (!detail.ok) return "failed";
  const body = JSON.parse(detail.text);
  return toFleetThread(
    { ...summary, ...body.thread },
    body.messages ?? [],
    session,
  );
}

type MailRow = {
  message_uuid: string;
  from_session_uuid: string | null;
  to_session_uuid: string | null;
  body: string;
  queued_at: string;
};

export function mailboxThread(
  rows: MailRow[],
  identities: Identity[],
  ownUuid: string,
): FleetThread | null {
  if (rows.length === 0) return null;
  const sorted = [...rows].sort((a, b) =>
    a.queued_at < b.queued_at ? -1 : a.queued_at > b.queued_at ? 1 : 0,
  );
  const summary: Summary = {
    thread_uuid: "mailbox",
    subject: "Mailbox",
    message_count: sorted.length,
    participant_identities: identities,
  };
  return toFleetThread(summary, sorted, ownUuid);
}

async function goalName(
  $: EngineInterface,
  id: string,
): Promise<string | null> {
  const cached = goalNames.get(id);
  if (cached !== undefined) return cached;
  const res = await $.http.fetch(`${INSPECT}/api/sessions/${id}/goal`);
  if (!res.ok) return null;
  const name = JSON.parse(res.text).name;
  if (typeof name !== "string" || name === "") return null;
  goalNames.set(id, name);
  return name;
}

async function loadMailbox(
  $: EngineInterface,
  identities: Identity[],
): Promise<FleetThread | null | "failed"> {
  const res = await polled($, `${INSPECT}/api/sessions/${session}/messages`);
  if (!res.ok) return "failed";
  const box = JSON.parse(res.text);
  const rows: MailRow[] = [...(box.inbox ?? []), ...(box.outbox ?? [])];
  const known = [...identities];
  try {
    const root = await $.session.root();
    const path = `${INSPECT}/api/workspace/projects/${encodeURIComponent(root)}/roster`;
    const addNames = async (url: string) => {
      const res = await polled($, url);
      if (!res.ok) return;
      for (const r of JSON.parse(res.text).roster ?? []) {
        if (typeof r.name === "string" && !known.some((k) => k.session_uuid === r.session_id))
          known.push({ session_uuid: r.session_id, label: r.name });
      }
    };
    await addNames(path);
  } catch {
    // names fall back to session prefixes
  }
  try {
    for (const r of rows) {
      const id = r.from_session_uuid;
      if (id === null || id === session) continue;
      if (known.some((k) => k.session_uuid === id)) continue;
      const label = await goalName($, id);
      if (label !== null) known.push({ session_uuid: id, label });
    }
  } catch {
    // names fall back to session prefixes
  }
  return mailboxThread(rows, known, session);
}

async function fetchThread($: EngineInterface): Promise<void> {
  try {
    const list = await polled(
      $,
      `${INSPECT}/api/threads?participant=${session}`,
    );
    if (!list.ok) return await noteFailure($);
    const threads: Summary[] = JSON.parse(list.text).threads ?? [];
    const fleetSummary = pickFleetThread(threads);
    const isSeat =
      fleetSummary !== null &&
      (await isFleetSeat($, fleetSummary, fleetSummary.fleet?.fleet_uuid ?? fleetSummary.thread_uuid));
    const loaded =
      fleetSummary !== null && isSeat
        ? await loadFleetThread($, fleetSummary)
        : await loadMailbox(
            $,
            threads.flatMap((t) => t.participant_identities ?? []),
          );
    if (loaded === "failed") return await noteFailure($);
    if (loaded === null) {
      await update($, thread, () => null);
      return await noteGood($);
    }
    const next = loaded;
    const before = await read($, thread);
    const previous =
      typeof before === "object" && before !== null ? before : null;
    if (previous === null || previous.threadUuid !== next.threadUuid) {
      await update($, anchor, () => null);
    }
    await update($, thread, (current) =>
      typeof current === "object" &&
      current !== null &&
      current.threadUuid === next.threadUuid &&
      current.state === next.state &&
      current.messages.length === next.messages.length &&
      current.messages.at(-1)?.id === next.messages.at(-1)?.id
        ? current
        : next,
    );
    await noteGood($);
    const grew =
      previous !== null &&
      previous.threadUuid === next.threadUuid &&
      next.messages.length > previous.messages.length;
    await followNewest($, grew);
  } catch {
    return await noteFailure($);
  }
}

async function noteGood($: EngineInterface): Promise<void> {
  const goodAt = await nowIso($);
  await update($, link, () => ({ failedAt: null, goodAt }));
}

let statusInFlight: Promise<void> | null = null;
let needsSeen = 0;
let needsSeeded = false;
let lastStatusLine: string | undefined;
let statusTold = false;

function refreshStatus($: EngineInterface): Promise<void> {
  if (session === "" || seatRun === null) return Promise.resolve();
  if (statusInFlight === null) {
    statusInFlight = fetchStatus($)
      .then(() => offerIfApplicable($))
      .finally(() => {
        statusInFlight = null;
      });
  }
  return statusInFlight;
}

async function noteStatusFailure($: EngineInterface): Promise<void> {
  const failedAt = await nowIso($);
  await update($, fleetStatus, (s) => ({ ...s, failedAt, isLoaded: true }));
}

async function ownStatus($: EngineInterface): Promise<unknown> {
  const res = await polled(
    $,
    `${INSPECT}/api/fleets/by-session/${session}/status`,
  );
  return res.ok ? JSON.parse(res.text) : null;
}

function tellStatus(
  $: EngineInterface,
  panel: FleetPanel | null,
  dismissedKey: string | null,
): void {
  const line = statusLine(panel, dismissedKey);
  if (!statusTold || line !== lastStatusLine) {
    statusTold = true;
    lastStatusLine = line;
    try {
      $.ui.status(line);
    } catch {
      return;
    }
  }
  const needs = panel !== null && isLive(panel) ? panel.needsOperator : [];
  const first = needs[0];
  // What already needed the operator when the session (or this mod) started
  // is on the band and the status line; only what arrives later is toasted.
  if (!needsSeeded) {
    needsSeeded = true;
    needsSeen = needs.length;
    return;
  }
  if (needs.length > needsSeen && first !== undefined) {
    try {
      $.ui.toast(`NEEDS YOU: ${first.seat}: ${first.title}`);
    } catch {
      needsSeen = needs.length;
      return;
    }
  }
  needsSeen = needs.length;
}

// A seat reads the status of its own fleet and no other: the fleet it was
// started in is the one it belongs to for the life of the session.
async function fetchStatus($: EngineInterface): Promise<void> {
  let panel: FleetPanel | null = null;
  try {
    const json = await ownStatus($);
    if (json === null) return await noteStatusFailure($);
    panel = toPanel(json, true);
    if (panel === null) return await noteStatusFailure($);
    const next = panel;
    await update($, fleetStatus, () => ({ panel: next, failedAt: null, isLoaded: true }));
  } catch {
    return await noteStatusFailure($);
  }
  tellStatus($, panel, await read($, dismissed));
}

async function toggleStatus($: EngineInterface): Promise<void> {
  await update($, statusShown, (shown: boolean) => !shown);
}

async function toggleWhole($: EngineInterface): Promise<void> {
  await update($, statusWhole, (whole: boolean) => !whole);
}

async function toggleClosed($: EngineInterface): Promise<void> {
  await update($, closedShown, (shown: boolean) => !shown);
}

async function toggleTask($: EngineInterface, id: string): Promise<void> {
  await update($, openTasks, (open: string[]) =>
    open.includes(id) ? open.filter((x) => x !== id) : [...open, id],
  );
}

async function openStatusPane(
  $: EngineInterface,
  panel: FleetPanel,
): Promise<void> {
  await $.ui.open({
    id: PANE,
    title: `Fleet ${panel.slug}`,
    focus: true,
    closeOnEscape: true,
  });
}

async function openPane(
  $: EngineInterface,
  t: FleetThread,
  action: ScrollAction = "open",
): Promise<void> {
  await update($, anchor, () => null);
  await $.ui.open({
    id: PANE,
    title: paneTitle(t),
    focus: true,
    closeOnEscape: true,
  });
  await scrollPane($, action);
}

async function showOlder($: EngineInterface): Promise<void> {
  const current = await read($, thread);
  if (typeof current !== "object" || current === null) return;
  const at = await read($, anchor);
  const target = olderAnchor(current.messages, pageFor(current.messages, at));
  if (target === null) return;
  await update($, anchor, () => target);
  await scrollPane($, "older");
}

async function showNewer($: EngineInterface): Promise<void> {
  const current = await read($, thread);
  if (typeof current !== "object" || current === null) return;
  const at = await read($, anchor);
  const page = pageFor(current.messages, at);
  if (page.end >= current.messages.length) return;
  await update($, anchor, () => newerAnchor(current.messages, page));
  await scrollPane($, "newer");
}

async function showLatest($: EngineInterface): Promise<void> {
  await update($, anchor, () => null);
  await scrollPane($, "latest");
}

async function hideBand($: EngineInterface): Promise<void> {
  await update($, isBandHidden, () => true);
}

async function armStop($: EngineInterface, panel: FleetPanel): Promise<void> {
  await update($, stopArmed, () => panel.fleet);
}

async function keepFleet($: EngineInterface): Promise<void> {
  await update($, stopArmed, () => null);
}

async function postStop(
  $: EngineInterface,
  panel: FleetPanel,
): Promise<{ status: number; ok: boolean; text: string } | null> {
  try {
    return await $.http.fetch(`${INSPECT}/api/fleets/${panel.fleet}/stop`, {
      method: "POST",
    });
  } catch {
    return null;
  }
}

async function stopFleet($: EngineInterface, panel: FleetPanel): Promise<void> {
  const answer = await postStop($, panel);
  await keepFleet($);
  $.ui.toast(stopWords(panel.slug, answer));
  await refreshStatus($);
}

async function dismissAlarm(
  $: EngineInterface,
  panel: FleetPanel,
  need: Attention,
): Promise<void> {
  const key = alarmKey(panel, need);
  await update($, dismissed, () => key);
  tellStatus($, panel, key);
}

async function toggleGoal($: EngineInterface): Promise<void> {
  await update($, goalShown, (shown: boolean) => !shown);
}

function drawTag(tags: Tags, tag: Tag) {
  const { Text } = tags;
  return (
    <Text bold color={tag === "Blocked" ? INK.red : INK.yellow}>
      {`  ${tag}`}
    </Text>
  );
}

type BandView = {
  isOpen: boolean;
  toggle: () => void;
};

function drawHead(tags: Tags, t: FleetThread, view: BandView) {
  const { Text, Button, Client } = tags;
  const label = headOf(t);
  if (!isHeadFaulted)
    return (
      <Client key="head" module="./head.tsx" props={{ label, isOpen: view.isOpen }} />
    );
  if (t.messages.length > 0)
    return (
      <Button key="head" label={label} variant="plain" onPress={view.toggle} />
    );
  return <Text bold>{label}</Text>;
}

function drawBand(
  tags: Tags,
  t: FleetThread,
  now: number,
  fit: BandFit,
  open: () => void,
  hide: () => void,
  view: BandView,
) {
  const { Box, Text, Button } = tags;
  const who = whoOf(t);
  const last = t.messages.at(-1);
  const tag = last === undefined ? null : tagOf(last.body);
  const row = (
    <Box columnGap={2}>
      <Box>
      {drawHead(tags, t, view)}
      <Text>
        {last === undefined && <Text dimColor> · no messages yet</Text>}
        {last !== undefined && fit === "full" && (
          <Text dimColor>{` · ${plural(t.messages.length)}`}</Text>
        )}
        {last !== undefined && fit !== "noLatest" && (
          <Text dimColor>{` · latest ${when(last.at, now)} `}</Text>
        )}
        {last !== undefined &&
          fit !== "noLatest" &&
          drawWho(tags, who, last.from, true)}
        {last !== undefined && (fit === "full" || fit === "noCount") && (
          <Text dimColor>{" → "}</Text>
        )}
        {last !== undefined &&
          (fit === "full" || fit === "noCount") &&
          drawWho(tags, who, last.to, false)}
        {last !== undefined &&
          fit !== "noLatest" &&
          tag !== null &&
          drawTag(tags, tag)}
      </Text>
      </Box>
      <Button key="open" label="Open" hotkey="o" variant="secondary" onPress={open} />
      <Button
        key="hide"
        label="Hide"
        hotkey="h"
        variant="secondary"
        dimColor
        onPress={hide}
      />
    </Box>
  );
  return row;
}

function drawHeader(
  tags: Tags,
  t: FleetThread,
  isGoalShown: boolean,
  isFocused: boolean,
  toggle: () => void,
) {
  const { Box, Text, Button } = tags;
  const goal = isFleetThread(t) ? goalOf(t.subject, t.slug) : "";
  return (
    <Box flexDirection="column">
      <Box columnGap={2}>
        {isFleetThread(t) ? (
          <Text>
            <Text bold backgroundColor={INK.purple} color={INK.badgeText}>
              {" FLEET "}
            </Text>{" "}
            <Text bold>{t.slug}</Text>
            {"  "}
            <Text color={stateColour(t.state)}>{t.state}</Text>
          </Text>
        ) : (
          <Text>
            <Text bold backgroundColor={INK.purple} color={INK.badgeText}>
              {" MAIL "}
            </Text>{" "}
            <Text bold>{t.subject}</Text>
          </Text>
        )}
        {goal !== "" && (
          <Button
            key="goal"
            label={isGoalShown ? "Hide goal" : "Show goal"}
            hotkey="g"
            variant="secondary"
            dimColor={!isFocused}
            onPress={toggle}
          />
        )}
      </Box>
      {isGoalShown && (
        <Box paddingLeft={1}>
          <Text dimColor>{goal}</Text>
        </Box>
      )}
    </Box>
  );
}

type PageActions = {
  older: () => void;
  newer: () => void;
  latest: () => void;
};

function drawPageRow(
  tags: Tags,
  at: "top" | "bottom",
  page: PageBounds,
  total: number,
  isFocused: boolean,
  actions: PageActions,
) {
  const { Box, Text, Button } = tags;
  const hasOlder = page.start > 0;
  const hasNewer = page.end < total;
  return (
    <Box columnGap={2} marginTop={1}>
      {at === "top" ? (
        <Text>
          <Text bold color={INK.purple}>
            MESSAGES
          </Text>
          {"  "}
          <Text color={INK.dim}>
            {total === 0 ? "no messages yet" : `${page.start + 1}–${page.end} of ${total}`}
          </Text>
        </Text>
      ) : (
        <Text dimColor>
          {total === 0
            ? "no messages yet"
            : `messages ${page.start + 1}–${page.end} of ${total}`}
        </Text>
      )}
      {hasOlder && (
        <Button
          key={`older-${at}`}
          label="Older"
          hotkey="o"
          variant="secondary"
          dimColor={!isFocused}
          onPress={actions.older}
        />
      )}
      {hasNewer && (
        <Button
          key={`newer-${at}`}
          label="Newer"
          hotkey="n"
          variant="secondary"
          dimColor={!isFocused}
          onPress={actions.newer}
        />
      )}
      {hasNewer && (
        <Button
          key={`latest-${at}`}
          label="Latest"
          hotkey="l"
          variant="secondary"
          dimColor={!isFocused}
          onPress={actions.latest}
        />
      )}
    </Box>
  );
}

function drawMessage(tags: Tags, who: Who, m: FleetMessage, now: number) {
  const { Box, Text, Markdown } = tags;
  const tag = tagOf(m.body);
  return (
    <Box key={m.id} flexDirection="column" marginTop={1}>
      <Text>
        <Text dimColor>{when(m.at, now)}</Text>
        {"  "}
        {drawWho(tags, who, m.from, true)}
        <Text dimColor>{" → "}</Text>
        {drawWho(tags, who, m.to, false)}
        {tag !== null && drawTag(tags, tag)}
      </Text>
      <Box flexDirection="column" paddingLeft={2}>
        {chunks(m.body).map((text, i) => (
          <Markdown
            key={`${m.id}-${i}`}
            text={text}
            dimColor={m.from === "mike"}
          />
        ))}
      </Box>
    </Box>
  );
}

async function armGoal($: EngineInterface): Promise<void> {
  if (!(await read($, fleetStatus)).isLoaded) await refreshStatus($);
  const panel = (await read($, fleetStatus)).panel;
  if (isControlSeat(panel, session)) {
    if ((await read($, goalCleared))) return;
    await $.command.run({ command: "goal", args: "clear" });
    await update($, goalCleared, () => true);
    return;
  }
  const goal = ownGoal(panel, session);
  if (goal === null) return;
  if (panel !== null && (await probeStopCheck($, INSPECT, panel.fleet, session))) {
    if ((await read($, goalArmed)) !== null && !(await read($, goalCleared))) {
      await $.command.run({ command: "goal", args: "clear" });
      await update($, goalCleared, () => true);
      await update($, goalArmed, () => null);
    }
    return;
  }
  const budget = await readGoalBudget($, session, goal);
  if (budget.exhausted) {
    if (!(await read($, goalCleared))) {
      await $.command.run({ command: "goal", args: "clear" });
      await update($, goalCleared, () => true);
    }
    return;
  }
  if ((await read($, goalArmed)) === goal) return;
  await $.command.run({ command: "goal", args: goal });
  await update($, goalArmed, () => goal);
  await update($, goalCleared, () => false);
  try {
    $.ui.toast("Goal armed from the fleet definition; /goal shows it");
  } catch {
    return;
  }
}

export const register: Register = (on) => {
  on("classic.Stop", async ($, e, next) => {
    if (seatRun === null || e.agent_id || e.session_id !== session) return next(e);
    if (!(await read($, fleetStatus)).isLoaded) await refreshStatus($);
    const panel = (await read($, fleetStatus)).panel;
    if (panel !== null && panel.isOwn && !isControlSeat(panel, session)) {
      const said = (e as unknown as { last_assistant_message?: string }).last_assistant_message;
      const verdict = await askStopCheck($, INSPECT, panel.fleet, session, said, e.stop_hook_active === true);
      if (verdict !== null) {
        return verdict.decision === "push_back" ? { block: verdict.reason } : next(e);
      }
    }
    const goal = ownGoal(panel, session);
    if (goal === null || isControlSeat(panel, session)) return next(e);
    try {
      const budget = await countGoalContinuation($, session, goal, e.stop_hook_active);
      if (!budget.exhausted) return next(e);
      await reportGoalPause($, session, budget);
      return { preventContinuation: true, stopReason: goalBudgetStopReason(budget) };
    } catch {
      return { preventContinuation: true, stopReason: "Fleet goal paused because its continuation budget could not be saved. The task remains unfinished." };
    }
  });

  on("prompt.submit", async ($, e, next) => {
    const result = await next(e);
    if (seatRun === null || "drop" in result || !renewsGoalBudget(e)) return result;
    if (!(await read($, fleetStatus)).isLoaded) await refreshStatus($);
    const panel = (await read($, fleetStatus)).panel;
    const goal = ownGoal(panel, session);
    if (goal === null || isControlSeat(panel, session)) return result;
    await $.store.set(goalBudgetKey(session), freshGoalBudget(goal));
    await update($, goalArmed, () => null);
    await update($, goalCleared, () => false);
    return result;
  });

  on("turn.complete", async ($, e, next) => {
    if (seatRun === null || e.agentId) return next(e);
    try {
      await armGoal($);
    } catch (err) {
      try {
        $.ui.log(`fleet-comms: the goal was not armed: ${String(err)}`);
      } catch {
        return next(e);
      }
    }
    return next(e);
  });

  on("session.start", async ($, e, next) => {
    session = await $.session.id();
    const run = await $.env.get("MIKE_RUN_ID").catch(() => undefined);
    seatRun = typeof run === "string" && run.trim() !== "" ? run : null;
    if (seatRun === null) return next(e);
    await $.command.register({
      name: "fleet-comms",
      description:
        "Show this seat's fleet in a pane: its status as mike's fleet panel draws it, and its messages",
    });
    void refresh($);
    void refreshStatus($);
    $.clock.every(POLL_MS, () => refresh($));
    $.clock.every(STATUS_MS, () => refreshStatus($));
    return next(e);
  });

  // Hidden from the typeahead, /help and the mods chip while nothing applies;
  // typed in full it still runs.
  on("command.describe", { command: "fleet-comms" }, async (_$, e, next) =>
    next({ ...e, isHidden: e.isHidden || !isOffered }),
  );

  on("command.run", { command: "fleet-comms" }, async ($) => {
    await Promise.all([refresh($), refreshStatus($)]);
    const current = await read($, thread);
    const panel = (await read($, fleetStatus)).panel;
    if (current === "loading") {
      const failed = (await read($, link)).failedAt !== null;
      if (failed && panel === null)
        return { text: `mike did not answer on ${INSPECT}.` };
      await $.ui.open({
        id: PANE,
        title: panel === null ? "Fleet" : `Fleet ${panel.slug}`,
        focus: true,
        closeOnEscape: true,
      });
      return { text: "Loading fleet messages…" };
    }
    await update($, isBandHidden, () => false);
    if (current === null) {
      if (panel === null)
        return { text: "mike has no fleet status or mail for this seat yet." };
      await openStatusPane($, panel);
      return { text: `Fleet ${panel.slug} opened.` };
    }
    // The pane shows the mail, so opening it reads it, as the band's Open does.
    await markSeen($, current);
    await openPane($, current, commandScroll(panel !== null));
    return { text: `${paneTitle(current)} opened.` };
  });

  on("ui.render", { component: "AbovePrompt" }, async ($, e, next) => {
    if (seatRun === null) return next(e);
    const current = await read($, thread);
    const panel = (await read($, fleetStatus)).panel;
    const alarm = panel === null ? null : attention(panel);
    const need =
      panel !== null &&
      alarm !== null &&
      alarmKey(panel, alarm) === (await read($, dismissed))
        ? null
        : alarm;
    // Hide puts the messages away; what needs the operator still shows, and
    // is put away one alarm at a time with Dismiss.
    const isHidden = await read($, isBandHidden);
    const live =
      !isHidden &&
      typeof current === "object" &&
      current !== null &&
      !isMailQuiet(current, await read($, seenMail))
        ? current
        : null;
    const hasThread = live !== null;
    if (e.props.hasSurvey || (!hasThread && need === null)) return next(e);
    const tags = $.ui.resolve(e);
    const { Box } = tags;
    const now = await $.clock.now();
    const theirs = await next(e);
    const over = (top: RenderElement, bottom: RenderElement | null) => (
      <Box flexDirection="column">
        {top}
        {bottom}
        {theirs}
      </Box>
    );
    const band = live !== null
      ? drawBand(
          tags,
          live,
          now,
          bandFit(live, now, e.props.bodyColumns),
          () => void markSeen($, live).then(() => openPane($, live)),
          () => void (isFleetThread(live) ? hideBand($) : markSeen($, live)),
          {
            isOpen: await isRowOpen($, live.messages.at(-1)?.body ?? ""),
            toggle: () => void openLatest($, live),
          },
        )
      : null;
    if (need === null || panel === null) return band === null ? theirs : over(band, null);
    const needRow = drawNeed(
      tags,
      need,
      panel,
      {
        ...(hasThread
          ? {}
          : {
              open: () => openStatusPane($, panel),
              ...(isHidden ? {} : { hide: () => hideBand($) }),
            }),
        dismiss: () => void dismissAlarm($, panel, need),
        stop: () => void armStop($, panel),
        yes: () => void stopFleet($, panel),
        no: () => void keepFleet($),
      },
      (await read($, stopArmed)) === panel.fleet,
    );
    return band === null ? over(needRow, null) : over(band, needRow);
  });

  on("ui.message", { component: "AbovePrompt", element: "head" }, async ($, e, next) => {
    const current = await read($, thread);
    if (isToggle(e.data) && typeof current === "object" && current !== null)
      await openLatest($, current);
    return next(e);
  });

  on("ui.fault", { component: "AbovePrompt", element: "head" }, async ($, e, next) => {
    isHeadFaulted = true;
    $.ui.log(
      `fleet-comms: the band's head module failed (${e.phase}: ${e.reason}); drawing it plain instead`,
    );
    return next(e);
  });

  registerMailRows(on);

  on("ui.render", { component: "Pane", requestId: PANE }, async ($, e) => {
    const tags = $.ui.resolve(e);
    const { Box, Text } = tags;
    const current = await read($, thread);
    const status = await read($, link);
    const fleet = await read($, fleetStatus);
    const isFocused = e.props.isFocused;
    const fleetPanel = fleet.panel;
    const panel =
      fleetPanel === null
        ? null
        : drawPanel(
            tags,
            fleetPanel,
            {
              columns: e.props.bodyColumns,
              surface: e.surface,
              isFocused,
              isShown: await read($, statusShown),
              isWhole: await read($, statusWhole),
              closedShown: await read($, closedShown),
              openTasks: await read($, openTasks),
              failedAt: fleet.failedAt,
              stopArmed: await read($, stopArmed),
              dismissed: await read($, dismissed),
            },
            {
              refresh: () => void refreshStatus($),
              toggle: () => void toggleStatus($),
              whole: () => void toggleWhole($),
              closed: () => void toggleClosed($),
              task: (id: string) => void toggleTask($, id),
              stop: () => void armStop($, fleetPanel),
              yes: () => void stopFleet($, fleetPanel),
              no: () => void keepFleet($),
            },
          );
    const above = (line: RenderElement) =>
      panel === null ? (
        line
      ) : (
        <Box flexDirection="column" rowGap={1}>
          {panel}
          {line}
        </Box>
      );
    if (current === "loading")
      return above(
        status.failedAt !== null ? (
          <Text color={INK.red}>{`mike did not answer on ${INSPECT}.`}</Text>
        ) : (
          <Text dimColor>Loading fleet messages…</Text>
        ),
      );
    if (current === null)
      return above(
        <Text dimColor>
          {panel === null
            ? "mike has no fleet status or mail for this seat yet."
            : "No fleet messages yet."}
        </Text>,
      );
    const now = await $.clock.now();
    const total = current.messages.length;
    const page = pageFor(current.messages, await read($, anchor));
    const who = whoOf(current);
    const actions: PageActions = {
      older: () => showOlder($),
      newer: () => showNewer($),
      latest: () => showLatest($),
    };
    const isSameFleet =
      fleet.panel !== null &&
      isFleetThread(current) &&
      fleet.panel.slug === current.slug;
    return (
      <Box flexDirection="column">
        {panel}
        {!isSameFleet &&
          drawHeader(tags, current, await read($, goalShown), isFocused, () =>
            toggleGoal($),
          )}
        {status.failedAt !== null && status.goodAt !== null && (
          <Text color={INK.red}>
            {`mike did not answer at ${when(status.failedAt, now)}. Showing what it sent at ${when(status.goodAt, now)}.`}
          </Text>
        )}
        {drawPageRow(tags, "top", page, total, isFocused, actions)}
        {current.messages
          .slice(page.start, page.end)
          .map((m) => drawMessage(tags, who, m, now))}
        {total > 0 &&
          drawPageRow(tags, "bottom", page, total, isFocused, actions)}
      </Box>
    );
  });
};
