import { ApiError } from '$lib/api';
import { cachedGet } from '$lib/stores/cache/apiCache.svelte';

export interface InjectedSessionLesson {
	lesson_id: number;
	lesson_text: string;
	kind: string;
	injection_count: number;
	first_injected_at: string;
	last_injected_at: string;
}

export interface RecordedSessionLesson {
	lesson_id: number;
	lesson_text: string;
	kind: string;
	created_at: string;
}

export interface SessionLessons {
	injected: InjectedSessionLesson[];
	injected_count: number;
	recorded: RecordedSessionLesson[];
	recorded_count: number;
}

export async function getSessionLessons(sessionId: string): Promise<SessionLessons> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/lessons`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SessionLessons;
	});
}
