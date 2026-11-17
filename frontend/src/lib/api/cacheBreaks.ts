import { ApiError } from '$lib/api';

/**
 * Prompt-cache breaks: a main-thread request that rewrote cache it should have read
 * (`break`), or resumed after the cache expired (`expiry`), with the cause mike found.
 * Wire shape follows crates/mike/src/api/cache_breaks.rs.
 */

export type CacheBreakKind = 'break' | 'expiry';
export type CacheBreakCause = 'mike' | 'client' | 'unknown' | 'expired';
export type CacheBreakLocation =
	| 'tools'
	| 'system'
	| 'messages_prefix'
	| 'delta'
	| 'continuation'
	| 'chain'
	| 'none'
	| 'unreadable';
export type CacheBreakWokenBy = 'operator' | 'harness' | 'agent' | 'mail' | 'unknown';

export interface CacheBreakDetail {
	applied_stages?: string[];
	filter_from?: string;
	filter_to?: string;
	tools_added?: string[];
	tools_removed?: string[];
	message?: number;
	[field: string]: unknown;
}

export interface CacheBreakGroup {
	kind: CacheBreakKind;
	cause: CacheBreakCause;
	stage: string | null;
	location: CacheBreakLocation | null;
	woken_by: CacheBreakWokenBy | null;
	breaks: number;
	sessions: number;
	tokens_rewritten: number;
}

export interface CacheBreak {
	id: number;
	request_id: number;
	session_id: string;
	session_title: string | null;
	prev_request_id: number;
	source_request_id: number | null;
	kind: CacheBreakKind;
	cause: CacheBreakCause;
	stage: string | null;
	location: CacheBreakLocation | null;
	woken_by: CacheBreakWokenBy | null;
	ttl_seconds: number;
	gap_seconds: number;
	expected_read: number;
	cache_read: number;
	tokens_rewritten: number;
	detail: CacheBreakDetail;
	occurred_at: string;
}

export interface CacheBreaksResponse {
	session_id: string | null;
	since: string;
	limit: number;
	offset: number;
	has_more: boolean;
	groups: CacheBreakGroup[];
	breaks: CacheBreak[];
}

export interface CacheBreaksParams {
	hours?: number;
	limit?: number;
	offset?: number;
}

function queryString(params: CacheBreaksParams): string {
	const query = new URLSearchParams();
	if (params.hours !== undefined) query.set('hours', String(params.hours));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	const qs = query.toString();
	return qs ? `?${qs}` : '';
}

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as T;
}

/** One session's breaks; leave `hours` out for the whole session. */
export function getSessionCacheBreaks(
	sessionId: string,
	params: CacheBreaksParams = {}
): Promise<CacheBreaksResponse> {
	return getJson<CacheBreaksResponse>(
		`/api/sessions/${encodeURIComponent(sessionId)}/cache-breaks${queryString(params)}`
	);
}

/** Every session's breaks in the window. */
export function getCacheBreaks(params: CacheBreaksParams = {}): Promise<CacheBreaksResponse> {
	return getJson<CacheBreaksResponse>(`/api/cache-breaks${queryString(params)}`);
}

/** Moves the scanner back so its next pass rescans the last `days` days; answers 202. */
export async function rescanCacheBreaks(days: number): Promise<void> {
	const path = '/api/cache-breaks/rescan';
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify({ days })
	});
	if (!res.ok) {
		throw new ApiError(`POST ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
}
