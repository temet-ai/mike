import { atom, read, update } from "claude-code";
import type { EngineInterface, PromptOrigin, Register } from "claude-code";

type On = Parameters<Register>[0];

export const openRows = atom(
  { plugin: "fleet-comms", key: "openRows" } as const,
  [] as string[],
);

const seen = new Map<string, string>();
let isRowFaulted = false;

export function flat(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

export function isMailOrigin(origin: PromptOrigin): boolean {
  return (
    origin.kind === "peer" ||
    (origin.kind === "channel" && (origin as { server?: string }).server === "mike")
  );
}

export function rememberRow(requestId: string, text: string): void {
  seen.delete(requestId);
  seen.set(requestId, flat(text));
}

export function forgetRows(): void {
  seen.clear();
  isRowFaulted = false;
}

export function rowFor(body: string, rows: ReadonlyMap<string, string> = seen): string | null {
  const needle = flat(body.split("\n").find((l) => l.trim() !== "") ?? "");
  if (needle === "") return null;
  let found: string | null = null;
  for (const [id, text] of rows) if (text.includes(needle)) found = id;
  return found;
}

export type Letter = { body: string; from: string | null; at: string | null };

const ENVELOPE =
  /^Cross-session message from (?:session ([0-9a-f]{8})[0-9a-f-]*|(an anonymous sender))[^\n]*?, sent (\d{2}):(\d{2})(?::\d{2})? UTC\.?[ \t]*$/m;

export function localClock(hours: number, minutes: number, now: Date = new Date()): string {
  const t = new Date(now);
  t.setUTCHours(hours, minutes, 0, 0);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(t.getHours())}:${two(t.getMinutes())}`;
}

const MAIL_EXPLAINED = /^Mail from another session arrives as [^\n]*$/m;
const MAIL_LINE = /^Mail from (?:[^\n]*?\(?session ([0-9a-f]{8})[0-9a-f-]*\)?|(an unknown sender)|[^\n(]*\(([0-9a-f]{8})[0-9a-f-]*\)): /m;

function mailLetterOf(text: string, now: Date): Letter | null {
  const unwrapped = text.replace(MAIL_EXPLAINED, "").replace(/^<\/?system-reminder>[ \t]*$/gm, "").trim();
  const hit = MAIL_LINE.exec(unwrapped);
  if (hit === null || hit.index !== 0) return null;
  const from = hit[1] ?? hit[3] ?? (hit[2] !== undefined ? "anonymous" : null);
  return {
    body: unwrapped.slice(hit[0].length).trim(),
    from,
    at: localClock(now.getUTCHours(), now.getUTCMinutes(), now),
  };
}

export function letterOf(text: string, now: Date = new Date()): Letter {
  const mail = mailLetterOf(text, now);
  if (mail !== null) return mail;
  const hit = ENVELOPE.exec(text);
  if (hit === null) return { body: text.trim(), from: null, at: null };
  const body = text
    .replace(ENVELOPE, "")
    .replace(/^<\/?system-reminder>[ \t]*$/gm, "")
    .replace(/^Reply: `send_session_message`[^\n]*$/m, "")
    .replace(/^\s*\n/, "")
    .trim();
  return {
    body,
    from: hit[1] ?? (hit[2] !== undefined ? "anonymous" : null),
    at: localClock(Number(hit[3]), Number(hit[4]), now),
  };
}

const openedAt = new Map<string, number>();

export function toggled(ids: string[], id: string): string[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
}

async function toggleRow($: EngineInterface, id: string): Promise<void> {
  openedAt.set(id, await $.clock.now());
  await update($, openRows, (ids: string[]) => toggled(ids, id));
}

export async function isRowOpen($: EngineInterface, body: string): Promise<boolean> {
  const id = rowFor(body);
  return id !== null && (await read($, openRows)).includes(id);
}

export async function openInPlace(
  $: EngineInterface,
  body: string,
  fallback: () => Promise<void>,
): Promise<void> {
  const id = rowFor(body);
  if (id === null) return fallback();
  if ((await read($, openRows)).includes(id)) return toggleRow($, id);
  await toggleRow($, id);
  try {
    const moved = await $.ui.scroll({ to: { requestId: id }, block: "center" });
    if (moved.deny !== undefined)
      $.ui.log(`fleet-comms: the mail row did not scroll into view: ${moved.deny}`);
  } catch (err) {
    $.ui.log(`fleet-comms: the mail row did not scroll into view: ${String(err)}`);
  }
}

export function isToggle(data: unknown): boolean {
  return (
    typeof data === "object" &&
    data !== null &&
    (data as { toggle?: unknown }).toggle === true
  );
}

export function registerMailRows(on: On): void {
  on("ui.render", { component: "UserMessage" }, async ($, e, next) => {
    if (isRowFaulted || e.props.isExpanded || !isMailOrigin(e.props.origin))
      return next(e);
    rememberRow(e.requestId, e.props.text);
    const { Client } = $.ui.resolve(e);
    const isOpen = (await read($, openRows)).includes(e.requestId);
    const letter = letterOf(e.props.text, new Date(await $.clock.now()));
    return (
      <Client
        key="mail-row"
        module="./mailrow.tsx"
        props={{
          from: letter.from ?? e.props.from?.name ?? "mike",
          at: letter.at,
          body: letter.body,
          isOpen,
          openedAt: openedAt.get(e.requestId) ?? 0,
          columns: e.viewport?.columns ?? 100,
        }}
      />
    );
  });

  on("ui.message", { component: "UserMessage", element: "mail-row" }, async ($, e, next) => {
    if (isToggle(e.data)) await toggleRow($, e.requestId);
    return next(e);
  });

  on("ui.fault", { component: "UserMessage", element: "mail-row" }, async ($, e, next) => {
    isRowFaulted = true;
    $.ui.log(
      `fleet-comms: the mail row module failed (${e.phase}: ${e.reason}); drawing mail rows the usual way`,
    );
    $.ui.invalidate("ui.render");
    return next(e);
  });
}
