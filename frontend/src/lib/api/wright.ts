import { ApiError } from '$lib/api';
import { readOperatorToken } from '$lib/wright/pairing';

/**
 * Client for the Wright page. Wire shapes follow
 * artifacts/2026-11-14/wright/BUILD_PLAN.md and, for problems,
 * crates/mike/src/api/wright/problems.rs. The remedies, investigations, repairs and
 * conversation routes are served only after their backends deploy; a 404 from one of them is
 * reported as `isNotDeployed` so the page shows an empty section, not an error.
 */

export type ProblemState = 'open' | 'healthy';

export interface WrightProblem {
	id: number;
	check_name: string;
	scope: string;
	source: string;
	/** What tells one problem from another. For a query it is cut after 120 characters of SQL. */
	resource: string;
	/** The whole SQL text of a query problem; null for a check that is not about a statement. */
	statement: string | null;
	state: ProblemState;
	opened_at: string;
	last_seen_at: string;
	healthy_at: string | null;
	episodes: number;
	latest_evidence: Record<string, unknown>;
}

/** A problem as the wire may send it: `statement` is absent before its backend deploys. */
export type WireProblem = Omit<WrightProblem, 'statement'> & { statement?: string | null };

export function normaliseProblem(wire: WireProblem): WrightProblem {
	return { ...wire, statement: wire.statement ?? null };
}

export interface ProblemsPage {
	problems: WrightProblem[];
	total: number;
	limit: number;
	offset: number;
	next_offset: number | null;
	watch_enabled: boolean;
}

export interface WrightEpisode {
	id: number;
	opened_at: string;
	closed_at: string | null;
	last_seen_at: string;
	opened_by_run: number;
	closed_by_run: number | null;
	last_seen_run: number;
	observations: number;
	evidence: Record<string, unknown>;
}

export interface ProblemDetail {
	problem: WrightProblem;
	episodes: WrightEpisode[];
	episode_limit: number;
	episode_offset: number;
	next_episode_offset: number | null;
}

export type RemedyKind = 'setting' | 'notice';

export interface WrightRemedy {
	name: string;
	kind: RemedyKind;
	action: string;
	check_name: string;
	enabled: boolean;
	daily_limit: number;
	max_resources: number;
	paused_at: string | null;
	pause_reason: string | null;
	actions_today: number;
	ineffective_runs: number;
}

export interface RemediesResponse {
	remedies: WrightRemedy[];
}

export interface RemedyPatch {
	enabled?: boolean;
	daily_limit?: number;
	max_resources?: number;
}

export type ActionState =
	| 'claimed'
	| 'applied'
	| 'verified'
	| 'failed'
	| 'refused'
	| 'undone'
	| 'undo_conflict'
	| 'ineffective';

export interface WrightAction {
	id: number;
	remedy: string;
	problem_id: number;
	state: ActionState;
	by: 'wright' | 'operator';
	created_at: string;
	verified_at: string | null;
	effect: unknown;
	can_undo: boolean;
}

export interface ActionsPage {
	actions: WrightAction[];
	next_before: number | null;
}

export type FindingKind = 'code_defect' | 'config' | 'leftover' | 'external' | 'unknown';
export type EvidenceKind = 'request' | 'problem' | 'episode' | 'file' | 'log' | 'session';

export interface FindingEvidence {
	kind: EvidenceKind;
	ref: string;
}

export interface WrightFinding {
	id: number;
	investigation_id: number;
	problem_id: number;
	symptom: string;
	suspected_cause: string;
	confirmed_cause: string | null;
	cause_established: boolean;
	owning_part: string | null;
	kind: FindingKind;
	remedy: string;
	evidence: FindingEvidence[];
	created_at: string;
}

export interface FindingsPage {
	findings: WrightFinding[];
	next_before: number | null;
}

export interface WrightInvestigation {
	id: number;
	problem_id: number | null;
	state: string;
	session_uuid: string | null;
	started_at: string | null;
	ended_at: string | null;
	end_reason: string | null;
}

export interface InvestigationsPage {
	investigations: WrightInvestigation[];
	next_before: number | null;
}

export interface EngineerLimits {
	[field: string]: unknown;
}

export type RepairState =
	| 'editing'
	| 'prepared'
	| 'gating'
	| 'batched'
	| 'dropped'
	| 'gate_failed'
	| 'refused'
	| 'landed'
	| 'deployed'
	| 'healthy'
	| 'ineffective';

export interface WrightRepair {
	id: number;
	finding_id: number;
	problem_id: number;
	state: RepairState;
	branch: string | null;
	pr_url: string | null;
	merge_sha: string | null;
	reason: string | null;
	created_at: string;
	updated_at: string | null;
}

export interface RepairsPage {
	repairs: WrightRepair[];
	next_before: number | null;
}

export interface SteerChange {
	type?: string;
	[field: string]: unknown;
}

export interface WrightSteer {
	id: number;
	change: SteerChange;
	applied_at: string;
	undone_at: string | null;
	can_undo: boolean;
	version?: number;
}

export interface MessageLink {
	kind: 'problem' | 'episode' | 'action' | 'finding' | 'repair';
	id: number;
}

export interface WrightMessage {
	id: number;
	author: 'operator' | 'wright';
	body: string;
	reply_to: number | null;
	links: MessageLink[];
	created_at: string;
	steer: WrightSteer | null;
}

export interface MessagesPage {
	messages: WrightMessage[];
	pending_reply: boolean;
}

export type FollowupState = 'open' | 'filed' | 'handed_off' | 'fixed' | 'dropped' | 'stuck';
export type FollowupKind = 'check_tuning' | 'code_defect' | 'hardening' | 'root_cause';

/** One row of the register of filed work, from `crates/mike/src/api/wright/followups.rs`. */
export interface WrightFollowup {
	id: number;
	kind: FollowupKind;
	check_name: string | null;
	owning_part: string | null;
	title: string;
	body: string;
	state: FollowupState;
	issue_number: number | null;
	issue_url: string | null;
	attempts: number;
	fixer_session: string | null;
	fixer_machine: string | null;
	filed_at: string | null;
	handed_off_at: string | null;
	closed_at: string | null;
	closed_reason: string | null;
	created_at: string;
	updated_at: string;
	findings: number[];
	problems: number[];
}

export interface FollowupsPage {
	followups: WrightFollowup[];
	next_before: number | null;
}

/** The numbers the follow-up flow runs on, from `GET /api/wright/followup-limits`. */
export interface FollowupLimits {
	repo: string;
	daily_filed: number;
	daily_handoffs: number;
	max_attempts: number;
	recurrence_actions: number;
	recurrence_days: number;
}

export interface FollowupLimitsResponse {
	limits: FollowupLimits;
}

export class WrightApiError extends ApiError {
	constructor(
		message: string,
		status: number,
		url: string,
		readonly reason: string | null
	) {
		super(message, status, url);
		this.name = 'WrightApiError';
	}

	get isConflict(): boolean {
		return this.status === 409;
	}
}

/** The route is not in the running mike yet: its backend has not deployed. */
export function isNotDeployed(err: unknown): boolean {
	return err instanceof ApiError && err.status === 404;
}

export function isUnauthorised(err: unknown): boolean {
	return err instanceof ApiError && (err.status === 401 || err.status === 403);
}

interface RequestOptions {
	body?: unknown;
	/** Send the operator token when one is stored. */
	auth?: boolean;
}

async function request<T>(
	method: 'GET' | 'POST' | 'PATCH' | 'DELETE',
	path: string,
	options: RequestOptions = {}
): Promise<T> {
	const headers: Record<string, string> = { accept: 'application/json' };
	if (options.body !== undefined) headers['content-type'] = 'application/json';
	if (options.auth) {
		const token = readOperatorToken();
		if (token !== null) headers.authorization = `Bearer ${token}`;
	}
	const res = await fetch(path, {
		method,
		headers,
		body: options.body === undefined ? undefined : JSON.stringify(options.body)
	});
	if (!res.ok) {
		let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
		let reason: string | null = null;
		try {
			const body = (await res.json()) as { error?: unknown; reason?: unknown };
			if (typeof body.reason === 'string' && body.reason.length > 0) reason = body.reason;
			if (reason !== null) message = reason;
			else if (typeof body.error === 'string' && body.error.length > 0) message = body.error;
		} catch {
			// The status line stays the message when the body is not JSON.
		}
		throw new WrightApiError(message, res.status, path, reason);
	}
	if (res.status === 204) return undefined as T;
	return (await res.json()) as T;
}

function query(params: Record<string, string | number | null | undefined>): string {
	const search = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value !== null && value !== undefined && value !== '') search.set(key, String(value));
	}
	const text = search.toString();
	return text.length > 0 ? `?${text}` : '';
}

export interface ListProblemsParams {
	state?: ProblemState | 'all';
	check?: string;
	source?: string;
	limit?: number;
	offset?: number;
}

export async function listProblems(params: ListProblemsParams = {}): Promise<ProblemsPage> {
	const res = await request<Omit<ProblemsPage, 'problems'> & { problems: WireProblem[] }>(
		'GET',
		`/api/wright/problems${query({ ...params })}`
	);
	return { ...res, problems: res.problems.map(normaliseProblem) };
}

export async function getProblem(
	id: number,
	params: { episode_limit?: number; episode_offset?: number } = {}
): Promise<ProblemDetail> {
	const res = await request<Omit<ProblemDetail, 'problem'> & { problem: WireProblem }>(
		'GET',
		`/api/wright/problems/${id}${query(params)}`
	);
	return { ...res, problem: normaliseProblem(res.problem) };
}

export function listRemedies(): Promise<RemediesResponse> {
	return request('GET', '/api/wright/remedies');
}

export function patchRemedy(name: string, patch: RemedyPatch): Promise<WrightRemedy> {
	return request('PATCH', `/api/wright/remedies/${encodeURIComponent(name)}`, {
		body: patch,
		auth: true
	});
}

export interface PagedParams {
	problem_id?: number;
	limit?: number;
	before?: number | null;
}

export function listActions(params: PagedParams = {}): Promise<ActionsPage> {
	return request('GET', `/api/wright/actions${query({ ...params })}`);
}

/** 200 with the action, or a `WrightApiError` whose `isConflict` is true and `reason` says why. */
export function undoAction(id: number): Promise<WrightAction> {
	return request('POST', `/api/wright/actions/${id}/undo`, { body: {}, auth: true });
}

export function listInvestigations(params: PagedParams = {}): Promise<InvestigationsPage> {
	return request('GET', `/api/wright/investigations${query({ ...params })}`);
}

export function listFindings(params: PagedParams = {}): Promise<FindingsPage> {
	return request('GET', `/api/wright/findings${query({ ...params })}`);
}

export function getFinding(id: number): Promise<WrightFinding> {
	return request('GET', `/api/wright/findings/${id}`);
}

export function getEngineerLimits(): Promise<EngineerLimits> {
	return request('GET', '/api/wright/engineer-limits');
}

export function patchEngineerLimits(patch: EngineerLimits): Promise<EngineerLimits> {
	return request('PATCH', '/api/wright/engineer-limits', { body: patch, auth: true });
}

export function listRepairs(params: PagedParams = {}): Promise<RepairsPage> {
	return request('GET', `/api/wright/repairs${query({ ...params })}`);
}

export function getRepair(id: number): Promise<WrightRepair> {
	return request('GET', `/api/wright/repairs/${id}`);
}

export interface ListFollowupsParams {
	state?: FollowupState;
	limit?: number;
	before?: number | null;
}

export function listFollowups(params: ListFollowupsParams = {}): Promise<FollowupsPage> {
	return request('GET', `/api/wright/followups${query({ ...params })}`);
}

export function getFollowupLimits(): Promise<FollowupLimitsResponse> {
	return request('GET', '/api/wright/followup-limits');
}

/**
 * One cause of Wright's open problems, from `GET /api/wright/causes`: the problems that share it,
 * what has been done about it, and where it ranks. `problem_id` is the most-observed problem in
 * the cause, the one a row opens. Rows arrive sorted, highest `priority` first.
 */
export interface WrightCause {
	cause_id: number;
	problem_id: number;
	check_name: string;
	/** Can be long SQL text; for a query it is cut after 120 characters of the SQL. */
	resource: string;
	/** The whole SQL text of the cause's most-observed problem; null when it is not a statement. */
	statement: string | null;
	problems: number;
	observations: number;
	opened_at: string;
	attempts: number;
	last_attempt_at: string | null;
	finding_id: number | null;
	followup_id: number | null;
	issue_number: number | null;
	/** A known fix already answers the check. */
	remedied: boolean;
	priority: number;
	/** When the operator was told the cause had waited. */
	notified_at: string | null;
	commented_at: string | null;
	/** The operator pinned it: pinned causes come before every unpinned one. */
	pinned: boolean;
	pinned_at: string | null;
	/** Set when the operator moved the cause; replaces the formula's `priority` for the order. */
	operator_priority: number | null;
	/** The value the order uses: `operator_priority` when set, else `priority`. */
	rank_priority: number;
	group_id: number | null;
	group_name: string | null;
	/** Merges into this cause, newest first. */
	absorbed: AbsorbedCause[];
	/** The engineer's pick would consider this cause. `null` until the server says. */
	investigable: boolean | null;
	not_investigated_because: NotInvestigatedBecause | null;
}

export type NotInvestigatedBecause = 'finding' | 'followup' | 'known_fix' | 'attempts';

export interface AbsorbedCause {
	merge_id: number;
	cause_id: number;
	check_name: string;
	key: string;
	/** How many problems moved with the merge. */
	problems: number;
	by: 'operator' | 'engineer';
	created_at: string;
	/** An operator merge that is not separated yet. */
	can_separate: boolean;
}

export interface WrightGroup {
	id: number;
	name: string;
	/** How many causes are in the group. */
	causes: number;
}

export interface CausesResponse {
	causes: WrightCause[];
	limit: number;
	/** A cause that nothing has answered for this many hours is "waiting". */
	waited_hours: number;
	watch_enabled: boolean;
	groups: WrightGroup[];
	/** The engineer's attempt limit per cause; null until the server sends it. */
	max_attempts: number | null;
}

/** A cause row as the wire may send it: the operator fields are absent before their backend deploys. */
type WireCause = Omit<
	WrightCause,
	| 'statement'
	| 'pinned'
	| 'pinned_at'
	| 'operator_priority'
	| 'rank_priority'
	| 'group_id'
	| 'group_name'
	| 'absorbed'
	| 'investigable'
	| 'not_investigated_because'
> &
	Partial<WrightCause>;

/** Fills the operator fields a running mike that has not deployed them yet leaves out. */
export function normaliseCause(wire: WireCause): WrightCause {
	const operatorPriority = wire.operator_priority ?? null;
	return {
		...wire,
		statement: wire.statement ?? null,
		pinned: wire.pinned ?? false,
		pinned_at: wire.pinned_at ?? null,
		operator_priority: operatorPriority,
		rank_priority: wire.rank_priority ?? operatorPriority ?? wire.priority,
		group_id: wire.group_id ?? null,
		group_name: wire.group_name ?? null,
		absorbed: wire.absorbed ?? [],
		investigable: wire.investigable ?? null,
		not_investigated_because: wire.not_investigated_because ?? null
	};
}

export async function listCauses(params: { limit?: number } = {}): Promise<CausesResponse> {
	const res = await request<
		Omit<CausesResponse, 'causes' | 'groups' | 'max_attempts'> & {
			causes: WireCause[];
			groups?: WrightGroup[];
			max_attempts?: number | null;
		}
	>('GET', `/api/wright/causes${query({ ...params })}`);
	return {
		...res,
		causes: res.causes.map(normaliseCause),
		groups: res.groups ?? [],
		max_attempts: res.max_attempts ?? null
	};
}

/** What the operator may change on a cause; at least one field. `null` clears the value. */
export interface CausePatch {
	pinned?: boolean;
	operator_priority?: number | null;
	group_id?: number | null;
}

/** Needs the operator token. The answer is the cause as the list shows it. */
export async function patchCause(id: number, patch: CausePatch): Promise<WrightCause> {
	const res = await request<{ cause: WireCause }>('PATCH', `/api/wright/causes/${id}`, {
		body: patch,
		auth: true
	});
	return normaliseCause(res.cause);
}

/** The merge answer's `causes` and `problems` are not read: the page loads the queue again. */
export interface MergeResult {
	causes: unknown;
	problems: unknown;
	merge_ids: number[];
}

/** Needs the operator token. Absorbs `causeIds` into the cause `rootId`. */
export function mergeCauses(rootId: number, causeIds: number[]): Promise<MergeResult> {
	return request('POST', `/api/wright/causes/${rootId}/merge`, {
		body: { cause_ids: causeIds },
		auth: true
	});
}

/** Needs the operator token. A 409 with a `reason` for an engineer merge or one already separated. */
export function separateMerge(mergeId: number): Promise<{ merge: unknown }> {
	return request('POST', `/api/wright/cause-merges/${mergeId}/separate`, { body: {}, auth: true });
}

/** Every open problem of the cause, most observed first. */
export async function listCauseProblems(id: number): Promise<WrightProblem[]> {
	const res = await request<{ problems: WireProblem[] }>(
		'GET',
		`/api/wright/causes/${id}/problems`
	);
	return res.problems.map(normaliseProblem);
}

export async function listCauseGroups(): Promise<WrightGroup[]> {
	const res = await request<{ groups: WrightGroup[] }>('GET', '/api/wright/cause-groups');
	return res.groups;
}

/** Needs the operator token. A 409 means the name is taken. */
export async function createCauseGroup(name: string): Promise<WrightGroup> {
	const res = await request<{ group: WrightGroup }>('POST', '/api/wright/cause-groups', {
		body: { name },
		auth: true
	});
	return res.group;
}

export async function renameCauseGroup(id: number, name: string): Promise<WrightGroup> {
	const res = await request<{ group: WrightGroup }>('PATCH', `/api/wright/cause-groups/${id}`, {
		body: { name },
		auth: true
	});
	return res.group;
}

/** The group's causes lose the group. */
export function deleteCauseGroup(id: number): Promise<void> {
	return request('DELETE', `/api/wright/cause-groups/${id}`, { auth: true });
}

export function listMessages(
	params: { after?: number | null; limit?: number } = {}
): Promise<MessagesPage> {
	return request('GET', `/api/wright/messages${query({ ...params })}`);
}

export interface PostMessageBody {
	client_msg_id: string;
	body: string;
	reply_to?: number;
}

/** Needs the operator token. The reply to it arrives through `listMessages`. */
export async function postMessage(body: PostMessageBody): Promise<WrightMessage | null> {
	const res = await request<WrightMessage | { message?: WrightMessage }>(
		'POST',
		'/api/wright/messages',
		{ body, auth: true }
	);
	if ('message' in res) return res.message ?? null;
	return 'id' in res ? res : null;
}

/** Needs the operator token. `version` is the steer's version, so a stale undo gets a 409. */
export function undoSteer(id: number, version: number): Promise<WrightSteer> {
	return request('POST', `/api/wright/steers/${id}/undo`, { body: { version }, auth: true });
}
