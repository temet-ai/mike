import type {
	CacheBreak,
	CacheBreakDetail,
	CacheBreakGroup,
	CacheBreakLocation
} from '$lib/api/cacheBreaks';
import { formatDuration, formatCompactCount } from '$lib/time';

export const COLUMN_TOOLTIPS = {
	kind: 'A break rewrote cache the request should have read. An expiry resumed after the cache had lapsed.',
	cause:
		'mike names the pipeline stage that changed the prompt. Claude Code means the client changed its own tools, system or history. Unknown means nothing explains it.',
	where: 'The part of the prompt where the cached prefix stopped matching.',
	tokens: 'Tokens written to cache that the request should have read.',
	gap: 'Time since the previous request in the session.'
} as const;

const LOCATION_LABELS: Record<CacheBreakLocation, string> = {
	tools: 'tools',
	system: 'system',
	messages_prefix: 'earlier messages',
	delta: 'new turn',
	continuation: 'continuation',
	chain: 'chain',
	none: 'none',
	unreadable: 'unreadable'
};

export function locationLabel(location: string | null): string {
	if (location === null) return '—';
	return Object.hasOwn(LOCATION_LABELS, location)
		? LOCATION_LABELS[location as CacheBreakLocation]
		: location;
}

export function formatTokens(tokens: number): string {
	return formatCompactCount(tokens);
}

export function formatGap(seconds: number): string {
	return formatDuration(seconds * 1_000);
}

export function plural(count: number, noun: string): string {
	return `${count.toLocaleString('en-US')} ${noun}${count === 1 ? '' : 's'}`;
}

type Causal = Pick<CacheBreak, 'cause' | 'stage' | 'location' | 'woken_by'>;

/** The cause as one phrase, with mike's stage name first when mike made the change. */
export function causeLabel(item: Causal): string {
	switch (item.cause) {
		case 'mike':
			return item.stage ?? 'mike';
		case 'client':
			return 'Claude Code';
		case 'expired':
			return item.woken_by ? `expired, woken by ${item.woken_by}` : 'expired';
		default:
			return 'unknown';
	}
}

/** The group heading: the cause plus where the prefix broke, for causes that have one. */
export function groupLabel(group: CacheBreakGroup): string {
	const base = causeLabel(group);
	if (group.cause === 'mike' || group.cause === 'expired' || group.location === null) return base;
	return `${base} · ${locationLabel(group.location)}`;
}

const CAUSE_ORDER = { mike: 0, client: 1, unknown: 2, expired: 3 } as const;

/** Mike-caused first, then client, unknown and expiries; the server's order holds within each. */
export function orderGroups(groups: readonly CacheBreakGroup[]): CacheBreakGroup[] {
	return groups
		.map((group, index) => ({ group, index }))
		.sort((a, b) => CAUSE_ORDER[a.group.cause] - CAUSE_ORDER[b.group.cause] || a.index - b.index)
		.map(({ group }) => group);
}

export function groupKey(group: CacheBreakGroup): string {
	return [group.kind, group.cause, group.stage, group.location, group.woken_by].join('|');
}

export function totals(groups: readonly CacheBreakGroup[]): { breaks: number; tokens: number } {
	let breaks = 0;
	let tokens = 0;
	for (const group of groups) {
		breaks += group.breaks;
		tokens += group.tokens_rewritten;
	}
	return { breaks, tokens };
}

function list(values: string[] | undefined): string[] {
	return Array.isArray(values) ? values : [];
}

/** Short facts the scanner found about one break, for the line under its cause. */
export function detailNotes(detail: CacheBreakDetail | null | undefined): string[] {
	if (!detail) return [];
	const notes: string[] = [];
	const added = list(detail.tools_added);
	const removed = list(detail.tools_removed);
	if (added.length > 0) notes.push(`tools added: ${added.join(', ')}`);
	if (removed.length > 0) notes.push(`tools removed: ${removed.join(', ')}`);
	if (detail.filter_from !== undefined || detail.filter_to !== undefined) {
		notes.push(`filter ${detail.filter_from ?? '—'} → ${detail.filter_to ?? '—'}`);
	}
	const stages = list(detail.applied_stages);
	if (stages.length > 0) notes.push(`stages applied: ${stages.join(', ')}`);
	if (typeof detail.message === 'number') notes.push(`message ${detail.message}`);
	return notes;
}
