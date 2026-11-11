import { ApiError } from '$lib/api';

/**
 * Client for the Threads page: mike's work threads, per project. Wire shapes follow
 * crates/mike/src/api/work_threads/mod.rs and crates/mike/src/store/persistence/work_threads/.
 */

export const THREAD_STATUSES = ['active', 'implementing', 'review', 'parked', 'done'] as const;
export type ThreadStatus = (typeof THREAD_STATUSES)[number];

export const ENTRY_KINDS = [
	'checkpoint',
	'decision',
	'finding',
	'brief',
	'handoff',
	'note'
] as const;
export type EntryKind = (typeof ENTRY_KINDS)[number];

export interface ThreadListRow {
	slug: string;
	title: string | null;
	status: string | null;
	tldr: string;
	next_up: string;
	parse_error: string | null;
	updated_at: string;
}

export interface ThreadListResponse {
	threads: ThreadListRow[];
	total: number;
}

export interface ThreadIdentity {
	id: number;
	slug: string;
	title: string | null;
	goal: string | null;
	status: string | null;
	branch: string | null;
	worktree: string | null;
	source_path: string | null;
	parse_error: string | null;
	last_session_uuid: string | null;
	created_at: string;
	updated_at: string;
}

export interface ThreadState {
	tldr: string;
	where_we_are: string;
	next_up: string;
	blockers: string;
	open_questions: string;
	decisions: string;
	files: string[];
	updated_at: string;
}

export interface ThreadEntry {
	ord: number;
	kind: string;
	summary: string | null;
	body: string;
	session_uuid: string | null;
	commit_sha: string | null;
	data: unknown;
	entry_date: string;
	created_at: string;
}

export interface ThreadSessionLink {
	session_uuid: string;
	contribution: string;
	first_seen_at: string;
	last_seen_at: string;
}

export interface ThreadDetail {
	thread: ThreadIdentity;
	state: ThreadState | null;
	acceptance_criteria: string | null;
	depends_on: string[];
	depended_on_by: string[];
	history: ThreadEntry[];
	history_total: number;
	history_next_before_ord: number | null;
	sessions: ThreadSessionLink[];
}

export interface ThreadMetaPatch {
	title?: string;
	goal?: string;
	status?: string;
	branch?: string;
	worktree?: string;
	depends_on?: string[];
	acceptance_criteria?: string;
}

export interface ThreadCheckpointInput {
	slug: string;
	tldr: string;
	where_we_are: string;
	next_up: string;
	blockers: string;
	open_questions: string;
	decisions: string;
	files: string[];
	contribution?: string;
}

export interface ThreadEntryInput {
	kind: string;
	body: string;
	summary?: string;
}

const HISTORY_PAGE = 500;

async function send<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(path, {
		method,
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const parsed = (await res.json()) as { error?: string };
			if (parsed.error) message = parsed.error;
		} catch {
			message = `${method} ${path} failed: ${res.status}`;
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as T;
}

export function listThreads(project: string, status?: string): Promise<ThreadListResponse> {
	const query = new URLSearchParams({ project, limit: '500' });
	if (status) query.set('status', status);
	return send('GET', `/api/work-threads?${query}`);
}

/** One thread with its whole history: pages of 500 entries are fetched until none remain. */
export async function getThread(project: string, slug: string): Promise<ThreadDetail> {
	const path = `/api/work-threads/${encodeURIComponent(slug)}`;
	const first = await send<ThreadDetail>(
		'GET',
		`${path}?${new URLSearchParams({ project, history_limit: String(HISTORY_PAGE) })}`
	);
	let next = first.history_next_before_ord;
	const history = [...first.history];
	while (next !== null) {
		const page = await send<ThreadDetail>(
			'GET',
			`${path}?${new URLSearchParams({
				project,
				history_limit: String(HISTORY_PAGE),
				history_before: String(next)
			})}`
		);
		history.push(...page.history);
		next = page.history_next_before_ord;
	}
	return { ...first, history, history_next_before_ord: null };
}

export function patchThread(
	project: string,
	slug: string,
	patch: ThreadMetaPatch
): Promise<ThreadIdentity> {
	return send('PATCH', `/api/work-threads/${encodeURIComponent(slug)}`, { project, ...patch });
}

export function checkpointThread(project: string, input: ThreadCheckpointInput): Promise<unknown> {
	return send('POST', '/api/work-threads', { project, ...input });
}

export function addThreadEntry(
	project: string,
	slug: string,
	input: ThreadEntryInput
): Promise<unknown> {
	return send('POST', `/api/work-threads/${encodeURIComponent(slug)}/entries`, {
		project,
		...input
	});
}
