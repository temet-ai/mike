import { ApiError } from '$lib/api';

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as T;
}

async function postJson<Req, Res>(path: string, body: Req): Promise<Res> {
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string; message?: string };
			const human = errBody?.message ?? errBody?.error;
			if (typeof human === 'string' && human.length > 0) message = human;
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

async function patchJson<Req, Res>(path: string, body: Req): Promise<Res> {
	const res = await fetch(path, {
		method: 'PATCH',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `PATCH ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string; message?: string };
			const human = errBody?.message ?? errBody?.error;
			if (typeof human === 'string' && human.length > 0) message = human;
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

export type SessionNoteKind = 'note' | 'finding' | 'task' | 'decision' | 'question' | string;
export type SessionNoteTaskStatus = 'open' | 'in_progress' | 'blocked' | 'done' | string;

export const NOTE_KIND_OPTIONS: readonly SessionNoteKind[] = [
	'note',
	'finding',
	'task',
	'decision',
	'question'
];

export const NOTE_TASK_STATUS_OPTIONS: readonly SessionNoteTaskStatus[] = [
	'open',
	'in_progress',
	'blocked',
	'done'
];

export interface SessionNoteDto {
	note_uuid: string;
	kind: string;
	title: string;
	body: string;
	status: string | null;
	tags: string[];
	data: unknown;
	created_at: string;
	updated_at: string;
}

export interface ProjectSessionNoteDto {
	session_uuid: string;
	session_name: string | null;
	session_role: string | null;
	note_uuid: string;
	kind: string;
	title: string;
	body: string;
	status: string | null;
	tags: string[];
	data: unknown;
	created_at: string;
	updated_at: string;
}

export interface CreateSessionNoteBody {
	kind?: string;
	title: string;
	body?: string;
	status?: string;
	tags?: string[];
	data?: unknown;
}

export interface UpdateSessionNoteBody {
	kind?: string;
	title?: string;
	body?: string;
	status?: string;
	tags?: string[];
	data?: unknown;
}

export interface SessionNotesQuery {
	kind?: string;
	status?: string;
	tag?: string;
	q?: string;
	order?: 'asc' | 'desc';
	limit?: number;
	offset?: number;
}

export interface ProjectNotesQuery extends SessionNotesQuery {
	role?: string;
	name?: string;
	session_uuid?: string;
}

export interface ProjectNotesResponse {
	project_id: number;
	project_path: string;
	total_count: number;
	offset: number;
	limit: number;
	notes: ProjectSessionNoteDto[];
}

function buildQuery(params: object): string {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params) as [string, string | number | undefined][]) {
		if (value === undefined || value === '') continue;
		query.set(key, String(value));
	}
	const s = query.toString();
	return s.length > 0 ? `?${s}` : '';
}

export function getSessionNotes(
	sessionId: string,
	params: SessionNotesQuery = {}
): Promise<SessionNoteDto[]> {
	const query = buildQuery(params);
	return getJson<SessionNoteDto[]>(`/api/sessions/${encodeURIComponent(sessionId)}/notes${query}`);
}

export function createSessionNote(
	sessionId: string,
	body: CreateSessionNoteBody
): Promise<SessionNoteDto> {
	return postJson<CreateSessionNoteBody, SessionNoteDto>(
		`/api/sessions/${encodeURIComponent(sessionId)}/notes`,
		body
	);
}

export function updateSessionNote(
	sessionId: string,
	noteUuid: string,
	body: UpdateSessionNoteBody
): Promise<SessionNoteDto> {
	return patchJson<UpdateSessionNoteBody, SessionNoteDto>(
		`/api/sessions/${encodeURIComponent(sessionId)}/notes/${encodeURIComponent(noteUuid)}`,
		body
	);
}

export function getProjectNotes(
	projectPath: string,
	params: ProjectNotesQuery = {}
): Promise<ProjectNotesResponse> {
	const query = buildQuery(params);
	return getJson<ProjectNotesResponse>(
		`/api/workspace/projects/${encodeURIComponent(projectPath)}/notes${query}`
	);
}
