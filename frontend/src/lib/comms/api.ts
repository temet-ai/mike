import { ApiError } from '$lib/api';
import type { SendMailboxMessageResponse } from '$lib/api/mailbox';
import type {
	NewThreadMessageBody,
	SessionFleetDto,
	SessionThreadsResponse,
	TagsResponse,
	ThreadDetailResponse,
	ThreadMessagePostResponse,
	ThreadReplyBody,
	ThreadResolveResponse,
	ThreadsListResponse
} from './types';

export function isCommsUnavailable(err: unknown): boolean {
	return err instanceof ApiError && err.status === 404;
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

export function getSessionThreads(sessionId: string): Promise<SessionThreadsResponse> {
	return getJson<SessionThreadsResponse>(`/api/sessions/${encodeURIComponent(sessionId)}/threads`);
}

export function getThread(threadUuid: string): Promise<ThreadDetailResponse> {
	return getJson<ThreadDetailResponse>(`/api/threads/${encodeURIComponent(threadUuid)}`);
}

export function postNewThreadMessage(
	sessionId: string,
	body: NewThreadMessageBody
): Promise<SendMailboxMessageResponse> {
	return postJson<NewThreadMessageBody, SendMailboxMessageResponse>(
		`/api/sessions/${encodeURIComponent(sessionId)}/message`,
		body
	);
}

export function postThreadMessage(
	threadUuid: string,
	body: ThreadReplyBody
): Promise<ThreadMessagePostResponse> {
	return postJson<ThreadReplyBody, ThreadMessagePostResponse>(
		`/api/threads/${encodeURIComponent(threadUuid)}/message`,
		body
	);
}

export function resolveThread(threadUuid: string): Promise<ThreadResolveResponse> {
	return postJson<Record<string, never>, ThreadResolveResponse>(
		`/api/threads/${encodeURIComponent(threadUuid)}/resolve`,
		{}
	);
}

export function listThreads(
	params: { project?: string; tag?: string; participant?: string; resolved?: boolean } = {}
): Promise<ThreadsListResponse> {
	const qs = new URLSearchParams();
	if (params.project) qs.set('project', params.project);
	if (params.tag) qs.set('tag', params.tag);
	if (params.participant) qs.set('participant', params.participant);
	if (params.resolved !== undefined) qs.set('resolved', String(params.resolved));
	const suffix = qs.toString();
	return getJson<ThreadsListResponse>(`/api/threads${suffix ? `?${suffix}` : ''}`);
}

export function getTags(): Promise<TagsResponse> {
	return getJson<TagsResponse>('/api/tags');
}

export function getSessionFleet(sessionId: string): Promise<SessionFleetDto> {
	return getJson<SessionFleetDto>(`/api/sessions/${encodeURIComponent(sessionId)}/fleet`);
}
