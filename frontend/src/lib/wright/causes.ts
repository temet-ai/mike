import type { WrightCause } from '$lib/api/wright';

/**
 * What the Triage queue says about the causes the API ranks. Pure functions, so the wording and
 * the links are tested without a browser. The server decides the order; this only names the
 * state each row is in.
 */

export type CauseState = 'found' | 'filed' | 'known_fix' | 'waiting' | 'queued';

export interface CauseBadge {
	label: string;
	variant: 'chip-active' | 'chip-muted' | 'chip-warning' | 'chip-analyzed';
}

const CAUSE_BADGE: Record<CauseState, CauseBadge> = {
	found: { label: 'Found', variant: 'chip-analyzed' },
	filed: { label: 'Filed', variant: 'chip-analyzed' },
	known_fix: { label: 'Known fix', variant: 'chip-active' },
	waiting: { label: 'Waiting', variant: 'chip-warning' },
	queued: { label: 'Queued', variant: 'chip-muted' }
};

export function causeBadge(state: CauseState): CauseBadge {
	return CAUSE_BADGE[state];
}

const HOUR_MS = 3_600_000;

/** A finding wins over a filed issue, a filed issue over a known fix, and only what none of them answers can be waiting. */
export function causeState(
	cause: WrightCause,
	waitedHours: number,
	now: number = Date.now()
): CauseState {
	if (cause.finding_id !== null) return 'found';
	if (cause.followup_id !== null) return 'filed';
	if (cause.remedied) return 'known_fix';
	const opened = Date.parse(cause.opened_at);
	if (!Number.isNaN(opened) && now - opened > waitedHours * HOUR_MS) return 'waiting';
	return 'queued';
}

/** What the state means, for the badge's tooltip. */
export function causeTooltip(state: CauseState, waitedHours: number): string {
	switch (state) {
		case 'found':
			return "Wright's engineer investigated this cause and wrote down what it found.";
		case 'filed':
			return 'Wright filed work for this cause. The issue tracks the fix.';
		case 'known_fix':
			return 'A known fix already answers this check, so Wright leaves the cause to it.';
		case 'waiting':
			return `Nothing has answered this cause in over ${waitedHours} hours. It needs a decision from you.`;
		case 'queued':
			return 'Wright has not got to this cause yet. It is ranked by how many problems it explains and how often they were seen.';
	}
}

/** "Open 40m", "Open 26h", "Open 5d": how long the cause has been open, in the largest whole unit. */
export function openFor(iso: string, now: number = Date.now()): string {
	const opened = Date.parse(iso);
	if (Number.isNaN(opened)) return 'Open —';
	const minutes = Math.max(0, Math.floor((now - opened) / 60_000));
	if (minutes < 60) return `Open ${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 48) return `Open ${hours}h`;
	return `Open ${Math.floor(hours / 24)}d`;
}

export function problemsText(count: number): string {
	return `${count} ${count === 1 ? 'problem' : 'problems'}`;
}

export function attemptsText(attempts: number): string {
	if (attempts === 0) return 'no attempts';
	return `${attempts} ${attempts === 1 ? 'attempt' : 'attempts'}`;
}

export interface CauseLink {
	kind: 'finding' | 'issue' | 'problem';
	text: string;
	href: string;
	external?: boolean;
}

export const GITHUB_ISSUE = 'https://github.com/temet-ai/mike/issues/';

/** Where each answer to the cause lives: the finding on this page, the issue on GitHub, the problem row. */
export function causeLinks(cause: WrightCause): CauseLink[] {
	const links: CauseLink[] = [];
	if (cause.finding_id !== null) {
		links.push({
			kind: 'finding',
			text: `${cause.finding_id}`,
			href: `#finding-${cause.finding_id}`
		});
	}
	if (cause.issue_number !== null) {
		links.push({
			kind: 'issue',
			text: `#${cause.issue_number}`,
			href: `${GITHUB_ISSUE}${cause.issue_number}`,
			external: true
		});
	}
	links.push({
		kind: 'problem',
		text: `${cause.problem_id}`,
		href: `#problem-${cause.problem_id}`
	});
	return links;
}

/** Server order: highest priority first, the older cause first on a tie. */
export const byPriorityDesc = (a: WrightCause, b: WrightCause): number =>
	b.priority - a.priority || a.cause_id - b.cause_id;
