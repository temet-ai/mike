import type { RepeatGroup } from '$lib/api/improvement';
import { formatDay } from '$lib/components/eval/evalReview';

const ISSUES_URL = 'https://github.com/temet-ai/mike/issues/new';
const MAX_HREF_LENGTH = 8000;
const TITLE_PREFIX = 'Operator keeps saying: ';
const TITLE_TEXT_MAX = 80;
const MEASURE_LINE =
	"Measure: this group's count per week on /sessions/improvement should fall to zero.";
const PASTE_LINE = 'Paste the list from your clipboard here.';
const DAY_MS = 86_400_000;

function localDay(iso: string): Date | null {
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return null;
	return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function dayDiff(from: string, to: Date): number | null {
	const a = localDay(from);
	if (!a) return null;
	const b = new Date(to.getFullYear(), to.getMonth(), to.getDate());
	return Math.round((b.getTime() - a.getTime()) / DAY_MS);
}

function plural(n: number, one: string, many: string): string {
	return `${n} ${n === 1 ? one : many}`;
}

/** The title text: the first message on one line, ended at a word boundary within 80 characters. */
function titleText(text: string): string {
	const words = text.split(/\s+/).filter((w) => w.length > 0);
	let out = '';
	for (const word of words) {
		const next = out === '' ? word : `${out} ${word}`;
		if (out !== '' && next.length > TITLE_TEXT_MAX) break;
		out = next;
	}
	return out;
}

export function issueTitle(group: RepeatGroup): string {
	return `${TITLE_PREFIX}${titleText(group.items[0]?.text ?? '')}`;
}

/** One line per item, oldest first, then the measure. The full list, never shortened. */
export function issueBody(group: RepeatGroup): string {
	const lines = group.items.map(
		(item) => `- ${formatDay(item.at) ?? item.at} · ${item.project} · ${item.text}`
	);
	return `${lines.join('\n')}\n\n${MEASURE_LINE}`;
}

function linkFor(title: string, body: string): string {
	return `${ISSUES_URL}?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
}

/** The link with the full list in it. */
export function fullIssueHref(group: RepeatGroup): string {
	return linkFor(issueTitle(group), issueBody(group));
}

/** True when the full list would not fit in a GitHub link. */
export function issueTooLong(group: RepeatGroup): boolean {
	return fullIssueHref(group).length > MAX_HREF_LENGTH;
}

/** The link the button carries: the full one, or the title and a paste line when it is too long. */
export function issueHref(group: RepeatGroup): string {
	if (!issueTooLong(group)) return fullIssueHref(group);
	return linkFor(issueTitle(group), `${PASTE_LINE}\n\n${MEASURE_LINE}`);
}

/** "{first} to {last} · {days} days · last said today / yesterday / N days ago". */
export function spanText(group: RepeatGroup, now: number): string {
	const nowDate = new Date(now);
	const first = formatDay(group.first_at, nowDate) ?? group.first_at;
	const last = formatDay(group.last_at, nowDate) ?? group.last_at;
	const lastAgo = dayDiff(group.last_at, nowDate);
	const span = localDay(group.first_at);
	const end = localDay(group.last_at);
	const days = span && end ? Math.round((end.getTime() - span.getTime()) / DAY_MS) : null;
	const parts = [`${first} to ${last}`];
	if (days !== null) parts.push(days === 0 ? 'same day' : plural(days, 'day', 'days'));
	if (lastAgo !== null) {
		const ago = lastAgo <= 0 ? 'today' : lastAgo === 1 ? 'yesterday' : `${lastAgo} days ago`;
		parts.push(`last said ${ago}`);
	}
	return parts.join(' · ');
}

function weekStartDay(date: string): string {
	return formatDay(`${date}T00:00:00`) ?? date;
}

/** "Weeks starting 24 Jul to 9 Oct". */
export function weekCaption(weekStarts: string[]): string {
	const first = weekStarts[0];
	const last = weekStarts.at(-1);
	if (first === undefined || last === undefined) return '';
	return `Weeks starting ${weekStartDay(first)} to ${weekStartDay(last)}`;
}

/** The largest weekly count over all groups, so bars compare between rows. Never below 1. */
export function scaleMax(groups: RepeatGroup[]): number {
	let max = 1;
	for (const g of groups) for (const n of g.weekly) if (n > max) max = n;
	return max;
}

/** The sentence a screen reader gets in place of the bars. */
export function weeklyReadout(weekly: number[]): string {
	return `Per week, oldest first: ${weekly.join(', ')}`;
}
