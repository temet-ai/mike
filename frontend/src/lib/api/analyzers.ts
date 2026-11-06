import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';

export interface SessionAnalyzerDto {
	analyzer_name: string;
	display_name: string;
	enabled: boolean;
	hit_count: number;
	is_intercept: boolean;
}

export async function getSessionAnalyzers(sessionId: string): Promise<SessionAnalyzerDto[]> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/analyzers`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SessionAnalyzerDto[];
	});
}

export async function patchAnalyzerEnabled(
	sessionId: string,
	analyzerName: string,
	enabled: boolean
): Promise<void> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/analyzers/${encodeURIComponent(analyzerName)}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ enabled })
	});
	if (!res.ok) {
		throw new ApiError(`PATCH ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache(`/api/sessions/${encodeURIComponent(sessionId)}/analyzers`);
}
