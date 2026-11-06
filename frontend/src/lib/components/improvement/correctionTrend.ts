import type { TrendPoint } from '$lib/api/improvement';
import { share } from '$lib/components/judges/judges';

const DAY_MS = 86_400_000;

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function day(ms: number): string {
	const d = new Date(ms);
	return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
}

/** The last day a week covers: the day before its midnight end. */
export function lastDayLabel(weekEnd: string): string {
	return day(Date.parse(weekEnd) - DAY_MS);
}

/** "22–28 Sep": the seven days before `weekEnd`, which is midnight UTC after the last one. */
export function weekLabel(weekEnd: string): string {
	const end = Date.parse(weekEnd);
	const first = day(end - 7 * DAY_MS);
	const last = day(end - DAY_MS);
	const [firstDay, firstMonth] = first.split(' ');
	const [, lastMonth] = last.split(' ');
	return firstMonth === lastMonth ? `${firstDay}–${last}` : `${first} – ${last}`;
}

/** The corrected share as the Judge page writes it (`judges.ts` `share`), so both pages agree. */
export function sharePct(point: TrendPoint): string {
	return share(point.k, point.n);
}

export function shareValue(point: TrendPoint): number | null {
	return point.n > 0 ? (point.k / point.n) * 100 : null;
}

export function judgeChanged(point: TrendPoint): boolean {
	return point.judgesStarted.length > 0 || point.judgesStopped.length > 0;
}

function list(names: string[]): string {
	const last = names.at(-1) ?? '';
	const rest = names.filter((_, i) => i < names.length - 1);
	return rest.length === 0 ? last : `${rest.join(', ')} and ${last}`;
}

/** "configuration 14 began labelling; google/gemini-3-flash-preview stopped." */
export function changeSentence(point: TrendPoint): string {
	const parts: string[] = [];
	if (point.judgesStarted.length > 0) parts.push(`${list(point.judgesStarted)} began labelling`);
	if (point.judgesStopped.length > 0) parts.push(`${list(point.judgesStopped)} stopped`);
	return `${parts.join('; ')}.`;
}

/** Y-axis top and ticks in percent: clean steps of 10 (or 20 above 60%). */
export function yTicks(maxPct: number): number[] {
	const top = Math.max(10, Math.ceil(maxPct / 10) * 10);
	const step = top > 60 ? 20 : 10;
	const ticks: number[] = [];
	for (let t = 0; t <= top; t += step) ticks.push(t);
	if (ticks.at(-1) !== top) ticks.push(ticks.at(-1)! + step);
	return ticks;
}

/** Which week indices get a date under the x axis, always keeping the latest. */
export function xTickIndices(count: number, plotWidth: number, minGap = 56): number[] {
	if (count === 0) return [];
	const every = Math.max(1, Math.ceil((minGap * Math.max(count - 1, 1)) / Math.max(plotWidth, 1)));
	const out: number[] = [];
	for (let i = count - 1; i >= 0; i -= every) out.unshift(i);
	return out;
}

/** The week (by index) whose seven days contain `iso`, or null when it falls outside them all. */
export function weekIndexOf(weekly: TrendPoint[], iso: string): number | null {
	const t = Date.parse(iso);
	if (Number.isNaN(t)) return null;
	const idx = weekly.findIndex((p) => {
		const end = Date.parse(p.weekEnd);
		return t >= end - 7 * DAY_MS && t < end;
	});
	return idx === -1 ? null : idx;
}
