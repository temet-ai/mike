import type { ScheduledJob, ScheduledJobWindow } from '$lib/api/scheduledJobs';
import { parseIso } from '$lib/time';

/** The axis runs from 12:00 UTC to 12:00 UTC, so the nightly chain sits unbroken in the middle. */
export const AXIS_START_MIN = 12 * 60;
const DAY_MIN = 24 * 60;
const HOUR_MS = 3_600_000;

export const AXIS_TICKS_WIDE = [12, 15, 18, 21, 0, 3, 6, 9, 12];
export const AXIS_TICKS_NARROW = [12, 18, 0, 6, 12];

/** A stretch of the axis, in percent of its width. */
export interface Segment {
	left: number;
	width: number;
}

const pad2 = (n: number): string => String(n).padStart(2, '0');
export const clock = (hour: number, minute: number): string => `${pad2(hour)}:${pad2(minute)}`;

/** Position on the axis, in percent, of a UTC time of day in minutes. */
export function axisPercent(minuteOfDay: number): number {
	return (((((minuteOfDay - AXIS_START_MIN) % DAY_MIN) + DAY_MIN) % DAY_MIN) / DAY_MIN) * 100;
}

/** A span from a UTC time of day, split in two where it crosses the 12:00 UTC seam. */
export function spanSegments(startMin: number, durationMin: number): Segment[] {
	const left = axisPercent(startMin);
	const width = (Math.min(Math.max(durationMin, 0), DAY_MIN) / DAY_MIN) * 100;
	if (left + width <= 100 + 1e-9) return [{ left, width }];
	return [
		{ left, width: 100 - left },
		{ left: 0, width: left + width - 100 }
	];
}

export type WindowShape =
	| { kind: 'bar'; segments: Segment[]; weekday: string | null }
	| { kind: 'ticks'; ticks: number[] }
	| { kind: 'none' };

/** The first and last hour of a window, in window order. */
function windowEnds(window: ScheduledJobWindow): [number, number] {
	return [window.hours_utc[0] ?? 0, window.hours_utc[window.hours_utc.length - 1] ?? 0];
}

function tickMinutes(window: ScheduledJobWindow): number[] {
	return window.hours_utc.map((h) => h * 60 + window.minute);
}

/** What to draw in the lane for a window. */
export function windowShape(window: ScheduledJobWindow | null): WindowShape {
	if (!window || window.hours_utc.length === 0) return { kind: 'none' };
	if (window.period === 'hour') {
		return { kind: 'ticks', ticks: tickMinutes(window).map(axisPercent) };
	}
	const [first, last] = windowEnds(window);
	const start = first * 60 + window.minute;
	const duration = ((((last + 1) * 60 - start) % DAY_MIN) + DAY_MIN) % DAY_MIN || DAY_MIN;
	return {
		kind: 'bar',
		segments: spanSegments(start, duration),
		weekday: window.period === 'week' ? window.weekday : null
	};
}

/** The stretches that light up while a lane is hovered; a tick widens to its hour. */
export function hoverSegments(window: ScheduledJobWindow | null): Segment[] {
	if (!window || window.hours_utc.length === 0) return [];
	const shape = windowShape(window);
	if (shape.kind === 'bar') return shape.segments;
	return tickMinutes(window).flatMap((m) => spanSegments(m, 60));
}

/** Where a window starts on the axis, in percent; Infinity for none. Orders jobs in a group. */
export function windowStart(window: ScheduledJobWindow | null): number {
	const shape = windowShape(window);
	if (shape.kind === 'bar') return shape.segments[0]?.left ?? Number.POSITIVE_INFINITY;
	if (shape.kind === 'ticks') return Math.min(...shape.ticks);
	return Number.POSITIVE_INFINITY;
}

export interface ZonedParts {
	year: number;
	month: number;
	day: number;
	hour: number;
	minute: number;
	second: number;
	weekday: string;
}

const formatters = new Map<string, Intl.DateTimeFormat>();

function formatter(timeZone: string): Intl.DateTimeFormat {
	let f = formatters.get(timeZone);
	if (!f) {
		try {
			f = new Intl.DateTimeFormat('en-GB', {
				timeZone,
				hourCycle: 'h23',
				weekday: 'short',
				year: 'numeric',
				month: '2-digit',
				day: '2-digit',
				hour: '2-digit',
				minute: '2-digit',
				second: '2-digit'
			});
		} catch {
			f = formatter('UTC');
		}
		formatters.set(timeZone, f);
	}
	return f;
}

export function zonedParts(ms: number, timeZone: string): ZonedParts {
	const parts = formatter(timeZone).formatToParts(new Date(ms));
	const num = (type: string): number => Number(parts.find((p) => p.type === type)?.value ?? 0);
	return {
		year: num('year'),
		month: num('month'),
		day: num('day'),
		hour: num('hour'),
		minute: num('minute'),
		second: num('second'),
		weekday: parts.find((p) => p.type === 'weekday')?.value ?? ''
	};
}

export const utcClock = (ms: number): string => {
	const d = new Date(ms);
	return clock(d.getUTCHours(), d.getUTCMinutes());
};

export function localClock(ms: number, timeZone: string): string {
	const p = zonedParts(ms, timeZone);
	return clock(p.hour, p.minute);
}

/** The zone's offset from UTC at that moment, in minutes. */
export function offsetMinutes(ms: number, timeZone: string): number {
	const p = zonedParts(ms, timeZone);
	const asUtc = Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute);
	return Math.round((asUtc - Math.floor(ms / 60_000) * 60_000) / 60_000);
}

/** "UTC+2", "UTC-3:30", "UTC+0". */
export function offsetLabel(minutes: number): string {
	const sign = minutes < 0 ? '-' : '+';
	const abs = Math.abs(minutes);
	const h = Math.floor(abs / 60);
	const m = abs % 60;
	return `UTC${sign}${h}${m ? `:${pad2(m)}` : ''}`;
}

/** The moment of a UTC time of day on the UTC date of `now`. */
function atUtc(now: number, hour: number, minute: number): number {
	const d = new Date(now);
	return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), hour, minute);
}

export interface AxisLabel {
	utc: string;
	local: string;
	percent: number;
}

export function axisLabels(now: number, timeZone: string, ticks: readonly number[]): AxisLabel[] {
	return ticks.map((hour, i) => ({
		utc: pad2(hour),
		local: pad2(zonedParts(atUtc(now, hour, 0), timeZone).hour),
		percent: (i / (ticks.length - 1)) * 100
	}));
}

/** "22:00 UTC" for a UTC clock and "00:00 UTC · 02:00 local" style text for a moment. */
export function stampText(iso: string | null, timeZone: string, withLocal: boolean): string {
	const parsed = parseIso(iso);
	if (!parsed) return '—';
	const ms = parsed.getTime();
	const u = zonedParts(ms, 'UTC');
	const date = `${u.year}-${pad2(u.month)}-${pad2(u.day)}`;
	const utc = `${date} ${clock(u.hour, u.minute)}:${pad2(u.second)} UTC`;
	if (!withLocal) return utc;
	const l = zonedParts(ms, timeZone);
	return `${utc} · ${clock(l.hour, l.minute)}:${pad2(l.second)} local`;
}

function listJoin(items: readonly string[]): string {
	if (items.length <= 1) return items.join('');
	const lastIndex = items.length - 1;
	return `${items.filter((_, i) => i < lastIndex).join(', ')} and ${items[lastIndex] ?? ''}`;
}

/** "22:00–23:59 UTC · 00:00–01:59 local" for a bar window; the lane label for others. */
export function windowRange(
	window: ScheduledJobWindow,
	now: number,
	timeZone: string,
	withLocal: boolean
): string {
	const [first, last] = windowEnds(window);
	const isTicks = window.period === 'hour';
	const endMinute = isTicks ? window.minute : 59;
	const utc = `${clock(first, window.minute)}–${clock(last, endMinute)} UTC`;
	if (!withLocal) return utc;
	const from = localClock(atUtc(now, first, window.minute), timeZone);
	const to = localClock(atUtc(now, last, endMinute), timeZone);
	return `${utc} · ${from}–${to} local`;
}

function describeWindow(window: ScheduledJobWindow, now: number, timeZone: string): string {
	const showLocal = offsetMinutes(now, timeZone) !== 0;
	const local = (items: string[], joiner: (xs: string[]) => string): string =>
		showLocal ? ` (${joiner(items)} local)` : '';
	const [first, last] = windowEnds(window);
	const at = (h: number, m: number): string => localClock(atUtc(now, h, m), timeZone);
	if (window.period === 'hour') {
		if (window.hours_utc.length <= 4) {
			const utc = window.hours_utc.map((h) => clock(h, window.minute));
			const loc = window.hours_utc.map((h) => at(h, window.minute));
			return `Starts at ${listJoin(utc)} UTC${local(loc, listJoin)}.`;
		}
		const range = `${clock(first, window.minute)} to ${clock(last, window.minute)}`;
		const loc = `${at(first, window.minute)} to ${at(last, window.minute)}`;
		return `Starts hourly at :${pad2(window.minute)} from ${range} UTC${local([loc], (x) => x[0] ?? '')}.`;
	}
	const span = `${clock(first, window.minute)} and ${clock(last, 59)}`;
	const locSpan = `${at(first, window.minute)} and ${at(last, 59)}`;
	const when = window.period === 'week' ? `on ${weekdayName(window.weekday)}` : 'a day';
	return `Starts once ${when} between ${span} UTC${local([locSpan], (x) => x[0] ?? '')}.`;
}

const WEEKDAYS: Record<string, string> = {
	Mon: 'Monday',
	Tue: 'Tuesday',
	Wed: 'Wednesday',
	Thu: 'Thursday',
	Fri: 'Friday',
	Sat: 'Saturday',
	Sun: 'Sunday'
};
const weekdayName = (short: string | null): string =>
	(short && WEEKDAYS[short]) || short || 'a day';

export interface RunStrip {
	segments: Segment[];
	kind: 'live' | 'complete' | 'error';
}

function minuteOfDay(ms: number): number {
	const d = new Date(ms);
	return d.getUTCHours() * 60 + d.getUTCMinutes() + d.getUTCSeconds() / 60;
}

/** The run inside the 24 hours before `now`, as a strip on the axis; null when there is none. */
export function runStrip(job: ScheduledJob, now: number): RunStrip | null {
	const running = job.running_since !== null || job.state === 'running';
	const start = parseIso(
		running ? (job.running_since ?? job.last_started_at) : job.last_started_at
	);
	if (!start) return null;
	const startMs = start.getTime();
	if (startMs > now || now - startMs > 24 * HOUR_MS) return null;
	const finished = parseIso(job.last_finished_at);
	const endMs = running ? now : (finished?.getTime() ?? startMs);
	const duration = Math.max(0, (endMs - startMs) / 60_000);
	const kind = running ? 'live' : job.last_exit_code === 0 ? 'complete' : 'error';
	return { segments: spanSegments(minuteOfDay(startMs), duration), kind };
}

/** The lane's accessible text, also shown in its tooltip. */
export function describeLane(job: ScheduledJob, now: number, timeZone: string): string {
	if (!job.window || job.window.hours_utc.length === 0) return 'No schedule window reported.';
	const head = describeWindow(job.window, now, timeZone);
	const by =
		job.last_trigger === 'manual'
			? ', by hand'
			: job.last_trigger === 'schedule'
				? ', on schedule'
				: '';
	const strip = runStrip(job, now);
	if (!strip) return `${head} No run in the last 24 hours.`;
	if (strip.kind === 'live') {
		const since = parseIso(job.running_since ?? job.last_started_at);
		return `${head} Running since ${utcClock(since?.getTime() ?? now)} UTC${by}.`;
	}
	const start = parseIso(job.last_started_at);
	const finished = parseIso(job.last_finished_at);
	const span = finished
		? `${utcClock(start?.getTime() ?? now)}–${utcClock(finished.getTime())}`
		: utcClock(start?.getTime() ?? now);
	const exit = job.last_exit_code === null ? '' : `, exit ${job.last_exit_code}`;
	return `${head} Last run ${span} UTC${by}${exit}.`;
}
