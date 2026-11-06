import type { ClientModule } from "claude-code";

export type MailRowProps = {
  from: string;
  at: string | null;
  body: string;
  isOpen: boolean;
  openedAt: number;
  columns: number;
};

export const FRESH_MS = 1000;

export function startFrame(isOpen: boolean, openedAt: number, now: number): number {
  const age = now - openedAt;
  if (age >= 0 && age < FRESH_MS) return 0;
  return isOpen ? OPEN_FRAMES : CLOSE_FRAMES;
}
export type MailRowState = {
  isOpen: boolean;
  frame: number;
  hover: boolean;
};

export const FRAME_MS = 33;
export const OPEN_FRAMES = 34;
export const CLOSE_FRAMES = 12;
export const SLOPE = 3;
export const TAIL = 22;
export const SCRAMBLE = 7;
const GLYPHS = "░▒▓<>/\\|+*=~:·";
const CHROME = 4;

type Rgb = [number, number, number];
const WHITE: Rgb = [255, 255, 255];
const PURPLE: Rgb = [187, 154, 247];
const BLUE: Rgb = [122, 162, 247];
const INK: Rgb = [192, 202, 245];
const FAINT: Rgb = [86, 95, 137];
const FRAME: Rgb = [125, 110, 180];
const FRAME_HOVER: Rgb = [122, 162, 247];
const STOPS: [number, Rgb][] = [
  [0, WHITE],
  [4, PURPLE],
  [11, BLUE],
  [TAIL, INK],
];

export const CHIP = {
  rest: { bg: "rgb(41,46,66)", fg: "rgb(187,154,247)" },
  open: { bg: "rgb(187,154,247)", fg: "rgb(26,27,38)" },
  hover: { bg: "rgb(122,162,247)", fg: "rgb(26,27,38)" },
} as const;

export function rgb(c: Rgb): string {
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  const k = Math.max(0, Math.min(1, t));
  return [
    Math.round(a[0] + (b[0] - a[0]) * k),
    Math.round(a[1] + (b[1] - a[1]) * k),
    Math.round(a[2] + (b[2] - a[2]) * k),
  ];
}

export function easeOut(t: number): number {
  const k = Math.max(0, Math.min(1, t));
  return 1 - (1 - k) ** 3;
}

export function glow(delta: number): Rgb {
  for (let i = 1; i < STOPS.length; i++) {
    const [at, colour] = STOPS[i]!;
    const [was, from] = STOPS[i - 1]!;
    if (delta <= at) return mix(from, colour, (delta - was) / (at - was));
  }
  return INK;
}

export function wrapLines(body: string, width: number): string[] {
  const w = Math.max(8, width);
  const out: string[] = [];
  for (const raw of body.replace(/\r\n?/g, "\n").split("\n")) {
    let line = "";
    for (const word of raw.split(/(\s+)/)) {
      if (word === "") continue;
      if ((line + word).length <= w) {
        line += word;
        continue;
      }
      if (line.trim() !== "") out.push(line.trimEnd());
      const pieces = /^\s+$/.test(word)
        ? [""]
        : (word.match(new RegExp(`.{1,${w}}`, "g")) ?? [word]);
      line = pieces.pop() ?? "";
      out.push(...pieces);
    }
    out.push(line.trimEnd());
  }
  while (out.length > 1 && out.at(-1) === "") out.pop();
  return out;
}

export function easeInOut(t: number): number {
  const k = Math.max(0, Math.min(1, t));
  return k < 0.5 ? 4 * k ** 3 : 1 - (-2 * k + 2) ** 3 / 2;
}

export function frontAt(frame: number, width: number, rows: number): number {
  return easeInOut(frame / OPEN_FRAMES) * (width + rows * SLOPE + TAIL);
}

export function scramble(r: number, c: number, frame: number): string {
  const h = ((r * 73856093) ^ (c * 19349663) ^ (frame * 83492791)) >>> 0;
  return GLYPHS[h % GLYPHS.length] ?? "·";
}

export type Cell = { text: string; colour: string; bold?: boolean };

function merge(cells: Cell[]): Cell[] {
  const out: Cell[] = [];
  for (const cell of cells) {
    const last = out.at(-1);
    if (last !== undefined && last.colour === cell.colour && last.bold === cell.bold)
      last.text += cell.text;
    else out.push({ ...cell });
  }
  return out;
}

export function revealCell(
  ch: string,
  d: number,
  front: number,
  r: number,
  c: number,
  frame: number,
): Cell {
  const delta = front - d;
  if (delta < -SCRAMBLE) return { text: " ", colour: rgb(FAINT) };
  if (delta < 0)
    return {
      text: ch === " " ? " " : scramble(r, c, frame),
      colour: rgb(mix(FAINT, PURPLE, 1 + delta / SCRAMBLE)),
    };
  return { text: ch, colour: rgb(glow(delta)), bold: delta < 2 };
}

export function revealLine(
  line: string,
  r: number,
  width: number,
  front: number,
  frame: number,
): Cell[] {
  const cells: Cell[] = [];
  const padded = line.padEnd(width, " ");
  for (let c = 0; c < padded.length; c++)
    cells.push(revealCell(padded[c]!, c + r * SLOPE, front, r, c, frame));
  return merge(cells);
}

export function shownRows(state: MailRowState, rows: number): number {
  if (state.isOpen) {
    if (state.frame >= OPEN_FRAMES) return rows;
    const front = easeOut(state.frame / (OPEN_FRAMES * 0.45)) * rows;
    return Math.min(rows, Math.max(1, Math.ceil(front)));
  }
  if (state.frame >= CLOSE_FRAMES) return 0;
  return Math.ceil(rows * (1 - easeOut(state.frame / CLOSE_FRAMES)));
}

export function isMoving(state: MailRowState): boolean {
  return state.frame < (state.isOpen ? OPEN_FRAMES : CLOSE_FRAMES);
}

function edgeColour(state: MailRowState, front: number, d: number): string {
  if (state.hover && !isMoving(state)) return rgb(FRAME_HOVER);
  if (!state.isOpen) return rgb(mix(FRAME, FAINT, state.frame / CLOSE_FRAMES));
  const delta = front - d;
  if (delta < 0) return rgb(FAINT);
  return rgb(mix(glow(delta), FRAME, delta / (TAIL * 2)));
}

const MailRow: ClientModule<MailRowProps, MailRowState> = (props, s) => {
  const { Box, Text } = s.elements;
  const from = typeof props?.from === "string" ? props.from : "mail";
  const body = typeof props?.body === "string" ? props.body : "";
  const wantOpen = props?.isOpen === true;
  const columns = s.columns > 0 ? s.columns : Math.max(20, (props?.columns ?? 80) - 4);
  const width = Math.max(8, columns - CHROME);
  const lines = wrapLines(body, width);
  const initial: MailRowState = {
    isOpen: wantOpen,
    frame: startFrame(wantOpen, Number(props?.openedAt ?? 0), Date.now()),
    hover: false,
  };
  if (s.state === undefined) {
    s.every(FRAME_MS, () => {
      const st = s.state ?? initial;
      if (isMoving(st)) s.setState({ ...st, frame: st.frame + 1 });
    });
    s.onPointer((e) => {
      const st = s.state ?? initial;
      if (e.type === "leave") return s.setState({ ...st, hover: false });
      if (e.type === "up" && e.button === "left") s.post({ toggle: true });
      if (!st.hover) s.setState({ ...st, hover: true });
    });
  }
  const state = s.state ?? initial;
  if (state.isOpen !== wantOpen) s.setState({ ...state, isOpen: wantOpen, frame: 0 });
  const rows = shownRows(state, lines.length);
  const front = state.isOpen
    ? state.frame >= OPEN_FRAMES
      ? Number.POSITIVE_INFINITY
      : frontAt(state.frame, width, lines.length)
    : Number.POSITIVE_INFINITY;
  const chip = state.hover ? CHIP.hover : state.isOpen || rows > 0 ? CHIP.open : CHIP.rest;
  const fade = state.isOpen ? 0 : state.frame / CLOSE_FRAMES;
  const lineCount = `${lines.length} ${lines.length === 1 ? "line" : "lines"}`;
  const bottomD = rows * SLOPE;
  return (
    <Box flexDirection="column">
      <Text>
        <Text bold backgroundColor={chip.bg} color={chip.fg}>
          {` ✉ mail ${state.isOpen ? "▾" : "▸"} `}
        </Text>
        <Text dimColor>{"  from "}</Text>
        <Text bold>{from}</Text>
        {typeof props?.at === "string" && <Text dimColor>{`  ${props.at}`}</Text>}
        <Text dimColor>
          {state.isOpen ? "  · click to fold" : `  · ${lineCount} · click to read`}
        </Text>
      </Text>
      {rows > 0 && (
        <Text color={edgeColour(state, front, 0)}>{`╭${"─".repeat(width + 2)}╮`}</Text>
      )}
      {lines.map((line, r) => {
        if (r >= rows) return null;
        const cells = state.isOpen
          ? revealLine(line, r, width, front, state.frame)
          : [{ text: line.padEnd(width, " "), colour: rgb(mix(INK, FAINT, fade * 1.4)) }];
        return (
          <Text>
            <Text color={edgeColour(state, front, r * SLOPE)}>{"│ "}</Text>
            {cells.map((cell) => (
              <Text color={cell.colour} bold={cell.bold === true}>
                {cell.text}
              </Text>
            ))}
            <Text color={edgeColour(state, front, width + r * SLOPE)}>{" │"}</Text>
          </Text>
        );
      })}
      {rows > 0 && (
        <Text color={edgeColour(state, front, bottomD)}>{`╰${"─".repeat(width + 2)}╯`}</Text>
      )}
    </Box>
  );
};

export default MailRow;
