import type {
	ActionState,
	FollowupKind,
	FollowupLimits,
	FollowupState,
	RepairState,
	SteerChange,
	WrightAction,
	WrightFollowup,
	WrightMessage,
	WrightProblem,
	WrightRemedy
} from '$lib/api/wright';

/**
 * What the Wright page says about the rows its API returns. Pure functions, so the wording and
 * the links are tested without a browser.
 */

export type LinkKind = 'problem' | 'action' | 'undo' | 'measure' | 'fix' | 'evidence';

export interface WrightLink {
	kind: LinkKind;
	text: string;
	href?: string;
	external?: boolean;
}

const CHECK_TITLES: Record<string, string> = {
	tool_contract_errors: 'A tool called with the wrong inputs',
	session_tool_errors: 'Tool calls failing in sessions',
	mcp_tool_errors: "mike's own tools failing",
	hook_errors: 'A hook refusing or failing',
	slow_queries: 'A slow database query',
	log_warnings: "Warnings in mike's log",
	log_errors: "Errors in mike's log",
	request_failures: 'Requests failing',
	llm_role_errors: "A mike worker's model calls failing",
	undelivered_mail: 'Mail waiting for a seat that is not reading'
};

export function humanise(code: string): string {
	const words = code.replace(/[_-]+/g, ' ').trim();
	return words.length === 0 ? code : words.charAt(0).toUpperCase() + words.slice(1);
}

export function problemTitle(checkName: string): string {
	return CHECK_TITLES[checkName] ?? humanise(checkName);
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Numbers in the latest evidence, as one line: the full evidence is in the problem's detail. */
export function evidenceFigures(evidence: Record<string, unknown>): string[] {
	const out: string[] = [];
	for (const [key, value] of Object.entries(evidence)) {
		if (typeof value === 'number' && !key.endsWith('_id') && !key.endsWith('_run')) {
			out.push(`${humanise(key).toLowerCase()} ${value}`);
		} else if (key === 'window' && typeof value === 'string') {
			out.push(`window ${value}`);
		}
	}
	return out;
}

/**
 * Links from a problem to the places its evidence lives. A request id links to the inspect API's
 * request (the SPA has no request route without its session); a session id, a tool and a thread
 * link to their pages. A log file has no page, so it is a label.
 */
export function problemLinks(problem: WrightProblem): WrightLink[] {
	const evidence = problem.latest_evidence;
	const links: WrightLink[] = [
		{ kind: 'problem', text: `Problem ${problem.id}`, href: `#problem-${problem.id}` }
	];
	const sessionId = ['session_id', 'session_uuid', 'last_session_id']
		.map((key) => evidence[key])
		.find((value): value is string => typeof value === 'string' && UUID.test(value));
	const requestId = ['last_request_id', 'first_request_id']
		.map((key) => evidence[key])
		.find((value): value is number => typeof value === 'number');
	if (sessionId !== undefined && requestId !== undefined) {
		links.push({
			kind: 'evidence',
			text: `Request ${requestId}`,
			href: `/sessions/${sessionId}/requests/${requestId}`
		});
	} else if (requestId !== undefined) {
		links.push({
			kind: 'evidence',
			text: `Request ${requestId}`,
			href: `/api/requests/${requestId}`,
			external: true
		});
	}
	if (sessionId !== undefined) {
		links.push({ kind: 'evidence', text: 'Session', href: `/sessions/${sessionId}` });
	} else if (UUID.test(problem.resource)) {
		links.push({
			kind: 'evidence',
			text: 'The session it concerns',
			href: `/sessions/${problem.resource}`
		});
	}
	const tool = evidence.tool;
	if (typeof tool === 'string' && tool.length > 0) {
		links.push({
			kind: 'evidence',
			text: `Tool errors for ${tool}`,
			href: `/tools?tool_name=${encodeURIComponent(tool)}`
		});
	}
	const file = evidence.file;
	if (typeof file === 'string' && file.length > 0) {
		links.push({ kind: 'evidence', text: `Log ${file}` });
	}
	return links;
}

const ACTION_STATE: Record<
	ActionState,
	{ label: string; tone: 'active' | 'muted' | 'warning' | 'error' | 'analyzed' }
> = {
	claimed: { label: 'Starting', tone: 'analyzed' },
	applied: { label: 'Applied', tone: 'analyzed' },
	verified: { label: 'Verified', tone: 'active' },
	failed: { label: 'Failed', tone: 'error' },
	refused: { label: 'Refused', tone: 'warning' },
	undone: { label: 'Undone', tone: 'muted' },
	undo_conflict: { label: 'Undo blocked', tone: 'warning' },
	ineffective: { label: 'No change', tone: 'muted' }
};

export function actionStateBadge(state: ActionState): { label: string; tone: string } {
	return ACTION_STATE[state] ?? { label: humanise(state), tone: 'muted' };
}

/** What an action did to its problem's measure, in plain words. Null while it is unknown. */
export function actionMeasure(
	action: WrightAction,
	problem: WrightProblem | undefined
): string | null {
	if (action.state === 'ineffective')
		return 'The problem was still open after the passes this fix waits for.';
	if (action.state === 'verified') {
		return problem?.state === 'healthy'
			? 'The problem passed its check again after this action.'
			: 'The state was read again after acting and matched. The problem has not passed its check yet.';
	}
	return null;
}

export function remedyTitle(name: string): string {
	return humanise(name);
}

export interface FixVerdict {
	label: string;
	tone: 'active' | 'muted' | 'warning' | 'analyzed';
	line: string;
}

/**
 * Whether a known fix moved its measure, from the actions it took that are on screen: an action
 * that was `verified` cleared its problem, one that is `ineffective` did not.
 */
export function fixVerdict(remedy: WrightRemedy, actions: WrightAction[]): FixVerdict {
	const mine = actions.filter((a) => a.remedy === remedy.name);
	const verified = mine.filter((a) => a.state === 'verified').length;
	const ineffective = mine.filter((a) => a.state === 'ineffective').length;
	const seen = `In the ${mine.length} of its actions listed here`;
	const counted =
		mine.length === 0
			? 'It has not acted yet.'
			: `${seen}: ${verified} cleared the problem, ${ineffective} did not.`;
	if (remedy.paused_at !== null) {
		return {
			label: 'Paused',
			tone: 'warning',
			line: remedy.pause_reason ?? 'Paused. No reason was recorded.'
		};
	}
	if (!remedy.enabled) {
		return {
			label: ineffective > 0 || remedy.ineffective_runs > 0 ? 'No change · off' : 'Off',
			tone: 'muted',
			line:
				remedy.ineffective_runs > 0
					? `Turned off after ${remedy.ineffective_runs} runs that left the problem open. ${counted}`
					: `Switched off. ${counted}`
		};
	}
	if (verified > 0 && ineffective === 0) return { label: 'Better', tone: 'active', line: counted };
	if (ineffective > 0 && verified === 0)
		return { label: 'No change', tone: 'muted', line: counted };
	return { label: 'Measuring', tone: 'analyzed', line: counted };
}

export interface RepairStep {
	state: RepairState;
	label: string;
	status: 'done' | 'current' | 'todo';
}

const REPAIR_PATH: { state: RepairState; label: string }[] = [
	{ state: 'prepared', label: 'Prepared' },
	{ state: 'gating', label: 'Checked by the push hook' },
	{ state: 'landed', label: 'Landed on main' },
	{ state: 'deployed', label: 'Deployed' },
	{ state: 'healthy', label: 'Healthy' }
];

/** A repair is a path from prepared to healthy; it can also stop at `gate_failed` or `refused`. */
export function repairSteps(state: RepairState): RepairStep[] {
	const stoppedAt: Partial<Record<RepairState, number>> = {
		editing: 0,
		refused: 0,
		gate_failed: 1,
		batched: 1,
		dropped: 1,
		ineffective: 3
	};
	const reached = stoppedAt[state] ?? REPAIR_PATH.findIndex((step) => step.state === state);
	return REPAIR_PATH.map((step, i) => ({
		...step,
		status: i < reached ? 'done' : i === reached ? 'current' : 'todo'
	}));
}

export const REPAIR_STOPPED: Partial<Record<RepairState, string>> = {
	gate_failed: 'The push hook failed on this change, so it was not landed.',
	refused: 'Refused: the change touches something Wright may not change.',
	dropped: 'Taken off the day\'s branch before it landed.',
	ineffective: 'Deployed, but the problem came back or never went away.'
};

export type ChangeIcon =
	| 'pause'
	| 'resume'
	| 'open'
	| 'limit'
	| 'skip'
	| 'pin'
	| 'order'
	| 'merge'
	| 'separate'
	| 'group'
	| 'ungroup'
	| 'other';

export interface SteerView {
	icon: ChangeIcon;
	title: string;
	detail: string;
	/** Where the change points: the rows of the causes it names, on this page. */
	links: { text: string; href: string }[];
}

/** Whole numbers named by a steer field, in the order given; anything else is left out. */
function idList(value: unknown): number[] {
	return Array.isArray(value)
		? value.filter((v): v is number => typeof v === 'number' && Number.isFinite(v))
		: [];
}

/** "19", "19 and 318", "19, 318 and 400". */
function joinIds(ids: number[]): string {
	if (ids.length <= 1) return ids.map(String).join('');
	return `${ids.slice(0, -1).join(', ')} and ${ids.at(-1)}`;
}

/** "cause 19", "causes 19 and 318", or just "causes" when the steer names none. */
function causeWords(ids: number[]): string {
	if (ids.length === 0) return 'causes';
	return `${ids.length === 1 ? 'cause' : 'causes'} ${joinIds(ids)}`;
}

const causeLinks = (ids: number[]): SteerView['links'] =>
	ids.map((id) => ({ text: `Cause ${id}`, href: `#cause-${id}` }));

function causeSteer(kind: string, change: SteerChange): SteerView | null {
	const one = idList([change.cause_id]);
	const many = idList(change.cause_ids);
	const into = idList([change.into]);
	switch (kind) {
		case 'pin_cause':
			return {
				icon: 'pin',
				title: `Pin ${causeWords(one)}`,
				detail: 'A pinned cause comes before every unpinned one.',
				links: causeLinks(one)
			};
		case 'unpin_cause':
			return {
				icon: 'pin',
				title: `Unpin ${causeWords(one)}`,
				detail: 'It goes back to its place by score.',
				links: causeLinks(one)
			};
		case 'set_cause_priority': {
			const reset = change.priority === null || change.priority === undefined;
			return {
				icon: 'order',
				title: `${reset ? 'Reset the order of' : 'Set the order of'} ${causeWords(one)}`,
				detail: reset
					? 'The formula decides its place again.'
					: `Ranked as ${text(change.priority)} instead of by the formula.`,
				links: causeLinks(one)
			};
		}
		case 'merge_causes':
			return {
				icon: 'merge',
				title: `Merge ${causeWords(many)} into ${into.length > 0 ? joinIds(into) : 'one cause'}`,
				detail: 'Their problems move into it. Separate on the kept cause undoes this.',
				links: causeLinks([...into, ...many])
			};
		case 'separate_merge':
			return {
				icon: 'separate',
				title: `Separate merge ${text(change.merge_id)}`,
				detail: 'The merged cause comes back as its own cause, with its problems.',
				links: []
			};
		case 'group_causes':
			return {
				icon: 'group',
				title: `Put ${causeWords(many)} in the group ${text(change.name)}`,
				detail: 'A group only changes how the page lists causes, not their order.',
				links: causeLinks(many)
			};
		case 'ungroup_causes':
			return {
				icon: 'ungroup',
				title: `Take ${causeWords(many)} out of ${many.length === 1 ? 'its' : 'their'} group`,
				detail: 'A group only changes how the page lists causes, not their order.',
				links: causeLinks(many)
			};
		default:
			return null;
	}
}

function text(value: unknown): string {
	return typeof value === 'string' ? value : value === undefined ? '' : JSON.stringify(value);
}

function until(value: unknown): string {
	return typeof value === 'string' && value.length > 0 ? `Until ${value}` : 'Until you resume it';
}

/** The closed set of steers Wright may propose, as the card that sits under its reply. */
export function describeSteer(change: SteerChange): SteerView {
	const kind = text(change.type ?? change.kind ?? change.action);
	return causeSteer(kind, change) ?? { ...describeRemedySteer(kind, change), links: [] };
}

function describeRemedySteer(kind: string, change: SteerChange): Omit<SteerView, 'links'> {
	switch (kind) {
		case 'pause_remedy':
			return {
				icon: 'pause',
				title: `Paused: ${remedyTitle(text(change.name))}`,
				detail: until(change.until)
			};
		case 'resume_remedy':
			return {
				icon: 'resume',
				title: `Resumed: ${remedyTitle(text(change.name))}`,
				detail: 'Back on.'
			};
		case 'open_problem':
			return {
				icon: 'open',
				title: `Opened: ${problemTitle(text(change.check))}`,
				detail: text(change.resource)
			};
		case 'set_engineer_limit':
			return {
				icon: 'limit',
				title: `Limit changed: ${humanise(text(change.field))}`,
				detail: `Now ${text(change.value)}`
			};
		case 'skip_problem':
			return {
				icon: 'skip',
				title: `Skipped: problem ${text(change.id)}`,
				detail: until(change.until)
			};
		default:
			return { icon: 'other', title: 'Change made', detail: JSON.stringify(change) };
	}
}

export const MESSAGE_LINK_LABEL: Record<WrightMessage['links'][number]['kind'], string> = {
	problem: 'Problem',
	episode: 'Episode',
	action: 'Action',
	finding: 'Finding',
	repair: 'Repair'
};

/** Chips for the evidence a reply links to. Problems link to their row on this page. */
export function messageLinks(message: WrightMessage): WrightLink[] {
	return message.links.map((link) => {
		const text = `${MESSAGE_LINK_LABEL[link.kind]} ${link.id}`;
		switch (link.kind) {
			case 'problem':
				return { kind: 'problem', text, href: `#problem-${link.id}` };
			case 'action':
				return { kind: 'action', text, href: `#action-${link.id}` };
			case 'repair':
				return { kind: 'fix', text, href: `#repair-${link.id}` };
			case 'finding':
				return { kind: 'evidence', text, href: `#finding-${link.id}` };
			default:
				return { kind: 'evidence', text };
		}
	});
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function dayStart(date: Date): number {
	return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export function clockTime(iso: string): string {
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '—';
	return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

/** `Today`, `Yesterday`, or `15 Oct` (with the year once it is not this one). */
export function dayLabel(iso: string, now: number = Date.now()): string {
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '—';
	const days = Math.round((dayStart(new Date(now)) - dayStart(date)) / 86_400_000);
	if (days === 0) return 'Today';
	if (days === 1) return 'Yesterday';
	const base = `${date.getDate()} ${MONTHS[date.getMonth()]}`;
	return date.getFullYear() === new Date(now).getFullYear()
		? base
		: `${base} ${date.getFullYear()}`;
}

export function dayAndTime(iso: string, now: number = Date.now()): string {
	return `${dayLabel(iso, now)} ${clockTime(iso)}`;
}

export function groupByDay<T>(
	rows: T[],
	at: (row: T) => string,
	now: number = Date.now()
): { day: string; rows: T[] }[] {
	const groups: { day: string; rows: T[] }[] = [];
	for (const row of rows) {
		const day = dayLabel(at(row), now);
		const last = groups.at(-1);
		if (last !== undefined && last.day === day) last.rows.push(row);
		else groups.push({ day, rows: [row] });
	}
	return groups;
}

export interface EvidenceRef {
	text: string;
	href?: string;
	external?: boolean;
}

/** Where a piece of a finding's evidence lives: a session, a request or another problem link; the rest are labels. */
export function findingEvidenceRef(kind: string, ref: string): EvidenceRef {
	const text = `${humanise(kind)} ${ref}`;
	switch (kind) {
		case 'session':
			return { text, href: `/sessions/${ref}` };
		case 'request':
			return { text, href: `/api/requests/${ref}`, external: true };
		case 'problem':
			return { text, href: `#problem-${ref}` };
		default:
			return { text };
	}
}

export const GITHUB_COMMIT = 'https://github.com/temet-ai/mike/commit/';

export interface FollowupBadge {
	label: string;
	variant: 'chip-active' | 'chip-muted' | 'chip-warning' | 'chip-analyzed';
}

const FOLLOWUP_BADGE: Record<FollowupState, FollowupBadge> = {
	open: { label: 'Not filed', variant: 'chip-muted' },
	filed: { label: 'Filed', variant: 'chip-analyzed' },
	handed_off: { label: 'Being fixed', variant: 'chip-analyzed' },
	fixed: { label: 'Fixed', variant: 'chip-active' },
	dropped: { label: 'Dropped', variant: 'chip-muted' },
	stuck: { label: 'Stuck', variant: 'chip-warning' }
};

export function followupBadge(state: FollowupState): FollowupBadge {
	return FOLLOWUP_BADGE[state] ?? { label: humanise(state), variant: 'chip-muted' };
}

const FOLLOWUP_KIND: Record<FollowupKind, string> = {
	check_tuning: 'Check tuning',
	code_defect: 'Code bug',
	hardening: 'Hardening',
	root_cause: 'Why it keeps happening'
};

export function followupKind(kind: FollowupKind): string {
	return FOLLOWUP_KIND[kind] ?? humanise(kind);
}

/** The operator's choices for the Show control, in the order the page lists them. */
export const FOLLOWUP_FILTERS: { value: 'all' | FollowupState; label: string }[] = [
	{ value: 'all', label: 'All' },
	{ value: 'stuck', label: 'Stuck' },
	{ value: 'handed_off', label: 'Being fixed' },
	{ value: 'filed', label: 'Filed' },
	{ value: 'open', label: 'Not filed' },
	{ value: 'fixed', label: 'Fixed' },
	{ value: 'dropped', label: 'Dropped' }
];

/** "Kind · check · owned by part": the part is left out when the follow-up names none. */
export function followupMeta(f: WrightFollowup): string {
	const parts = [followupKind(f.kind)];
	if (f.check_name) parts.push(problemTitle(f.check_name));
	if (f.owning_part) parts.push(`owned by ${f.owning_part}`);
	return parts.join(' · ');
}

function when(iso: string | null): string {
	return iso === null ? '' : ` ${dayAndTime(iso)}`;
}

/** "attempt 2 of 3" once the limit is known, "attempt 2" until then. */
export function attemptText(attempts: number, limits: FollowupLimits | null): string {
	return limits === null ? `attempt ${attempts}` : `attempt ${attempts} of ${limits.max_attempts}`;
}

/** What the row says its state is, in one sentence. */
export function followupLine(f: WrightFollowup, limits: FollowupLimits | null = null): string {
	switch (f.state) {
		case 'open':
			return 'No issue yet.';
		case 'filed':
			return `Issue #${f.issue_number ?? '?'} is open. No session has picked it up.`;
		case 'handed_off':
			return `${f.fixer_machine ?? 'A session'}, ${attemptText(f.attempts, limits)}${
				f.handed_off_at === null ? '' : `, since ${dayAndTime(f.handed_off_at)}`
			}.`;
		case 'fixed':
			return `Closed${when(f.closed_at)}.`;
		case 'dropped':
			return `Closed${when(f.closed_at)}${f.closed_reason ? `: ${humanise(f.closed_reason)}` : ''}.`;
		case 'stuck': {
			const ended = `${f.attempts} ${f.attempts === 1 ? 'session' : 'sessions'} ended without a fix.`;
			const noIssue = f.issue_url === null ? ' No issue was filed.' : '';
			return `${ended} Wright has stopped trying.${noIssue}`;
		}
		default:
			return humanise(f.state);
	}
}

/** The plain meaning of a state, for the badge's tooltip; the daily limits are named once known. */
export function followupTooltip(
	state: FollowupState,
	limits: FollowupLimits | null = null
): string {
	switch (state) {
		case 'open':
			return `Wright wrote this down and has not opened an issue for it.${
				limits === null ? '' : ` It files at most ${limits.daily_filed} a day.`
			}`;
		case 'filed':
			return `Wright opened the issue and is waiting for a quiet machine to start a session on it.${
				limits === null ? '' : ` It starts at most ${limits.daily_handoffs} a day.`
			}`;
		case 'handed_off':
			return 'A session is working on the issue. Wright marks it fixed when the issue closes or its pull request merges.';
		case 'fixed':
			return 'The issue closed or its pull request merged.';
		case 'dropped':
			return 'The GitHub issue was closed as not planned, by the fixer session or by anyone.';
		case 'stuck':
			return 'Sessions tried this and none finished it. Wright has stopped. Read the issue and decide.';
		default:
			return humanise(state);
	}
}
