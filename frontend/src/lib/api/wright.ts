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
	resource: string;
	state: ProblemState;
	opened_at: string;
	last_seen_at: string;
	healthy_at: string | null;
	episodes: number;
	latest_evidence: Record<string, unknown>;
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

export type RemedyKind = 'tidy' | 'setting' | 'notice';

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
	'editing' | 'prepared' | 'gating' | 'gate_failed' | 'refused' | 'landed' | 'deployed' | 'healthy';

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
	method: 'GET' | 'POST' | 'PATCH',
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

export function listProblems(params: ListProblemsParams = {}): Promise<ProblemsPage> {
	return request('GET', `/api/wright/problems${query({ ...params })}`);
}

export function getProblem(
	id: number,
	params: { episode_limit?: number; episode_offset?: number } = {}
): Promise<ProblemDetail> {
	return request('GET', `/api/wright/problems/${id}${query(params)}`);
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
