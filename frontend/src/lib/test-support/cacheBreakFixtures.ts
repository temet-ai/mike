import type { CacheBreak, CacheBreakGroup, CacheBreaksResponse } from '$lib/api/cacheBreaks';

export const CACHE_BREAK_SESSION_ID = '9d9b690e-cb59-4607-81f0-c96bdb3ff9b2';

export function buildCacheBreak(overrides: Partial<CacheBreak> = {}): CacheBreak {
	return {
		id: 1,
		request_id: 5120,
		session_id: CACHE_BREAK_SESSION_ID,
		session_title: 'Fix the deploy script',
		prev_request_id: 5118,
		source_request_id: 5119,
		kind: 'break',
		cause: 'mike',
		stage: 'self_identity',
		location: 'system',
		woken_by: null,
		ttl_seconds: 3600,
		gap_seconds: 42,
		expected_read: 180_000,
		cache_read: 12_000,
		tokens_rewritten: 168_000,
		detail: { applied_stages: ['self_identity'] },
		occurred_at: '2026-11-17T09:30:00Z',
		...overrides
	};
}

export const CACHE_BREAKS: CacheBreak[] = [
	buildCacheBreak({ id: 4 }),
	buildCacheBreak({
		id: 3,
		request_id: 5100,
		prev_request_id: 5098,
		source_request_id: null,
		cause: 'client',
		stage: null,
		location: 'tools',
		tokens_rewritten: 95_000,
		gap_seconds: 130,
		detail: { tools_added: ['mcp__mike__get_stats'], tools_removed: ['WebSearch'] },
		occurred_at: '2026-11-17T09:10:00Z'
	}),
	buildCacheBreak({
		id: 2,
		request_id: 5050,
		prev_request_id: 5049,
		source_request_id: null,
		cause: 'unknown',
		stage: null,
		location: 'messages_prefix',
		tokens_rewritten: 40_000,
		gap_seconds: 15,
		detail: { message: 7 },
		occurred_at: '2026-11-17T08:40:00Z'
	}),
	buildCacheBreak({
		id: 1,
		request_id: 5000,
		prev_request_id: 4990,
		source_request_id: null,
		kind: 'expiry',
		cause: 'expired',
		stage: null,
		location: 'none',
		woken_by: 'operator',
		gap_seconds: 5_400,
		tokens_rewritten: 1_200_000,
		detail: {},
		occurred_at: '2026-11-17T07:00:00Z'
	})
];

export const CACHE_BREAK_GROUPS: CacheBreakGroup[] = [
	{
		kind: 'expiry',
		cause: 'expired',
		stage: null,
		location: 'none',
		woken_by: 'operator',
		breaks: 1,
		sessions: 1,
		tokens_rewritten: 1_200_000
	},
	{
		kind: 'break',
		cause: 'mike',
		stage: 'self_identity',
		location: 'system',
		woken_by: null,
		breaks: 3,
		sessions: 2,
		tokens_rewritten: 500_000
	},
	{
		kind: 'break',
		cause: 'client',
		stage: null,
		location: 'tools',
		woken_by: null,
		breaks: 1,
		sessions: 1,
		tokens_rewritten: 95_000
	},
	{
		kind: 'break',
		cause: 'unknown',
		stage: null,
		location: 'messages_prefix',
		woken_by: null,
		breaks: 1,
		sessions: 1,
		tokens_rewritten: 40_000
	}
];

export function buildCacheBreaksResponse(
	overrides: Partial<CacheBreaksResponse> = {}
): CacheBreaksResponse {
	return {
		session_id: null,
		since: '2026-11-16T10:00:00Z',
		limit: 100,
		offset: 0,
		has_more: false,
		groups: CACHE_BREAK_GROUPS,
		breaks: CACHE_BREAKS,
		...overrides
	};
}
