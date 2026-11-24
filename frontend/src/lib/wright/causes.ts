import type { NotInvestigatedBecause, WrightCause } from '$lib/api/wright';

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

/**
 * The engineer's order, which the server also uses: pinned causes first, then the highest
 * `rank_priority` (the operator's value when set, else the formula's), the older cause on a tie.
 */
export const byPriorityDesc = (a: WrightCause, b: WrightCause): number =>
	Number(b.pinned) - Number(a.pinned) ||
	b.rank_priority - a.rank_priority ||
	a.cause_id - b.cause_id;

/** The operator moved this cause, so its score no longer follows the formula. */
export const isMoved = (cause: WrightCause): boolean => cause.operator_priority !== null;

/* ---------------------------------------------------------------------------------- areas */

export type AreaKind = 'tables' | 'module' | 'job' | 'decisions' | 'check';

export interface CauseArea {
	/** Causes with the same key show together, whichever check they come from. */
	key: string;
	kind: AreaKind;
	/** "wright tables", "mike_features::proxy::responses_capture", "decisions_loop". */
	label: string;
	/** What the label was taken from, for the opened row: every table a query names, the full path. */
	detail: string[];
}

const SQL_TABLE =
	/\b(?:from|update|into|join)\s+(?:only\s+)?"?([a-z_][a-z0-9_]*)"?\s*\.\s*"?([a-z_][a-z0-9_]*)"?/gi;
const QUERY_CHECKS = new Set(['slow_queries', 'reads_outpace_writes', 'idle_polls']);

/** Every `schema.table` a query names after FROM, UPDATE, INTO or JOIN, in order, once each. */
export function queryTables(sql: string): string[] {
	const seen: string[] = [];
	for (const match of sql.matchAll(SQL_TABLE)) {
		const table = `${match[1]!.toLowerCase()}.${match[2]!.toLowerCase()}`;
		if (!seen.includes(table)) seen.push(table);
	}
	return seen;
}

function checkArea(cause: WrightCause): CauseArea {
	return { key: `check:${cause.check_name}`, kind: 'check', label: cause.check_name, detail: [] };
}

/**
 * The part of mike a cause belongs to, from its check and resource. Pure and local: the rules
 * come from the live queue (queries by schema, log errors by module, job failures by job name).
 * Causes that match none of them are an area of their own named by the check.
 */
export function causeArea(cause: WrightCause): CauseArea {
	const { check_name: check, resource } = cause;
	if (QUERY_CHECKS.has(check)) {
		const tables = queryTables(resource);
		const first = tables[0];
		if (first === undefined) return checkArea(cause);
		const schema = first.split('.')[0]!;
		return { key: `tables:${schema}`, kind: 'tables', label: `${schema} tables`, detail: tables };
	}
	if (check === 'log_errors') {
		const path = /^([A-Za-z_]\w*(?:::[A-Za-z_]\w*)+)/.exec(resource)?.[1];
		if (path === undefined) return checkArea(cause);
		const module = path.split('::').slice(0, 3).join('::');
		return {
			key: `module:${module}`,
			kind: 'module',
			label: module,
			detail: module === path ? [] : [path]
		};
	}
	if (check === 'scheduled_job_failures') {
		const job = /^job:([^@\s]+)/.exec(resource)?.[1];
		if (job === undefined) return checkArea(cause);
		return { key: `job:${job}`, kind: 'job', label: job, detail: [] };
	}
	if (check === 'decisions_health') {
		return { key: 'decisions', kind: 'decisions', label: 'decisions', detail: [] };
	}
	return checkArea(cause);
}

export interface TriageSection {
	/** `group:<id>` for a named group, else the area key. */
	key: string;
	kind: 'group' | 'area';
	label: string;
	detail: string[];
	causes: WrightCause[];
}

/**
 * The By area view: a section for each named group, then one for each derived area; causes in
 * rank order inside a section, sections ordered by their highest-ranked cause. A cause in a
 * group shows only in its group. `rows` arrive in the order the page shows (the store keeps it
 * still while Wright refreshes), so the sections keep it instead of sorting again.
 */
export function sectionsByArea(rows: WrightCause[]): TriageSection[] {
	const groups = new Map<string, TriageSection>();
	const areas = new Map<string, TriageSection>();
	for (const cause of rows) {
		if (cause.group_id !== null) {
			const key = `group:${cause.group_id}`;
			const section = groups.get(key) ?? {
				key,
				kind: 'group' as const,
				label: cause.group_name ?? `Group ${cause.group_id}`,
				detail: [],
				causes: []
			};
			section.causes.push(cause);
			groups.set(key, section);
			continue;
		}
		const area = causeArea(cause);
		const section = areas.get(area.key) ?? {
			key: area.key,
			kind: 'area' as const,
			label: area.label,
			detail: [],
			causes: []
		};
		section.causes.push(cause);
		for (const line of area.detail) if (!section.detail.includes(line)) section.detail.push(line);
		areas.set(area.key, section);
	}
	// Map insertion order is best-cause-first, because `rows` are walked in rank order.
	return [...groups.values(), ...areas.values()];
}

/* -------------------------------------------------------------------------------- scoring */

const DAY_MS = 86_400_000;

export interface ScoreLine {
	label: string;
	value: string;
}

const round1 = (n: number): string => (Math.round(n * 10) / 10).toString();

/**
 * How the queue's score is made. The formula's own total is the server's `priority`; the lines
 * above it are its inputs as the page can see them, so they may differ from it by a little.
 */
export function scoreBreakdown(cause: WrightCause, now: number = Date.now()): ScoreLine[] {
	const opened = Date.parse(cause.opened_at);
	const days = Number.isNaN(opened) ? 0 : Math.max(0, (now - opened) / DAY_MS);
	const lines: ScoreLine[] = [
		{ label: 'Problems', value: `${cause.problems}` },
		{
			label: 'Times seen',
			value: `${cause.observations} (adds ${round1(Math.log(1 + cause.observations))})`
		},
		{ label: 'Days open', value: round1(days) },
		{ label: 'Formula score', value: round1(cause.priority) }
	];
	if (cause.operator_priority !== null) {
		lines.push({ label: 'Your value', value: `${round1(cause.operator_priority)}, used instead` });
	}
	return lines;
}

/** Why the engineer will or will not take the cause. `null` when the server has not said yet. */
export function engineerLine(cause: WrightCause, maxAttempts: number | null): string | null {
	const lead = cause.pinned ? 'Pinned. ' : '';
	if (cause.investigable === true) {
		return cause.pinned
			? 'Pinned. The engineer takes it before the unpinned causes.'
			: 'The engineer takes it when it reaches the top of the queue.';
	}
	if (cause.investigable === null && cause.not_investigated_because === null) return null;
	const reasons: Record<NotInvestigatedBecause, string> = {
		finding: 'it already has a finding',
		followup: 'a follow-up is already filed for it',
		known_fix: 'a known fix already answers it',
		attempts:
			maxAttempts === null
				? 'it has used all its attempts'
				: `it has used all ${maxAttempts} of its attempts`
	};
	const because = cause.not_investigated_because;
	return because === null
		? `${lead}The engineer will not take it.`
		: `${lead}The engineer will not take it: ${reasons[because]}.`;
}

/* ---------------------------------------------------------------------------- reordering */

/** Where in a list a cause goes: between its two new neighbours, or past an end. */
export function priorityBetween(
	above: WrightCause | undefined,
	below: WrightCause | undefined
): number | null {
	if (above !== undefined && below !== undefined) {
		return (above.rank_priority + below.rank_priority) / 2;
	}
	if (below !== undefined) return below.rank_priority + 1;
	if (above !== undefined) return above.rank_priority - 1;
	return null;
}

/**
 * The `operator_priority` that puts cause `id` at `targetIndex` of `scope`, counted after it is
 * taken out of its place. Only causes with the same pin state count: a pin keeps its row ahead
 * of every unpinned one, so a move across that edge would change nothing. `null` when the
 * cause is already there, is not in scope or has nothing to move past.
 */
export function priorityForIndex(
	scope: WrightCause[],
	id: number,
	targetIndex: number
): number | null {
	const moving = scope.find((c) => c.cause_id === id);
	if (moving === undefined) return null;
	const zone = scope.filter((c) => c.pinned === moving.pinned);
	const from = zone.findIndex((c) => c.cause_id === id);
	const others = zone.filter((c) => c.cause_id !== id);
	const to = Math.max(0, Math.min(targetIndex, others.length));
	if (to === from) return null;
	return priorityBetween(others[to - 1], others[to]);
}

export type MoveDirection = 'up' | 'down';

/** Whether the cause has a neighbour to pass in that direction, within its pin state. */
export function canMove(scope: WrightCause[], id: number, direction: MoveDirection): boolean {
	const moving = scope.find((c) => c.cause_id === id);
	if (moving === undefined) return false;
	const zone = scope.filter((c) => c.pinned === moving.pinned);
	const at = zone.findIndex((c) => c.cause_id === id);
	return direction === 'up' ? at > 0 : at < zone.length - 1;
}

/** The value for Move up or Move down, one place within the cause's pin state. */
export function moveStep(
	scope: WrightCause[],
	id: number,
	direction: MoveDirection
): number | null {
	const moving = scope.find((c) => c.cause_id === id);
	if (moving === undefined || !canMove(scope, id, direction)) return null;
	const zone = scope.filter((c) => c.pinned === moving.pinned);
	const at = zone.findIndex((c) => c.cause_id === id);
	return priorityForIndex(scope, id, direction === 'up' ? at - 1 : at + 1);
}

export type DropEdge = 'before' | 'after';

/** The value for dropping cause `id` on the `before` or `after` edge of cause `overId`. */
export function dropStep(
	scope: WrightCause[],
	id: number,
	overId: number,
	edge: DropEdge
): number | null {
	const moving = scope.find((c) => c.cause_id === id);
	const over = scope.find((c) => c.cause_id === overId);
	if (moving === undefined || over === undefined || id === overId) return null;
	if (moving.pinned !== over.pinned) return null;
	const others = scope.filter((c) => c.pinned === moving.pinned && c.cause_id !== id);
	const at = others.findIndex((c) => c.cause_id === overId);
	return priorityForIndex(scope, id, at + (edge === 'after' ? 1 : 0));
}
