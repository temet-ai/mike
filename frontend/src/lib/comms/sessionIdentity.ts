import { ApiError } from '$lib/api';

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as T;
}

async function putJson<Req, Res>(path: string, body: Req): Promise<Res> {
	const res = await fetch(path, {
		method: 'PUT',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `PUT ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string; message?: string };
			const human = errBody?.message ?? errBody?.error;
			if (typeof human === 'string' && human.length > 0) message = human;
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

export interface SessionIdentityDto {
	goal: string | null;
	next_steps: string | null;
	role: string | null;
	name: string | null;
	instructions: string | null;
}

export interface SetSessionIdentityBody {
	goal?: string | null;
	next_steps?: string | null;
	role?: string | null;
	name?: string | null;
	instructions?: string | null;
	set_by?: string;
}

export function getSessionIdentity(sessionId: string): Promise<SessionIdentityDto> {
	return getJson<SessionIdentityDto>(`/api/sessions/${encodeURIComponent(sessionId)}/goal`);
}

export function putSessionIdentity(
	sessionId: string,
	body: SetSessionIdentityBody
): Promise<{ ok: boolean }> {
	return putJson<SetSessionIdentityBody, { ok: boolean }>(
		`/api/sessions/${encodeURIComponent(sessionId)}/goal`,
		{ set_by: 'operator', ...body }
	);
}
