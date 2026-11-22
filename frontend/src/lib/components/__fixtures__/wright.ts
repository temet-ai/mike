import type {
	FollowupLimits,
	ProblemDetail,
	WrightAction,
	WrightCause,
	WrightEpisode,
	WrightFinding,
	WrightFollowup,
	WrightInvestigation,
	WrightMessage,
	WrightProblem,
	WrightRemedy,
	WrightRepair
} from '$lib/api/wright';
import type { ProblemDetailState } from '$lib/wright/store.svelte';
import { emptyFeed, type Feed } from '$lib/wright/feed';

/** Fixtures in the shapes of artifacts/2026-11-14/wright/BUILD_PLAN.md and api/wright/problems.rs. */

export const ago = (minutes: number): string =>
	new Date(Date.now() - minutes * 60_000).toISOString();

const SESSION = '3dcd7aa2-3126-4122-84e0-051a00751c02';

export function problemFixture(over: Partial<WrightProblem> = {}): WrightProblem {
	return {
		id: 707,
		check_name: 'tool_contract_errors',
		scope: 'tool_contract',
		source: 'mcp_tools',
		resource:
			'mcp__tether__work_thread_checkpoint: <tool_use_error>work_thread_checkpoint needs `tldr`</tool_use_error>',
		state: 'open',
		opened_at: ago(420),
		last_seen_at: ago(3),
		healthy_at: null,
		episodes: 1,
		latest_evidence: {
			error_kind: '<tool_use_error>work_thread_checkpoint needs `tldr`</tool_use_error>',
			failures: 3,
			last_request_id: 799713,
			sample_error: '<tool_use_error>work_thread_checkpoint needs `tldr`</tool_use_error>',
			sessions: 1,
			tool: 'mcp__tether__work_thread_checkpoint',
			window: '168 hours'
		},
		...over
	};
}

export const WAITED_HOURS = 6;

/** The long SQL a slow-query cause carries as its resource, as the real API returns it. */
export const LONG_RESOURCE =
	'query 6512464039861100498: WITH own_turn AS ( SELECT max(t.id) AS id FROM memory.operator_turns t WHERE t.session_id = $1 AND t.kind = $2 ), prior AS ( SELECT p.id, p.text FROM memory.operator_turns p JOIN own_turn o ON p.id < o.id WHERE p.session_id = $1 ORDER BY p.id DESC LIMIT 20 ) SELECT * FROM prior';

export function causeFixture(over: Partial<WrightCause> = {}): WrightCause {
	return {
		cause_id: 10,
		problem_id: 707,
		check_name: 'decisions_health',
		resource: 'blocked_dialog',
		problems: 16,
		observations: 16,
		opened_at: ago(60 * 30),
		attempts: 0,
		last_attempt_at: null,
		finding_id: null,
		followup_id: null,
		issue_number: null,
		remedied: false,
		priority: 90,
		notified_at: null,
		commented_at: null,
		...over
	};
}

/** One cause in each state, in the server's order (highest priority first). */
export function causes(): WrightCause[] {
	return [
		causeFixture(),
		causeFixture({
			cause_id: 18,
			problem_id: 703,
			check_name: 'slow_queries',
			resource: LONG_RESOURCE,
			problems: 7,
			observations: 1733,
			opened_at: ago(60 * 52),
			attempts: 2,
			last_attempt_at: ago(60 * 5),
			finding_id: 5,
			priority: 80,
			notified_at: ago(60 * 40)
		}),
		causeFixture({
			cause_id: 21,
			problem_id: 707,
			check_name: 'tool_contract_errors',
			resource: 'mcp__tether__work_thread_checkpoint',
			problems: 3,
			observations: 41,
			opened_at: ago(60 * 9),
			attempts: 1,
			last_attempt_at: ago(60 * 3),
			followup_id: 14,
			issue_number: 272,
			priority: 60
		}),
		causeFixture({
			cause_id: 22,
			problem_id: 703,
			check_name: 'undelivered_mail',
			resource: '3dcd7aa2-3126-4122-84e0-051a00751c02',
			problems: 1,
			observations: 6,
			opened_at: ago(60 * 3),
			remedied: true,
			priority: 40
		}),
		causeFixture({
			cause_id: 23,
			problem_id: 707,
			check_name: 'log_warnings',
			resource: 'mike.log: connection reset by peer',
			problems: 1,
			observations: 2,
			opened_at: ago(25),
			priority: 10
		})
	];
}

export function openProblems(): WrightProblem[] {
	return [
		problemFixture(),
		problemFixture({
			id: 703,
			check_name: 'undelivered_mail',
			scope: 'mail',
			source: 'capture_db',
			resource: SESSION,
			opened_at: ago(60 * 26),
			latest_evidence: {
				message_uuids: ['c2a1f0d4-7b1e-4c8a-9a07-0e3c5d2b1a99'],
				oldest_queued_at: ago(60 * 25),
				recipient_last_seen: ago(60 * 25),
				undelivered: 1
			}
		})
	];
}

export function healthyProblems(): WrightProblem[] {
	return [
		problemFixture({
			id: 72,
			check_name: 'mcp_tool_errors',
			scope: 'mcp_tool',
			resource: 'mcp__mike__fleet_report: fleet_report is incomplete: `sha` is missing',
			state: 'healthy',
			opened_at: ago(60 * 48),
			last_seen_at: ago(60 * 30),
			healthy_at: ago(60 * 20),
			latest_evidence: {
				error_kind: 'fleet_report is incomplete: `sha` is missing',
				failures: 3,
				last_request_id: 780680,
				sessions: 3,
				tool: 'mcp__mike__fleet_report',
				window: '24 hours'
			}
		})
	];
}

export function actionFixture(over: Partial<WrightAction> = {}): WrightAction {
	return {
		id: 41,
		remedy: 'tell_undelivered_senders',
		problem_id: 703,
		state: 'verified',
		by: 'wright',
		created_at: ago(90),
		verified_at: ago(88),
		effect: { notified: 'coordinator-e74ba661', messages: 1 },
		can_undo: false,
		...over
	};
}

export function actions(): WrightAction[] {
	return [
		actionFixture({
			id: 43,
			remedy: 'tell_undelivered_senders',
			problem_id: 707,
			state: 'applied',
			verified_at: null,
			can_undo: true,
			created_at: ago(12),
			effect: { notified: 'coordinator-e74ba661', messages: 1 }
		}),
		actionFixture({ id: 41 }),
		actionFixture({
			id: 38,
			remedy: 'tell_undelivered_senders',
			problem_id: 72,
			state: 'ineffective',
			can_undo: false,
			created_at: ago(60 * 30),
			verified_at: null,
			effect: null
		})
	];
}

export function remedies(): WrightRemedy[] {
	return [
		{
			name: 'tell_undelivered_senders',
			kind: 'notice',
			action: 'tell_undelivered_senders',
			check_name: 'undelivered_mail',
			enabled: true,
			daily_limit: 5,
			max_resources: 3,
			paused_at: ago(30),
			pause_reason: 'Paused at your request until the fleet closes.',
			actions_today: 2,
			ineffective_runs: 1
		}
	];
}

export function messageFixture(over: Partial<WrightMessage> = {}): WrightMessage {
	return {
		id: 1,
		author: 'operator',
		body: 'Why did you tell the coordinator about that mail?',
		reply_to: null,
		links: [],
		created_at: ago(40),
		steer: null,
		...over
	};
}

export function messages(): WrightMessage[] {
	return [
		messageFixture(),
		messageFixture({
			id: 2,
			author: 'wright',
			reply_to: 1,
			created_at: ago(39),
			body: 'The recipient had not read it for a day, so I sent the sender a notice with the recipient id.\n\nThe mail itself stays queued.',
			links: [
				{ kind: 'problem', id: 703 },
				{ kind: 'action', id: 41 }
			]
		}),
		messageFixture({
			id: 3,
			created_at: ago(8),
			body: 'Leave the sender notices alone until the fleet closes.'
		}),
		messageFixture({
			id: 4,
			author: 'wright',
			reply_to: 3,
			created_at: ago(7),
			body: 'Paused. It stays off until the fleet closes.',
			steer: {
				id: 9,
				change: { type: 'pause_remedy', name: 'tell_undelivered_senders', until: 'the fleet closes' },
				applied_at: ago(7),
				undone_at: null,
				can_undo: true,
				version: 1
			}
		})
	];
}

export function episodeFixture(over: Partial<WrightEpisode> = {}): WrightEpisode {
	return {
		id: 1,
		opened_at: ago(420),
		closed_at: null,
		last_seen_at: ago(3),
		opened_by_run: 4931,
		closed_by_run: null,
		last_seen_run: 5013,
		observations: 9,
		evidence: { failures: 3, sessions: 1, window: '168 hours' },
		...over
	};
}

export function findingFixture(): WrightFinding {
	return {
		id: 5,
		investigation_id: 3,
		problem_id: 707,
		symptom: 'work_thread_checkpoint is called without a tldr',
		suspected_cause: 'The tool description does not say tldr is required.',
		confirmed_cause: 'The tool schema marks tldr optional while the handler rejects an empty one.',
		cause_established: true,
		owning_part: 'crates/mike-mcp/src/tools/work_threads.rs',
		kind: 'code_defect',
		remedy: 'Mark tldr required in the schema.',
		evidence: [
			{ kind: 'request', ref: '799713' },
			{ kind: 'session', ref: SESSION },
			{ kind: 'file', ref: 'crates/mike-mcp/src/tools/work_threads.rs' }
		],
		created_at: ago(300)
	};
}

export function repairFixture(): WrightRepair {
	return {
		id: 2,
		finding_id: 5,
		problem_id: 707,
		state: 'landed',
		branch: 'wright/repair-5',
		pr_url: 'https://github.com/temet-ai/mike/pull/301',
		merge_sha: 'afc42e23c4d1',
		reason: null,
		created_at: ago(280),
		updated_at: ago(120)
	};
}

export function investigationFixture(): WrightInvestigation {
	return {
		id: 3,
		problem_id: 707,
		state: 'finished',
		session_uuid: SESSION,
		started_at: ago(330),
		ended_at: ago(300),
		end_reason: null
	};
}

export function followupLimitsFixture(over: Partial<FollowupLimits> = {}): FollowupLimits {
	return {
		repo: 'temet-ai/mike',
		daily_filed: 3,
		daily_handoffs: 2,
		max_attempts: 3,
		recurrence_actions: 3,
		recurrence_days: 7,
		...over
	};
}

export function followupFixture(over: Partial<WrightFollowup> = {}): WrightFollowup {
	return {
		id: 12,
		kind: 'code_defect',
		check_name: 'tool_contract_errors',
		owning_part: 'crates/mike-mcp/src/tools/work_threads.rs',
		title: 'work_thread_checkpoint accepts a call without tldr and then rejects it',
		body: 'The tool schema marks tldr optional while the handler rejects an empty one.\n\nMake the schema say tldr is required, so the model is told before it calls.',
		state: 'filed',
		issue_number: 269,
		issue_url: 'https://github.com/temet-ai/mike/issues/269',
		attempts: 0,
		fixer_session: null,
		fixer_machine: null,
		filed_at: ago(200),
		handed_off_at: null,
		closed_at: null,
		closed_reason: null,
		created_at: ago(260),
		updated_at: ago(200),
		findings: [5],
		problems: [707],
		...over
	};
}

/** One follow-up in each state, newest first, as the API lists them. */
export function followups(): WrightFollowup[] {
	const session = '9b1c2d3e-4f50-4a61-8b72-0c93d4e5f6a7';
	return [
		followupFixture({
			id: 16,
			state: 'stuck',
			title: 'Mail to a seat that never reads is retried forever',
			kind: 'root_cause',
			check_name: 'undelivered_mail',
			owning_part: 'crates/mike-launch/src/mailbox',
			issue_number: 277,
			issue_url: 'https://github.com/temet-ai/mike/issues/277',
			attempts: 3,
			fixer_session: session,
			fixer_machine: 'omit',
			filed_at: ago(60 * 30),
			handed_off_at: ago(60 * 20),
			problems: [703]
		}),
		followupFixture({
			id: 15,
			state: 'handed_off',
			title: 'Raise the slow-query threshold for the capture schema',
			kind: 'check_tuning',
			check_name: 'slow_queries',
			owning_part: null,
			issue_number: 275,
			issue_url: 'https://github.com/temet-ai/mike/issues/275',
			attempts: 1,
			fixer_session: session,
			fixer_machine: 'omit',
			handed_off_at: ago(45),
			problems: []
		}),
		followupFixture({
			id: 14,
			issue_number: 272,
			issue_url: 'https://github.com/temet-ai/mike/issues/272'
		}),
		followupFixture({
			id: 13,
			state: 'open',
			title: 'Hooks that fail on an empty tool list should say so',
			kind: 'hardening',
			check_name: 'hook_errors',
			issue_number: null,
			issue_url: null,
			filed_at: null,
			problems: []
		}),
		followupFixture({
			id: 12,
			state: 'fixed',
			attempts: 1,
			fixer_session: session,
			fixer_machine: 'omit',
			handed_off_at: ago(60 * 10),
			closed_at: ago(60 * 8),
			closed_reason: 'merged'
		}),
		followupFixture({
			id: 11,
			state: 'dropped',
			title: 'Warn when a log warning repeats',
			check_name: 'log_warnings',
			closed_at: ago(60 * 40),
			closed_reason: 'not_planned',
			problems: []
		})
	];
}

function loaded<T>(rows: T[], cursor: number | null = null): Feed<T> {
	return { ...emptyFeed<T>(), rows, status: 'ready', cursor, total: rows.length };
}

export function detailFixture(): ProblemDetailState {
	return {
		episodes: loaded([episodeFixture()]),
		actions: loaded(actions().slice(0, 1)),
		findings: loaded([findingFixture()]),
		repairs: loaded([repairFixture()]),
		investigations: loaded([investigationFixture()])
	};
}

export function problemDetailResponse(problem: WrightProblem): ProblemDetail {
	return {
		problem,
		episodes: [episodeFixture({ evidence: problem.latest_evidence })],
		episode_limit: 50,
		episode_offset: 0,
		next_episode_offset: null
	};
}

export interface WrightScenario {
	open: WrightProblem[];
	healthy: WrightProblem[];
	/** null: the route answers 404, as before its backend deploys. */
	actions: WrightAction[] | null;
	remedies: WrightRemedy[] | null;
	messages: WrightMessage[] | null;
	pending: boolean;
	findings: WrightFinding[] | null;
	repairs: WrightRepair[] | null;
	investigations: WrightInvestigation[] | null;
	/** null: the route answers 404, as before its backend deploys. */
	followups: WrightFollowup[] | null;
	followupLimits: FollowupLimits;
	/** null: the route answers 404, as before its backend deploys. */
	causes: WrightCause[] | null;
	/** What undoing an action answers: 200, or the 409 conflict. */
	undo: 'ok' | 'conflict';
	watchEnabled: boolean;
}

export function normalScenario(): WrightScenario {
	return {
		open: openProblems(),
		healthy: healthyProblems(),
		actions: actions(),
		remedies: remedies(),
		messages: messages(),
		pending: false,
		findings: [findingFixture()],
		repairs: [repairFixture()],
		investigations: [investigationFixture()],
		followups: followups(),
		followupLimits: followupLimitsFixture(),
		causes: causes(),
		undo: 'ok',
		watchEnabled: true
	};
}

export function emptyScenario(): WrightScenario {
	return {
		...normalScenario(),
		open: [],
		healthy: [],
		actions: [],
		remedies: [],
		messages: [],
		findings: [],
		repairs: [],
		investigations: [],
		followups: [],
		causes: []
	};
}

export const CONFLICT_REASON =
	'The launch was closed by someone else after Wright closed it, so it was left alone.';

function json(status: number, body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/** Stubs `fetch` for the Wright routes in the shapes the plan gives; posted bodies are returned. */
export function installWrightFetch(scenario: WrightScenario): { posted: unknown[] } {
	const state = {
		...scenario,
		messages: scenario.messages === null ? null : [...scenario.messages],
		actions: scenario.actions === null ? null : [...scenario.actions]
	};
	const posted: unknown[] = [];

	function forProblem<T>(
		rows: T[] | null,
		key: (row: T) => number,
		params: URLSearchParams
	): T[] | null {
		if (rows === null) return null;
		const problemId = params.get('problem_id');
		return rows.filter((row) => problemId === null || key(row) === Number(problemId));
	}

	globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = new URL(typeof input === 'string' ? input : input.toString(), 'http://localhost');
		const method = init?.method ?? 'GET';
		const path = url.pathname;
		const params = url.searchParams;
		if (method === 'GET' && path === '/api/wright/problems') {
			const rows = params.get('state') === 'healthy' ? state.healthy : state.open;
			const offset = Number(params.get('offset') ?? 0);
			const limit = Number(params.get('limit') ?? 50);
			const slice = rows.slice(offset, offset + limit);
			const next = offset + slice.length < rows.length ? offset + slice.length : null;
			return json(200, {
				problems: slice,
				total: rows.length,
				limit,
				offset,
				next_offset: next,
				watch_enabled: state.watchEnabled
			});
		}
		if (method === 'GET' && path === '/api/wright/causes') {
			if (state.causes === null) return json(404, {});
			const limit = Number(params.get('limit') ?? 50);
			return json(200, {
				causes: state.causes.slice(0, limit),
				limit,
				waited_hours: WAITED_HOURS,
				watch_enabled: state.watchEnabled
			});
		}
		if (method === 'GET' && path === '/api/wright/followups') {
			if (state.followups === null) return json(404, {});
			const only = params.get('state');
			const before = Number(params.get('before') ?? Number.MAX_SAFE_INTEGER);
			const limit = Number(params.get('limit') ?? 50);
			const rows = state.followups.filter(
				(f) => (only === null || f.state === only) && f.id < before
			);
			const page = rows.filter((_, index) => index < limit);
			const next = rows.length > limit ? (page.at(-1)?.id ?? null) : null;
			return json(200, { followups: page, next_before: next });
		}
		if (method === 'GET' && path === '/api/wright/followup-limits') {
			return json(200, { limits: state.followupLimits, ranges: {} });
		}
		const one = /^\/api\/wright\/problems\/(\d+)$/.exec(path);
		if (method === 'GET' && one) {
			const found = [...state.open, ...state.healthy].find((p) => p.id === Number(one[1]));
			return found
				? json(200, problemDetailResponse(found))
				: json(404, { error: 'no such problem' });
		}
		if (method === 'GET' && path === '/api/wright/remedies') {
			return state.remedies === null ? json(404, {}) : json(200, { remedies: state.remedies });
		}
		if (method === 'GET' && path === '/api/wright/actions') {
			const rows = forProblem(state.actions, (a) => a.problem_id, params);
			return rows === null ? json(404, {}) : json(200, { actions: rows, next_before: null });
		}
		if (method === 'GET' && path === '/api/wright/findings') {
			const rows = forProblem(state.findings, (f) => f.problem_id, params);
			return rows === null ? json(404, {}) : json(200, { findings: rows, next_before: null });
		}
		if (method === 'GET' && path === '/api/wright/repairs') {
			const rows = forProblem(state.repairs, (r) => r.problem_id, params);
			return rows === null ? json(404, {}) : json(200, { repairs: rows, next_before: null });
		}
		if (method === 'GET' && path === '/api/wright/investigations') {
			const rows = forProblem(state.investigations, (i) => i.problem_id ?? -1, params);
			return rows === null ? json(404, {}) : json(200, { investigations: rows, next_before: null });
		}
		if (method === 'GET' && path === '/api/wright/messages') {
			if (state.messages === null) return json(404, {});
			const after = Number(params.get('after') ?? 0);
			return json(200, {
				messages: state.messages.filter((m) => m.id > after),
				pending_reply: state.pending
			});
		}
		if (method === 'POST' && path === '/api/wright/messages') {
			const body = JSON.parse(String(init?.body ?? '{}')) as { body: string };
			posted.push({ ...body, authorization: new Headers(init?.headers).get('authorization') });
			const stored = messageFixture({
				id: (state.messages?.at(-1)?.id ?? 0) + 1,
				body: body.body,
				created_at: new Date().toISOString()
			});
			state.messages = [...(state.messages ?? []), stored];
			state.pending = true;
			return json(200, stored);
		}
		const undoAction = /^\/api\/wright\/actions\/(\d+)\/undo$/.exec(path);
		if (method === 'POST' && undoAction) {
			if (state.undo === 'conflict')
				return json(409, { error: 'conflict', reason: CONFLICT_REASON });
			const action = state.actions?.find((a) => a.id === Number(undoAction[1]));
			return action ? json(200, { ...action, state: 'undone', can_undo: false }) : json(404, {});
		}
		if (method === 'POST' && /^\/api\/wright\/steers\/\d+\/undo$/.test(path)) {
			if (state.messages !== null) {
				state.messages = state.messages.map((m) =>
					m.steer === null
						? m
						: { ...m, steer: { ...m.steer, undone_at: new Date().toISOString(), can_undo: false } }
				);
			}
			return json(200, {});
		}
		return json(404, { error: 'not stubbed' });
	}) as typeof fetch;
	return { posted };
}
