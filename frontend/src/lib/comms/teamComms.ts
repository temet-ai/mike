import { ApiError } from '$lib/api';

export function isTeamCommsUnavailable(err: unknown): boolean {
	return err instanceof ApiError && (err.status === 404 || err.status === 405);
}

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
			if (typeof human === 'string' && human.length > 0) {
				message = human;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

export interface OpenThreadBody {
	subject: string;
	body: string;
	from_session_id: string | null;
	project_path: string | null;
	tags: string[];
	notify: string[];
}

export interface OpenThreadResponse {
	thread_uuid: string;
	subject: string;
}

export function postOpenThread(body: OpenThreadBody): Promise<OpenThreadResponse> {
	return postJson<OpenThreadBody, OpenThreadResponse>('/api/threads', body);
}

export interface TeamDispatchDto {
	agent_label: string;
	description: string;
	first_seen: string;
	last_seen: string;
	request_count: number;
}

export interface TeamMessageDto {
	ts: string;
	direction: 'sent' | 'received';
	counterpart: string;
	summary: string | null;
	body: string;
}

export interface TeamCommsResponse {
	dispatches: TeamDispatchDto[];
	messages: TeamMessageDto[];
}

export function getTeamComms(sessionId: string): Promise<TeamCommsResponse> {
	return getJson<TeamCommsResponse>(`/api/sessions/${encodeURIComponent(sessionId)}/team-comms`);
}
