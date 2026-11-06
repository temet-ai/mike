import type {
  BoxProps,
  ButtonProps,
  ElementConstructor,
  RenderChildren,
  RenderSurface,
  TextProps,
} from "claude-code";

import type { FleetPanel, FleetSeat, FleetTask, FleetVerdict } from "../types";
import { INK, fleetStateColour, roleColour } from "./ink";
import type { InkKey } from "./ink";
import {
  alarmKey,
  ago,
  attention,
  cacheStart,
  closedSeatsLabel,
  contextBar,
  criterionColour,
  dayAndTime,
  hourAndMinute,
  missingMeans,
  noSeatOpen,
  noSeatReason,
  noSeatWords,
  seatColumns,
  seatColour,
  seatRows,
  seatStateColour,
  summarise,
  taskColumns,
  taskCounts,
  taskMark,
  titleButtons,
  titleFit,
} from "./status";
import type { Attention, Column, Segment, SeatKey, TaskKey } from "./status";

export type PanelTags = {
  Box: ElementConstructor<BoxProps>;
  Text: ElementConstructor<TextProps>;
  Button: ElementConstructor<ButtonProps>;
};

export type PanelView = {
  columns: number;
  surface: RenderSurface;
  isFocused: boolean;
  isShown: boolean;
  isWhole: boolean;
  closedShown: boolean;
  openTasks: string[];
  failedAt: string | null;
  stopArmed: string | null;
  dismissed: string | null;
};

export type StopActions = {
  stop: () => void;
  yes: () => void;
  no: () => void;
};

export type PanelActions = {
  refresh: () => void;
  toggle: () => void;
  whole: () => void;
  closed: () => void;
  task: (id: string) => void;
} & StopActions;

type Draw = {
  tags: PanelTags;
  fit: TextProps["wrap"];
  isWhole: boolean;
  now: string;
  open: string[];
  pressTask: (id: string) => void;
};

export const CUT: TextProps["wrap"] = "truncate-end";
export const WHOLE: TextProps["wrap"] = "wrap";
const FRAME_CELLS = 4;
const ROW_ROOM = 5;
const ROW_HOVER: TextProps["hover"] = { backgroundColor: INK.rowHover };
const PRESS_HOVER: ButtonProps["hover"] = { underline: true, color: INK.purple };
const ID_HOVER: ButtonProps["hover"] = { ...PRESS_HOVER, backgroundColor: INK.rowHover };
const SEAT_HEAD: Record<SeatKey, string> = {
  seat: "seat",
  state: "state",
  context: "context of 1000k",
  last: "last request",
  requests: "requests",
  model: "model",
  role: "role",
  first: "first request",
};

function inkOf(key: InkKey): string | undefined {
  return key === "ink" ? undefined : INK[key];
}

function widthOf<K extends string>(columns: Column<K>[], key: K): number {
  return columns.find((c) => c.key === key)?.width ?? 1;
}

function cell<K extends string>(d: Draw, column: Column<K>, body: RenderChildren) {
  const { Box } = d.tags;
  return (
    <Box
      width={column.width}
      flexShrink={0}
      justifyContent={column.align === "end" ? "flex-end" : "flex-start"}
    >
      {body}
    </Box>
  );
}

function sep(d: Draw) {
  const { Text } = d.tags;
  return <Text color={INK.faint}>{" · "}</Text>;
}

function rowKey(prefix: string, name: string, index: number, all: string[]): string {
  return all.indexOf(name) === index ? `${prefix}-${name}` : `${prefix}-${name}#${index}`;
}

function heading(d: Draw, text: string, colour: InkKey = "purple") {
  const { Text } = d.tags;
  return (
    <Text bold color={INK[colour]}>
      {text}
    </Text>
  );
}

function seam(d: Draw, inner: number) {
  const { Box, Text } = d.tags;
  return (
    <Box height={1} overflow="hidden">
      <Text color={INK.faint}>{"─".repeat(inner)}</Text>
    </Box>
  );
}

function drawMissing(d: Draw, p: FleetPanel) {
  const { Box, Text } = d.tags;
  if (p.missing.length === 0) return null;
  return (
    <Box flexDirection="column">
      <Text wrap={WHOLE}>
        {heading(d, "MISSING", "red")}
        <Text color={INK.dim}>{`   ${missingMeans(p.state)}`}</Text>
      </Text>
      {p.missing.map((m) => (
        <Box key={`missing-${m.item}`} paddingLeft={1}>
          <Text wrap={WHOLE}>
            <Text bold>{m.item}</Text>
            {"  "}
            <Text>{m.problem}</Text>
          </Text>
        </Box>
      ))}
    </Box>
  );
}

function dismissedNote(d: Draw, isShown: boolean) {
  const { Text } = d.tags;
  return isShown ? <Text color={INK.dim}>{" (dismissed from the band)"}</Text> : null;
}

export function drawStop(tags: PanelTags, p: FleetPanel, isArmed: boolean, a: StopActions) {
  const { Box, Button } = tags;
  const press = (key: string, label: string, onPress: () => void) => (
    <Box key={`${key}-slot`} flexShrink={0}>
      <Button key={key} label={label} variant={key === "stop-fleet-yes" ? "primary" : "secondary"} onPress={onPress} />
    </Box>
  );
  if (!isArmed) return [press("stop-fleet", "Stop fleet", a.stop)];
  return [
    press("stop-fleet-yes", `Yes, stop ${p.slug}`, a.yes),
    press("stop-fleet-no", "Keep it", a.no),
  ];
}

function drawNeeds(d: Draw, p: FleetPanel, isDismissed: boolean) {
  const { Box, Text } = d.tags;
  if (p.needsOperator.length === 0) return null;
  const head = heading(d, "NEEDS YOU", "red");
  return (
    <Box flexDirection="column">
      {isDismissed ? (
        <Text>
          {head}
          {dismissedNote(d, true)}
        </Text>
      ) : (
        head
      )}
      {p.needsOperator.map((n) => (
        <Box key={`need-${n.at}-${n.seat}`} paddingLeft={1}>
          <Text wrap={WHOLE}>
            <Text color={INK.dim}>{hourAndMinute(n.at)}</Text>
            {"  "}
            <Text bold color={seatColour(p, n.seat)}>
              {n.seat}
            </Text>
            {"  "}
            <Text>{n.title}</Text>
          </Text>
        </Box>
      ))}
    </Box>
  );
}

function drawIdle(d: Draw, p: FleetPanel, isDismissed: boolean) {
  const { Text } = d.tags;
  if (p.everySeatIdleSeconds === null) return null;
  return (
    <Text wrap={WHOLE}>
      {heading(d, "NOTHING IS MOVING", "orange")}
      {dismissedNote(d, isDismissed)}
      {"  "}
      <Text>{`every open seat has been idle for ${Math.floor(p.everySeatIdleSeconds / 60)}m`}</Text>
    </Text>
  );
}

function drawNoSeat(
  d: Draw,
  p: FleetPanel,
  isDismissed: boolean,
  isArmed: boolean,
  stop: StopActions,
) {
  const { Box, Text } = d.tags;
  const none = noSeatOpen(p);
  if (none === null) return null;
  return (
    <Box columnGap={2}>
      <Text wrap={WHOLE}>
        {heading(d, "NO SEAT IS OPEN", "orange")}
        {dismissedNote(d, isDismissed)}
        {"  "}
        <Text>{noSeatWords(none)}</Text>
        {"  "}
        <Text color={INK.dim}>{noSeatReason()}</Text>
      </Text>
      {drawStop(d.tags, p, isArmed, stop)}
    </Box>
  );
}

function drawAlarm(
  d: Draw,
  p: FleetPanel,
  fill: string | undefined,
  view: PanelView,
  actions: PanelActions,
) {
  const { Box } = d.tags;
  const need = attention(p);
  const isDismissed = need !== null && view.dismissed === alarmKey(p, need);
  const dismissedFor = (head: string) => isDismissed && need?.head === head;
  const blocks = [
    drawMissing(d, p),
    drawNeeds(d, p, dismissedFor("NEEDS YOU")),
    drawNoSeat(d, p, dismissedFor("NO SEAT IS OPEN"), view.stopArmed === p.fleet, actions),
    drawIdle(d, p, dismissedFor("NOTHING IS MOVING")),
  ].filter((b) => b !== null);
  if (blocks.length === 0) return null;
  const urgent = p.missing.length > 0 || p.needsOperator.length > 0;
  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={urgent ? INK.red : INK.orange}
      paddingX={1}
      backgroundColor={fill}
    >
      {blocks}
    </Box>
  );
}

function titleIdentity(d: Draw, p: FleetPanel, withNote: boolean) {
  const { Text } = d.tags;
  return (
    <Text>
      <Text bold backgroundColor={INK.purple} color={INK.ground}>
        {" FLEET "}
      </Text>{" "}
      <Text bold>{p.slug}</Text>
      {"  "}
      <Text bold backgroundColor={fleetStateColour(p.state)} color={INK.ground}>
        {` ${p.state.toUpperCase()} `}
      </Text>
      {withNote && p.isOwn && <Text color={INK.dim}>{"  this session's fleet"}</Text>}
    </Text>
  );
}

function titleButtonRow(d: Draw, view: PanelView, actions: PanelActions) {
  const { Box, Button } = d.tags;
  const press = { status: actions.toggle, whole: actions.whole, refresh: actions.refresh };
  return titleButtons(view.isShown, view.isWhole).map((b) => (
    <Box key={`title-${b.key}`} flexShrink={0}>
      <Button
        key={b.key}
        label={b.label}
        hotkey={b.hotkey}
        variant="secondary"
        dimColor={!view.isFocused}
        onPress={press[b.key]}
      />
    </Box>
  ));
}

function drawTitle(d: Draw, p: FleetPanel, view: PanelView, actions: PanelActions) {
  const { Box, Text } = d.tags;
  const fit = titleFit(view.columns, p, titleButtons(view.isShown, view.isWhole));
  const buttons = titleButtonRow(d, view, actions);
  const clock = (
    <Box flexShrink={0}>
      <Text color={INK.dim}>{dayAndTime(p.now)}</Text>
    </Box>
  );
  if (fit === "alone")
    return (
      <Box flexDirection="column">
        <Box flexWrap="wrap">{titleIdentity(d, p, false)}</Box>
        <Box columnGap={2} flexWrap="wrap">
          {clock}
          {p.isOwn && <Text color={INK.dim}>{"this session's fleet"}</Text>}
          {buttons}
        </Box>
      </Box>
    );
  if (fit === "one")
    return (
      <Box columnGap={2}>
        {titleIdentity(d, p, true)}
        <Box flexGrow={1} />
        {buttons}
        {clock}
      </Box>
    );
  if (fit === "two")
    return (
      <Box flexDirection="column">
        <Box columnGap={2}>
          {titleIdentity(d, p, true)}
          <Box flexGrow={1} />
          {clock}
        </Box>
        <Box columnGap={2} flexWrap="wrap">
          {buttons}
        </Box>
      </Box>
    );
  return (
    <Box flexDirection="column">
      <Box columnGap={2}>
        {titleIdentity(d, p, false)}
        <Box flexGrow={1} />
        {clock}
      </Box>
      <Box columnGap={2} flexWrap="wrap">
        {p.isOwn && <Text color={INK.dim}>{"this session's fleet"}</Text>}
        {buttons}
      </Box>
    </Box>
  );
}

function segments(d: Draw, parts: Segment[], lead = "") {
  const { Text } = d.tags;
  return (
    <Text>
      {lead !== "" && <Text color={INK.faint}>{lead}</Text>}
      {parts.map((s) => (
        <Text color={s.colour === undefined ? undefined : INK[s.colour]} bold={s.bold}>
          {s.text}
        </Text>
      ))}
    </Text>
  );
}

function drawSummary(d: Draw, p: FleetPanel) {
  const { Box } = d.tags;
  const s = summarise(p);
  const groups = [s.running, s.tasks, s.acceptance, s.verdicts, s.seats];
  return <Box flexWrap="wrap">{groups.map((g, i) => segments(d, g, i > 0 ? "  ·  " : ""))}</Box>;
}

function drawCriteria(d: Draw, p: FleetPanel) {
  const { Box, Text } = d.tags;
  if (p.criteria.length === 0) return null;
  return (
    <Box>
      <Box width={10} flexShrink={0}>
        <Text color={INK.dim}>criteria</Text>
      </Box>
      <Box flexGrow={1} flexWrap="wrap" columnGap={1}>
        {p.criteria.map((c) => {
          const colour = criterionColour(c);
          return (
            <Text
              bold
              backgroundColor={INK[colour]}
              color={colour === "faint" ? INK.ink : INK.ground}
            >
              {` ${c.id} ${c.passed}/${c.tasks} accepted `}
            </Text>
          );
        })}
      </Box>
    </Box>
  );
}

function drawIntro(d: Draw, p: FleetPanel) {
  const { Box, Text } = d.tags;
  const goal = p.goal.split(/\s+/).filter(Boolean).join(" ");
  return (
    <Box flexDirection="column">
      {goal !== "" && <Text wrap={d.fit}>{goal}</Text>}
      {drawSummary(d, p)}
      {drawCriteria(d, p)}
    </Box>
  );
}

function drawOpen(d: Draw, p: FleetPanel) {
  const { Box, Text } = d.tags;
  if (p.openObligations.length === 0) return null;
  return (
    <Box flexDirection="column">
      {p.openObligations.map((o) => (
        <Box key={`open-${o.id}`} flexDirection="column">
          <Text wrap={WHOLE}>
            {heading(d, "ACCEPTANCE OPEN", "orange")}
            {"  "}
            <Text>{`${o.id}: ${o.outcome} — ${o.title}`}</Text>
          </Text>
          {o.issue !== null && (
            <Box paddingLeft={1}>
              <Text color={INK.red} wrap={WHOLE}>
                {o.issue}
              </Text>
            </Box>
          )}
        </Box>
      ))}
    </Box>
  );
}

function drawBlocked(d: Draw, p: FleetPanel) {
  const { Box, Text } = d.tags;
  if (p.blocked.length === 0) return null;
  return (
    <Box flexDirection="column">
      {heading(d, "BLOCKED", "red")}
      {p.blocked.map((b) => (
        <Box key={`blocked-${b.seat}`} paddingLeft={1}>
          <Text wrap={WHOLE}>
            <Text bold color={seatColour(p, b.seat)}>
              {b.seat}
            </Text>
            {"  "}
            <Text>{b.title}</Text>
          </Text>
        </Box>
      ))}
    </Box>
  );
}

function verdictParts(d: Draw, v: FleetVerdict | null, fit: TextProps["wrap"] = d.fit) {
  const { Text } = d.tags;
  if (v === null)
    return (
      <Text color={INK.dim} hover={ROW_HOVER}>
        no verdict yet
      </Text>
    );
  const passed = v.verdict === "pass";
  return (
    <Text wrap={fit} hover={ROW_HOVER}>
      <Text bold color={passed ? INK.green : INK.red}>
        {v.verdict.toUpperCase()}
      </Text>{" "}
      <Text color={passed ? INK.dim : undefined}>{v.title}</Text>
      <Text color={INK.dim}>{`  ${ago(v.at, d.now)}`}</Text>
    </Text>
  );
}

function countsOf(d: Draw, t: FleetTask) {
  const { Text } = d.tags;
  return (
    <Text wrap={d.fit} hover={ROW_HOVER}>
      {taskCounts(t).map((s) => (
        <Text color={inkOf(s.colour ?? "ink")}>{s.text}</Text>
      ))}
    </Text>
  );
}

function idButton(d: Draw, t: FleetTask) {
  const { Button } = d.tags;
  return (
    <Button
      key={`task-${t.id}`}
      label={t.id}
      plain
      hover={ID_HOVER}
      onPress={() => d.pressTask(t.id)}
    />
  );
}

function taskCell(d: Draw, t: FleetTask, key: TaskKey) {
  const { Text } = d.tags;
  const m = taskMark(t.state);
  if (key === "mark")
    return (
      <Text bold color={inkOf(m.colour)} hover={ROW_HOVER}>
        {m.mark}
      </Text>
    );
  if (key === "id") return idButton(d, t);
  if (key === "name")
    return (
      <Text wrap={d.fit} hover={ROW_HOVER}>
        {t.name}
      </Text>
    );
  if (key === "state")
    return (
      <Text color={inkOf(m.colour)} wrap={d.fit} hover={ROW_HOVER}>
        {t.state}
      </Text>
    );
  if (key === "discipline")
    return (
      <Text color={roleColour(t.discipline) ?? INK.dim} wrap={d.fit} hover={ROW_HOVER}>
        {t.discipline}
      </Text>
    );
  if (key === "criteria")
    return (
      <Text color={INK.dim} wrap={d.fit} hover={ROW_HOVER}>
        {t.criteria.join(",")}
      </Text>
    );
  if (key === "seat")
    return (
      <Text color={INK.dim} wrap={d.fit} hover={ROW_HOVER}>
        {t.seat ?? "—"}
      </Text>
    );
  if (key === "counts") return countsOf(d, t);
  return verdictParts(d, t.last);
}

function taskDetail(d: Draw, t: FleetTask, indent: number) {
  const { Box, Text } = d.tags;
  if (!d.open.includes(t.id)) return null;
  return (
    <Box key={`task-open-${t.id}`} flexDirection="column" paddingLeft={indent}>
      <Text wrap={WHOLE} hover={ROW_HOVER}>
        {t.name}
      </Text>
      {verdictParts(d, t.last, WHOLE)}
    </Box>
  );
}

function drawLineTask(d: Draw, t: FleetTask, columns: Column<TaskKey>[]) {
  const { Box } = d.tags;
  return (
    <Box key={`task-row-${t.id}`} flexDirection="column" paddingLeft={1}>
      <Box columnGap={1}>{columns.map((c) => cell(d, c, taskCell(d, t, c.key)))}</Box>
      {taskDetail(d, t, widthOf(columns, "id") + 3)}
    </Box>
  );
}

function drawStackedTask(d: Draw, t: FleetTask, columns: Column<TaskKey>[]) {
  const { Box, Text } = d.tags;
  const m = taskMark(t.state);
  const idWidth = widthOf(columns, "id");
  return (
    <Box key={`task-row-${t.id}`} flexDirection="column" paddingLeft={1}>
      <Box columnGap={1}>
        {cell(d, { key: "mark", width: 1, align: "start" }, taskCell(d, t, "mark"))}
        {cell(d, { key: "id", width: idWidth, align: "start" }, taskCell(d, t, "id"))}
        <Box flexGrow={1} flexShrink={1}>
          {taskCell(d, t, "name")}
        </Box>
        <Box flexShrink={0}>
          <Text color={inkOf(m.colour)} hover={ROW_HOVER}>
            {t.state}
          </Text>
        </Box>
      </Box>
      <Box paddingLeft={idWidth + 3}>
        <Text wrap={d.fit} hover={ROW_HOVER}>
          <Text color={INK.dim}>{t.seat ?? "—"}</Text>
          {sep(d)}
          <Text color={roleColour(t.discipline) ?? INK.dim}>{t.discipline}</Text>
          {sep(d)}
          <Text color={INK.dim}>{t.criteria.join(",")}</Text>
          {sep(d)}
          {countsOf(d, t)}
          {sep(d)}
          {verdictParts(d, t.last)}
        </Text>
      </Box>
      {taskDetail(d, t, idWidth + 3)}
    </Box>
  );
}

function drawTasks(d: Draw, p: FleetPanel, view: PanelView) {
  const { Box } = d.tags;
  const fit = taskColumns(p.tasks, view.columns - ROW_ROOM);
  const stacked = fit.layout === "stacked" || (d.isWhole && fit.dropped);
  return (
    <Box flexDirection="column">
      {heading(d, "TASKS")}
      {p.tasks.map((t) =>
        stacked ? drawStackedTask(d, t, fit.columns) : drawLineTask(d, t, fit.columns),
      )}
    </Box>
  );
}

function drawBar(d: Draw, s: FleetSeat, cells: number) {
  const { Text } = d.tags;
  const bar = contextBar(s.contextTokens, cells);
  if (bar === null)
    return (
      <Text hover={ROW_HOVER}>
        <Text color={INK.faint}>{"·".repeat(cells)}</Text>
        <Text color={INK.dim}>{" no requests"}</Text>
      </Text>
    );
  return (
    <Text hover={ROW_HOVER}>
      {bar.runs.map((run) => (
        <Text color={s.closed ? INK.dim : INK[run.colour]} bold={run.bold && !s.closed}>
          {run.text}
        </Text>
      ))}
      {" "}
      <Text color={s.closed ? INK.dim : INK[bar.colour]}>{bar.number}</Text>
      {bar.replace && " "}
      {bar.replace && (
        <Text bold={!s.closed} color={s.closed ? INK.dim : INK.red}>
          replace
        </Text>
      )}
    </Text>
  );
}

function drawCache(d: Draw, s: FleetSeat, withPrefix: boolean) {
  const { Text } = d.tags;
  const cache = cacheStart(s);
  if (cache === null) return null;
  const loud = cache.cold && !s.closed;
  return (
    <Text bold={loud} color={loud ? INK.red : INK.dim} wrap={d.fit} hover={ROW_HOVER}>
      {withPrefix ? `first request ${cache.text}` : cache.text}
    </Text>
  );
}

function seatCell(d: Draw, s: FleetSeat, key: SeatKey, cells: number) {
  const { Text } = d.tags;
  const tone = (colour: string | undefined) => (s.closed ? INK.dim : colour);
  if (key === "seat")
    return (
      <Text bold color={tone(roleColour(s.role))} wrap={d.fit} hover={ROW_HOVER}>
        {s.seat}
      </Text>
    );
  if (key === "state")
    return (
      <Text color={tone(INK[seatStateColour(s.state)])} wrap={d.fit} hover={ROW_HOVER}>
        {s.state}
      </Text>
    );
  if (key === "context") return drawBar(d, s, cells);
  if (key === "last")
    return (
      <Text color={INK.dim} hover={ROW_HOVER}>
        {ago(s.lastRequestAt, d.now)}
      </Text>
    );
  if (key === "requests")
    return (
      <Text color={tone(undefined)} hover={ROW_HOVER}>
        {String(s.requests)}
      </Text>
    );
  if (key === "model")
    return (
      <Text color={INK.dim} hover={ROW_HOVER}>
        {s.model}
      </Text>
    );
  if (key === "role")
    return (
      <Text color={INK.dim} wrap={d.fit} hover={ROW_HOVER}>
        {s.role}
      </Text>
    );
  return drawCache(d, s, false);
}

function drawSeatHead(d: Draw, columns: Column<SeatKey>[]) {
  const { Box, Text } = d.tags;
  return (
    <Box paddingLeft={1} columnGap={1}>
      {columns.map((c) => cell(d, c, <Text color={INK.dim}>{SEAT_HEAD[c.key]}</Text>))}
    </Box>
  );
}

function drawLineSeat(
  d: Draw,
  s: FleetSeat,
  key: string,
  columns: Column<SeatKey>[],
  cells: number,
) {
  const { Box } = d.tags;
  return (
    <Box key={key} paddingLeft={1} columnGap={1}>
      {columns.map((c) => cell(d, c, seatCell(d, s, c.key, cells)))}
    </Box>
  );
}

function drawStackedSeat(d: Draw, s: FleetSeat, key: string, cells: number) {
  const { Box, Text } = d.tags;
  return (
    <Box key={key} flexDirection="column" paddingLeft={1}>
      <Box flexWrap="wrap" columnGap={2}>
        {seatCell(d, s, "seat", cells)}
        {seatCell(d, s, "role", cells)}
        {seatCell(d, s, "model", cells)}
        {seatCell(d, s, "state", cells)}
      </Box>
      <Box paddingLeft={2}>
        <Text wrap={d.fit} hover={ROW_HOVER}>
          {drawBar(d, s, cells)}
          {"   "}
          <Text color={s.closed ? INK.dim : undefined}>{`${s.requests} requests`}</Text>
          {"   "}
          <Text color={INK.dim}>{ago(s.lastRequestAt, d.now)}</Text>
          {cacheStart(s) !== null && "   "}
          {drawCache(d, s, true)}
        </Text>
      </Box>
    </Box>
  );
}

function drawClosedButton(d: Draw, closed: number, isShown: boolean, press: () => void) {
  const { Box, Button } = d.tags;
  if (closed === 0) return null;
  return (
    <Box key="closed-seats-box" paddingLeft={1} flexShrink={0}>
      <Button
        key="closed-seats"
        label={closedSeatsLabel(closed, isShown)}
        hotkey="c"
        variant="secondary"
        onPress={press}
      />
    </Box>
  );
}

function drawSeats(d: Draw, p: FleetPanel, view: PanelView, press: () => void) {
  const { Box } = d.tags;
  const { shown, closed } = seatRows(p.seats, d.isWhole || view.closedShown);
  const fit = seatColumns(shown, view.columns - ROW_ROOM);
  const stacked = fit.layout === "stacked" || (d.isWhole && fit.dropped);
  const names = shown.map((s) => s.seat);
  const { Text } = d.tags;
  return (
    <Box flexDirection="column">
      {heading(d, "SEATS")}
      {shown.length === 0 && (
        <Box paddingLeft={1}>
          <Text color={INK.dim}>no seat is open</Text>
        </Box>
      )}
      {!stacked && shown.length > 0 && drawSeatHead(d, fit.columns)}
      {shown.map((s, i) => {
        const key = rowKey("seat-row", s.seat, i, names);
        return stacked
          ? drawStackedSeat(d, s, key, fit.barCells)
          : drawLineSeat(d, s, key, fit.columns, fit.barCells);
      })}
      {!d.isWhole && drawClosedButton(d, closed, view.closedShown, press)}
    </Box>
  );
}

function drawVerdicts(d: Draw, p: FleetPanel) {
  const { Box, Text } = d.tags;
  return (
    <Box flexDirection="column">
      {heading(d, "LATEST FROM ACCEPTANCE")}
      {p.verdicts.length === 0 && (
        <Box paddingLeft={1}>
          <Text color={INK.dim}>nothing judged yet</Text>
        </Box>
      )}
      {p.verdicts.map((v) => (
        <Box paddingLeft={1}>
          <Text wrap={d.fit}>
            <Text color={INK.dim}>{hourAndMinute(v.at)}</Text>
            {"  "}
            <Text bold color={v.verdict === "pass" ? INK.green : INK.red}>
              {v.verdict.toUpperCase()}
            </Text>
            {"  "}
            <Text>{v.title}</Text>
          </Text>
        </Box>
      ))}
    </Box>
  );
}

export type NeedActions = {
  open?: () => void;
  hide?: () => void;
  dismiss: () => void;
  stop?: () => void;
  yes?: () => void;
  no?: () => void;
};

function needStop(tags: PanelTags, need: Attention, p: FleetPanel, a: NeedActions, isArmed: boolean) {
  const { stop, yes, no } = a;
  if (need.head !== "NO SEAT IS OPEN" || !stop || !yes || !no) return null;
  return drawStop(tags, p, isArmed, { stop, yes, no });
}

export function drawNeed(
  tags: PanelTags,
  need: Attention,
  p: FleetPanel,
  buttons: NeedActions,
  isArmed: boolean,
) {
  const { Box, Text, Button } = tags;
  const urgent = need.head === "NEEDS YOU";
  return (
    <Box columnGap={2}>
      <Text wrap={WHOLE}>
        <Text bold backgroundColor={urgent ? INK.red : INK.orange} color={INK.ground}>
          {` ${need.head} `}
        </Text>
        {"  "}
        {urgent ? (
          <Text>
            <Text bold color={seatColour(p, need.seat ?? "")}>
              {need.seat}
            </Text>
            {"  "}
            <Text>{need.title}</Text>
            {(need.more ?? 0) > 0 && <Text color={INK.dim}>{`  +${need.more} more`}</Text>}
          </Text>
        ) : (
          <Text>
            <Text color={INK.dim}>{"fleet "}</Text>
            <Text bold>{need.slug}</Text>
            <Text>
              {need.head === "NO SEAT IS OPEN"
                ? `  ${need.words}`
                : `  every open seat has been idle for ${need.minutes}m`}
            </Text>
          </Text>
        )}
      </Text>
      {needStop(tags, need, p, buttons, isArmed)}
      {buttons.open !== undefined && (
        <Button key="open" label="Open" hotkey="o" variant="secondary" onPress={buttons.open} />
      )}
      {buttons.hide !== undefined && (
        <Button key="hide" label="Hide" hotkey="h" variant="secondary" dimColor onPress={buttons.hide} />
      )}
      <Button key="dismiss" label="Dismiss" hotkey="d" variant="secondary" dimColor onPress={buttons.dismiss} />
    </Box>
  );
}

export function drawPanel(tags: PanelTags, p: FleetPanel, view: PanelView, actions: PanelActions) {
  const { Box, Text } = tags;
  const d: Draw = {
    tags,
    fit: view.isWhole ? WHOLE : CUT,
    isWhole: view.isWhole,
    now: p.now,
    open: view.openTasks,
    pressTask: actions.task,
  };
  const fill = view.surface === "terminal" ? undefined : INK.ground;
  const inner = Math.max(0, view.columns - FRAME_CELLS);
  const sections = [
    drawIntro(d, p),
    drawOpen(d, p),
    drawBlocked(d, p),
    ...(view.isShown
      ? [drawTasks(d, p, view), drawSeats(d, p, view, actions.closed), drawVerdicts(d, p)]
      : []),
  ].filter((s) => s !== null);
  const body: RenderChildren[] = [];
  sections.forEach((s, i) => {
    if (i > 0) body.push(seam(d, inner));
    body.push(s);
  });
  return (
    <Box flexDirection="column">
      {drawTitle(d, p, view, actions)}
      {view.failedAt !== null && (
        <Text color={INK.red}>
          {`mike did not answer at ${hourAndMinute(view.failedAt)} UTC; this is the status it sent at ${hourAndMinute(p.now)} UTC.`}
        </Text>
      )}
      {drawAlarm(d, p, fill, view, actions)}
      <Box
        flexDirection="column"
        borderStyle="round"
        borderColor={INK.faint}
        paddingX={1}
        backgroundColor={fill}
      >
        {body}
      </Box>
    </Box>
  );
}
