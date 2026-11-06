import type { RequestSummary } from './types';
import type { SessionSummary } from './types';
import { isBackgroundRequest } from './request-utils';

export interface ProjectGroup {
	project: string;
	sessions: SessionSummary[];
}

const UNGROUPED_KEY = '__ungrouped__';
const UNGROUPED_LABEL = 'Ungrouped';

export function compareByFirstSeenDesc(a: SessionSummary, b: SessionSummary): number {
	const ta = Date.parse(a.first_seen);
	const tb = Date.parse(b.first_seen);
	const va = Number.isNaN(ta) ? -Infinity : ta;
	const vb = Number.isNaN(tb) ? -Infinity : tb;
	if (vb !== va) return vb - va;
	return a.id < b.id ? 1 : a.id > b.id ? -1 : 0;
}

export function groupByProject(sessions: SessionSummary[]): ProjectGroup[] {
	if (sessions.length === 0) return [];

	const map = new Map<string, { label: string; sessions: SessionSummary[] }>();

	for (const sess of sessions) {
		const isNull = sess.project === null;
		const key = isNull ? UNGROUPED_KEY : sess.project!.toLowerCase();
		const label = isNull ? UNGROUPED_LABEL : sess.project!;
		const bucket = map.get(key);
		if (bucket) {
			bucket.sessions.push(sess);
		} else {
			map.set(key, { label, sessions: [sess] });
		}
	}

	for (const bucket of map.values()) {
		bucket.sessions.sort(compareByFirstSeenDesc);
	}

	const groups: ProjectGroup[] = [];
	const ungrouped = map.get(UNGROUPED_KEY);
	map.delete(UNGROUPED_KEY);

	const sorted = Array.from(map.values()).sort((a, b) => {
		const ta = a.sessions.length > 0 ? Date.parse(a.sessions[0]!.first_seen) : NaN;
		const tb = b.sessions.length > 0 ? Date.parse(b.sessions[0]!.first_seen) : NaN;
		const va = Number.isNaN(ta) ? -Infinity : ta;
		const vb = Number.isNaN(tb) ? -Infinity : tb;
		if (va !== vb) return vb - va;
		return a.label.localeCompare(b.label, 'en', { sensitivity: 'base' });
	});

	for (const { label, sessions } of sorted) {
		groups.push({ project: label, sessions });
	}

	if (ungrouped && ungrouped.sessions.length > 0) {
		groups.push({ project: ungrouped.label, sessions: ungrouped.sessions });
	}

	return groups;
}

export interface DayGroup {
	dayKey: string;
	dayLabel: string;
	sessionCount: number;
	projects: ProjectGroup[];
}

function formatDayLabel(date: Date): string {
	return date.toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric',
		year: 'numeric'
	});
}

function localDateKey(ts: number): string {
	const d = new Date(ts);
	const y = d.getFullYear();
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${y}-${m}-${day}`;
}

export function groupByDayThenProject(sessions: SessionSummary[], now: number): DayGroup[] {
	if (sessions.length === 0) return [];

	const todayKey = localDateKey(now);
	const yesterdayKey = localDateKey(now - 86_400_000);

	const dayBuckets = new Map<string, SessionSummary[]>();
	const UNKNOWN_KEY = '__unknown_date__';

	for (const sess of sessions) {
		const ts = Date.parse(sess.first_seen);
		const key = Number.isNaN(ts) ? UNKNOWN_KEY : localDateKey(ts);
		const bucket = dayBuckets.get(key);
		if (bucket) {
			bucket.push(sess);
		} else {
			dayBuckets.set(key, [sess]);
		}
	}

	const sortedKeys = Array.from(dayBuckets.keys())
		.filter((k) => k !== UNKNOWN_KEY)
		.sort((a, b) => b.localeCompare(a));

	if (dayBuckets.has(UNKNOWN_KEY)) {
		sortedKeys.push(UNKNOWN_KEY);
	}

	return sortedKeys.map((key) => {
		const bucket = dayBuckets.get(key)!;
		const projects = groupByProject(bucket);

		let dayLabel: string;
		if (key === UNKNOWN_KEY) {
			dayLabel = 'Unknown date';
		} else if (key === todayKey) {
			dayLabel = 'Today';
		} else if (key === yesterdayKey) {
			dayLabel = 'Yesterday';
		} else {
			const [y, mo, d] = key.split('-').map(Number) as [number, number, number];
			dayLabel = formatDayLabel(new Date(y, mo - 1, d));
		}

		const sessionCount = bucket.length;
		return { dayKey: key, dayLabel, sessionCount, projects };
	});
}

export type KickoffCandidate = Pick<
	RequestSummary,
	'id' | 'path' | 'max_tokens' | 'created_at' | 'turn_origin' | 'human_prompt'
>;

const COMMAND_WRAPPER_RE =
	/<(local-command-caveat|command-name|command-message|command-args|local-command-stdout)>[\s\S]*?<\/\1>/g;

const SESSION_WRAP_OPEN_RE = /<\/?session>/g;

export function isLocalCommandOnlyPrompt(humanPrompt: string): boolean {
	if (humanPrompt.length === 0) return true;
	const stripped = humanPrompt.replace(COMMAND_WRAPPER_RE, '').replace(SESSION_WRAP_OPEN_RE, '');
	return !/[\p{L}\p{N}]/u.test(stripped);
}

export function selectKickoffRequest(rows: readonly KickoffCandidate[]): number | null {
	let genuineId: number | null = null;
	let genuineTime = Number.POSITIVE_INFINITY;
	let fallbackId: number | null = null;
	let fallbackTime = Number.POSITIVE_INFINITY;

	for (const row of rows) {
		if (!row || row.path !== '/v1/messages') continue;
		const origin = row.turn_origin ?? 'primary';
		if (origin !== 'primary' && origin !== 'primary_first') continue;

		const t = Date.parse(row.created_at);
		const time = Number.isNaN(t) ? Number.POSITIVE_INFINITY : t;

		if (
			time < fallbackTime ||
			(time === fallbackTime && (fallbackId === null || row.id < fallbackId))
		) {
			fallbackId = row.id;
			fallbackTime = time;
		}

		if (isBackgroundRequest(row)) continue;
		if (isLocalCommandOnlyPrompt(row.human_prompt)) continue;

		if (
			time < genuineTime ||
			(time === genuineTime && (genuineId === null || row.id < genuineId))
		) {
			genuineId = row.id;
			genuineTime = time;
		}
	}

	return genuineId ?? fallbackId;
}
