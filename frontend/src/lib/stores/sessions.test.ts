import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		listSessions: vi.fn(),
		patchSessionPin: vi.fn(),
		findText: vi.fn(),
		searchContent: vi.fn()
	};
});

import { listSessions, patchSessionPin, findText, searchContent } from '$lib/api';
import {
	createSessionListStore,
	looksLikeSessionId,
	PAGE_SIZE,
	THROTTLE_MS,
	LIVE_STICKY_GRACE_MS,
	SESSION_SEARCH_DEBOUNCE_MS,
	SESSION_SEARCH_MIN_QUERY
} from './sessions.svelte';
import { buildSessionSummary, buildUnifiedTextMatch } from '$lib/components/__fixtures__/session';
import type { FindTextUnifiedResponse, ContentSearchResponse } from '$lib/types';

const mockList = vi.mocked(listSessions);
const mockPin = vi.mocked(patchSessionPin);
const mockFind = vi.mocked(findText);
const mockSearchContent = vi.mocked(searchContent);

function findResponse(over: Partial<FindTextUnifiedResponse> = {}): FindTextUnifiedResponse {
	return {
		needle: 'auth',
		case_sensitive: true,
		context_bytes: 256,
		full_match_count: 0,
		matches_truncated: false,
		matches: [],
		window_note: null,
		...over
	};
}

function contentResponse(over: Partial<ContentSearchResponse> = {}): ContentSearchResponse {
	return {
		query: 'auth',
		total_matches: 0,
		total_sessions: 0,
		sessions: [],
		has_more: false,
		...over
	};
}

beforeEach(() => {
	vi.useFakeTimers();
	mockList.mockReset();
	mockList.mockResolvedValue([]);
	mockPin.mockReset();
	mockFind.mockReset();
	mockFind.mockResolvedValue(findResponse());
	mockSearchContent.mockReset();
	mockSearchContent.mockResolvedValue(contentResponse());
});

afterEach(() => {
	vi.useRealTimers();
});

describe('refresh', () => {
	it('initial refresh populates `sessions` from the API', async () => {
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1' })]);
		const store = createSessionListStore();
		await store.refresh();
		expect(store.sessions).toHaveLength(1);
		expect(store.sessions[0]?.id).toBe('sess-1');
		expect(store.error).toBeNull();
	});

	it('dedupes overlapping refresh() calls into one fetch', async () => {
		mockList.mockResolvedValue([]);
		const store = createSessionListStore();
		const a = store.refresh();
		const b = store.refresh();
		await Promise.all([a, b]);
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('preserves `title` on sessions, supporting both null and populated values', async () => {
		mockList.mockResolvedValue([
			buildSessionSummary({ id: 'sess-with-title', title: 'Optimize request polling efficiency' }),
			buildSessionSummary({ id: 'sess-no-title', title: null })
		]);
		const store = createSessionListStore();
		await store.refresh();
		expect(store.sessions).toHaveLength(2);
		expect(store.sessions[0]?.title).toBe('Optimize request polling efficiency');
		expect(store.sessions[1]?.title).toBeNull();
	});

	it('captures ApiError messages into `error`', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockRejectedValue(new ApiError('boom', 500, '/api/sessions'));
		const store = createSessionListStore();
		await store.refresh();
		expect(store.error).toBe('boom');
		expect(store.sessions).toHaveLength(0);
	});

	it('loading is true during fetch and false after success', async () => {
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1' })]);
		const store = createSessionListStore();
		const promise = store.refresh();
		expect(store.loading).toBe(true);
		await promise;
		expect(store.loading).toBe(false);
	});

	it('loading resets to false after error', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockRejectedValue(new ApiError('fail', 500, '/api/sessions'));
		const store = createSessionListStore();
		await store.refresh();
		expect(store.loading).toBe(false);
	});
});

describe('trailing-edge throttle', () => {
	it('does NOT refetch on payload-bearing frames (TextDelta/etc.)', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'TextDelta', id: 1, text: 'hi' });
		store.handleEvent({ type: 'ThinkingDelta', id: 1, thinking: 'mmm' });
		store.handleEvent({ type: 'ToolUseStart', id: 1, tool_id: 'tu_x', tool_name: 'Read' });
		store.handleEvent({ type: 'ToolInputDelta', id: 1, partial_json: '{"a":' });
		store.handleEvent({ type: 'ToolUseEnd', id: 1 });
		store.handleEvent({
			type: 'ResponseComplete',
			id: 1,
			stop_reason: 'end_turn',
			usage: null
		});
		store.handleEvent({ type: 'ResponseError', id: 1, error: 'boom' });
		store.handleEvent({
			type: 'UsageReceived',
			id: 1,
			usage: {
				input_tokens: 0,
				output_tokens: 0,
				cache_creation_input_tokens: null,
				cache_read_input_tokens: null
			}
		});
		store.handleEvent({ type: 'ModelConfirmed', id: 1, model: 'claude-sonnet-4-5' });

		await vi.advanceTimersByTimeAsync(THROTTLE_MS * 2);
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('does NOT fire a refetch immediately on a single RowChanged{sessions}', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'sessions',
			id: 'sess-1',
			session_id: null,
			op: 'UPDATE'
		});
		expect(mockList).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(THROTTLE_MS);
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('coalesces a RowChanged{sessions} burst into exactly one trailing refetch', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		for (let i = 0; i < 10; i++) {
			store.handleEvent({
				type: 'RowChanged',
				table: 'sessions',
				id: `sess-${i}`,
				session_id: null,
				op: 'UPDATE'
			});
		}
		expect(mockList).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(THROTTLE_MS);
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('refetches on RowChanged{requests, INSERT} (new session implied)', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'requests',
			id: '42',
			session_id: 'sess-1',
			op: 'INSERT'
		});
		await vi.advanceTimersByTimeAsync(THROTTLE_MS);
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('does NOT refetch on RowChanged{requests, UPDATE} (covered by sessions trigger)', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'requests',
			id: '42',
			session_id: 'sess-1',
			op: 'UPDATE'
		});
		await vi.advanceTimersByTimeAsync(THROTTLE_MS * 2);
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('does NOT refetch on RowChanged{filter_rules} or {prompt_compositions}', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'filter_rules',
			id: 'rule-uuid',
			session_id: null,
			op: 'UPDATE'
		});
		store.handleEvent({
			type: 'RowChanged',
			table: 'prompt_compositions',
			id: 'comp-uuid',
			session_id: null,
			op: 'INSERT'
		});
		await vi.advanceTimersByTimeAsync(THROTTLE_MS * 2);
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('Lagged frames bypass the throttle and refetch immediately', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'sessions',
			id: 'sess-1',
			session_id: null,
			op: 'UPDATE'
		});
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'Lagged', skipped: 3 });
		await vi.advanceTimersByTimeAsync(0);
		expect(mockList).toHaveBeenCalledTimes(2);

		await vi.advanceTimersByTimeAsync(THROTTLE_MS * 2);
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('dispose() cancels the pending trailing timer', async () => {
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'sessions',
			id: 'sess-1',
			session_id: null,
			op: 'UPDATE'
		});
		store.dispose();

		await vi.advanceTimersByTimeAsync(THROTTLE_MS * 2);
		expect(mockList).toHaveBeenCalledTimes(1);
	});
});

describe('WS tracking for status pill unification', () => {
	it('latestWsEventForSession returns null when no mapping registered', () => {
		const store = createSessionListStore();
		expect(store.latestWsEventForSession('sess-1')).toBeNull();
	});

	it('returns the latest WS event for a registered session', () => {
		const store = createSessionListStore();
		store.registerSessionRequests('sess-1', [10, 11]);
		store.handleEvent({ type: 'TextDelta', id: 10, text: 'hi' });
		store.handleEvent({ type: 'ResponseComplete', id: 11, stop_reason: 'end_turn', usage: null });
		const ev = store.latestWsEventForSession('sess-1');
		expect(ev).not.toBeNull();
		expect(ev?.type).toBe('ResponseComplete');
	});

	it('does not return events for unrelated request ids', () => {
		const store = createSessionListStore();
		store.registerSessionRequests('sess-1', [10]);
		store.handleEvent({ type: 'TextDelta', id: 99, text: 'other' });
		expect(store.latestWsEventForSession('sess-1')).toBeNull();
	});

	it('clears WS tracking on successful refresh', async () => {
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1' })]);
		const store = createSessionListStore();
		store.registerSessionRequests('sess-1', [10]);
		store.handleEvent({ type: 'TextDelta', id: 10, text: 'hi' });
		expect(store.latestWsEventForSession('sess-1')).not.toBeNull();

		await store.refresh();
		expect(store.latestWsEventForSession('sess-1')).toBeNull();
	});

	it('prunes tracking for sessions removed from the list (memory leak regression)', async () => {
		const allSessions = Array.from({ length: 10 }, (_, i) =>
			buildSessionSummary({ id: `sess-${i}` })
		);
		mockList.mockResolvedValue(allSessions);
		const store = createSessionListStore();
		for (let i = 0; i < 10; i++) {
			store.registerSessionRequests(`sess-${i}`, [i * 10, i * 10 + 1]);
		}
		await store.refresh();
		expect(store.__wsTrackingSize).toBe(20);

		const remainingSessions = allSessions.slice(0, 5);
		mockList.mockResolvedValue(remainingSessions);
		await store.refresh();

		expect(store.__wsTrackingSize).toBe(10);
	});

	it('dispose() clears all tracking maps', () => {
		const store = createSessionListStore();
		store.registerSessionRequests('sess-1', [10, 11, 12]);
		store.handleEvent({ type: 'TextDelta', id: 10, text: 'hi' });
		expect(store.__wsTrackingSize).toBe(3);

		store.dispose();
		expect(store.__wsTrackingSize).toBe(0);
	});
});

describe('pagination', () => {
	function makePage(count: number, startIndex = 0) {
		return Array.from({ length: count }, (_, i) =>
			buildSessionSummary({ id: `sess-${startIndex + i}` })
		);
	}

	it('refresh() requests offset=0 + default limit', async () => {
		mockList.mockResolvedValue([]);
		const store = createSessionListStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledWith({ offset: 0, limit: PAGE_SIZE });
	});

	it('hasMore is true when the initial page is full', async () => {
		mockList.mockResolvedValue(makePage(PAGE_SIZE));
		const store = createSessionListStore();
		await store.refresh();
		expect(store.hasMore).toBe(true);
	});

	it('hasMore is false when the initial page is short', async () => {
		mockList.mockResolvedValue(makePage(3));
		const store = createSessionListStore();
		await store.refresh();
		expect(store.hasMore).toBe(false);
	});

	it('loadMore() appends the next page with offset=current length', async () => {
		const firstPage = makePage(PAGE_SIZE);
		mockList.mockResolvedValueOnce(firstPage);
		const store = createSessionListStore();
		await store.refresh();

		const secondPage = makePage(10, PAGE_SIZE);
		mockList.mockResolvedValueOnce(secondPage);
		await store.loadMore();

		expect(mockList).toHaveBeenLastCalledWith({ offset: PAGE_SIZE, limit: PAGE_SIZE });
		expect(store.sessions).toHaveLength(PAGE_SIZE + 10);
		expect(store.hasMore).toBe(false);
	});

	it('loadMore() dedupes overlapping ids against existing rows', async () => {
		mockList.mockResolvedValueOnce(makePage(PAGE_SIZE));
		const store = createSessionListStore();
		await store.refresh();

		const dup = buildSessionSummary({ id: 'sess-3' });
		const overlapping = [dup, ...makePage(9, PAGE_SIZE)];
		mockList.mockResolvedValueOnce(overlapping);
		await store.loadMore();

		expect(store.sessions).toHaveLength(PAGE_SIZE + 9);
	});

	it('loadMore() is a no-op when hasMore is false', async () => {
		mockList.mockResolvedValueOnce(makePage(3));
		const store = createSessionListStore();
		await store.refresh();
		expect(store.hasMore).toBe(false);
		expect(mockList).toHaveBeenCalledTimes(1);

		await store.loadMore();
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('loadMore() captures errors into loadMoreError without clearing sessions', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockResolvedValueOnce(makePage(PAGE_SIZE));
		const store = createSessionListStore();
		await store.refresh();
		const baseline = store.sessions.length;

		mockList.mockRejectedValueOnce(new ApiError('pagination blew up', 500, '/api/sessions'));
		await store.loadMore();

		expect(store.loadMoreError).toBe('pagination blew up');
		expect(store.sessions).toHaveLength(baseline);
	});

	it('refresh() resets loadMoreError', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockResolvedValueOnce(makePage(PAGE_SIZE));
		const store = createSessionListStore();
		await store.refresh();
		mockList.mockRejectedValueOnce(new ApiError('boom', 500, '/api/sessions'));
		await store.loadMore();
		expect(store.loadMoreError).toBe('boom');

		mockList.mockResolvedValueOnce([]);
		await store.refresh();
		expect(store.loadMoreError).toBeNull();
	});
});

describe('togglePin', () => {
	it('is a no-op when the session id is not in the list', async () => {
		const store = createSessionListStore();
		await store.togglePin('sess-unknown', true);
		expect(mockPin).not.toHaveBeenCalled();
	});

	it('optimistically flips pinned and confirms on success', async () => {
		mockList.mockResolvedValueOnce([buildSessionSummary({ id: 'sess-1', pinned: false })]);
		const store = createSessionListStore();
		await store.refresh();
		mockPin.mockResolvedValueOnce({ session_id: 'sess-1', pinned: true });

		const promise = store.togglePin('sess-1', true);
		expect(store.sessions[0]?.pinned).toBe(true);
		await promise;
		expect(store.sessions[0]?.pinned).toBe(true);
		expect(mockPin).toHaveBeenCalledWith('sess-1', true);
	});

	it('reconciles when the server echoes a different value', async () => {
		mockList.mockResolvedValueOnce([buildSessionSummary({ id: 'sess-1', pinned: false })]);
		const store = createSessionListStore();
		await store.refresh();
		mockPin.mockResolvedValueOnce({ session_id: 'sess-1', pinned: false });

		await store.togglePin('sess-1', true);
		expect(store.sessions[0]?.pinned).toBe(false);
	});

	it('rolls back to the previous pinned flag on error', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockResolvedValueOnce([buildSessionSummary({ id: 'sess-1', pinned: false })]);
		const store = createSessionListStore();
		await store.refresh();
		mockPin.mockRejectedValueOnce(
			new ApiError('persistence disabled', 503, '/api/sessions/sess-1/pin')
		);

		await store.togglePin('sess-1', true);
		expect(store.sessions[0]?.pinned).toBe(false);
		expect(store.error).toBe('persistence disabled');
	});

	it('does not mutate unrelated rows in the list', async () => {
		mockList.mockResolvedValueOnce([
			buildSessionSummary({ id: 'sess-1', pinned: false }),
			buildSessionSummary({ id: 'sess-2', pinned: true })
		]);
		const store = createSessionListStore();
		await store.refresh();
		mockPin.mockResolvedValueOnce({ session_id: 'sess-1', pinned: true });

		await store.togglePin('sess-1', true);
		expect(store.sessions[0]?.pinned).toBe(true);
		expect(store.sessions[1]?.pinned).toBe(true);
	});
});

describe('sticky liveness (hysteresis)', () => {
	it('a never-observed session is not recently active', () => {
		const store = createSessionListStore();
		expect(store.isRecentlyActive('sess-x', 1_000)).toBe(false);
	});

	it('a session observed live is recently active immediately', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'streaming', 10_000);
		expect(store.isRecentlyActive('sess-1', 10_000)).toBe(true);
	});

	it('pending counts as live (same partition as streaming)', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'pending', 5_000);
		expect(store.isRecentlyActive('sess-1', 5_000)).toBe(true);
	});

	it('stays active through a quiet gap shorter than the grace window', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'streaming', 0);
		store.observeStatus('sess-1', 'complete', LIVE_STICKY_GRACE_MS - 1);
		expect(store.isRecentlyActive('sess-1', LIVE_STICKY_GRACE_MS - 1)).toBe(true);
	});

	it('falls inactive once quiet longer than the grace window', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'streaming', 0);
		store.observeStatus('sess-1', 'complete', LIVE_STICKY_GRACE_MS - 1);
		expect(store.isRecentlyActive('sess-1', LIVE_STICKY_GRACE_MS)).toBe(false);
		expect(store.isRecentlyActive('sess-1', LIVE_STICKY_GRACE_MS + 5_000)).toBe(false);
	});

	it('re-observing live refreshes the window (no strobe across request boundaries)', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'streaming', 0);
		store.observeStatus('sess-1', 'complete', 30_000);
		expect(store.isRecentlyActive('sess-1', 30_000)).toBe(true);
		store.observeStatus('sess-1', 'streaming', 45_000);
		expect(store.isRecentlyActive('sess-1', 80_000)).toBe(true);
		expect(store.isRecentlyActive('sess-1', 45_000 + LIVE_STICKY_GRACE_MS)).toBe(false);
	});

	it('non-live kinds never stamp the registry', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'complete', 1_000);
		store.observeStatus('sess-1', 'error', 2_000);
		expect(store.isRecentlyActive('sess-1', 2_000)).toBe(false);
	});

	it('dispose clears the registry', () => {
		const store = createSessionListStore();
		store.observeStatus('sess-1', 'streaming', 1_000);
		expect(store.isRecentlyActive('sess-1', 1_000)).toBe(true);
		store.dispose();
		expect(store.isRecentlyActive('sess-1', 1_000)).toBe(false);
	});
});

describe('looksLikeSessionId', () => {
	it.each([
		'11111111-0000-0000-0000-000000000001',
		'11111111-0000',
		'hash-deadbeefdeadbeef',
		'0123456789abcdef0123'
	])('treats %s as id-shaped', (q) => {
		expect(looksLikeSessionId(q)).toBe(true);
	});

	it.each(['auth', 'migration', 'rate limit', 'cafe', 'fix the bug'])(
		'treats %s as free text',
		(q) => {
			expect(looksLikeSessionId(q)).toBe(false);
		}
	);
});

describe('unified search (setSearch)', () => {
	it('does not search below the min-query gate', async () => {
		const store = createSessionListStore();
		store.setSearch('aut');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS * 2);
		expect(mockList).not.toHaveBeenCalled();
		expect(mockFind).not.toHaveBeenCalled();
		expect(mockSearchContent).not.toHaveBeenCalled();
		expect(store.searchHits).toEqual([]);
		expect(store.bodyHits).toEqual([]);
		expect(store.contentHits).toEqual([]);
	});

	it('min-query gate is 4', () => {
		expect(SESSION_SEARCH_MIN_QUERY).toBe(4);
	});

	it('free text fans out to listSessions, findText, AND searchContent', async () => {
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1', title: 'auth work' })]);
		mockFind.mockResolvedValue(
			findResponse({
				full_match_count: 2,
				matches: [
					buildUnifiedTextMatch({ request_id: 10 }),
					buildUnifiedTextMatch({ request_id: 11 })
				]
			})
		);
		mockSearchContent.mockResolvedValue(
			contentResponse({
				total_matches: 3,
				total_sessions: 1,
				has_more: true,
				sessions: [
					{
						session_id: 'sess-1',
						project_path: '/proj',
						project_display_name: '/proj',
						title: 'auth work',
						match_count: 3,
						turn_count: 2,
						last_match_at: '2026-07-01T00:00:00Z',
						matches_truncated: false,
						matches: [
							{
								turn_id: 1,
								request_id: 10,
								role: 'prompt',
								chunk_idx: 0,
								byte_offset: 0,
								match_length: 4,
								line: 1,
								column: 1,
								before: '',
								matched: 'auth',
								after: '',
								created_at: '2026-07-01T00:00:00Z'
							}
						]
					}
				]
			})
		);
		const store = createSessionListStore();
		store.setSearch('auth');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);

		expect(mockList).toHaveBeenCalledWith({ search: 'auth' });
		expect(mockFind).toHaveBeenCalledWith(
			expect.objectContaining({ needle: 'auth', source: 'response' })
		);
		expect(mockSearchContent).toHaveBeenCalledWith(expect.objectContaining({ q: 'auth' }));
		expect(store.searchHits).toHaveLength(1);
		expect(store.bodyHits).toHaveLength(2);
		expect(store.bodyFullMatchCount).toBe(2);
		expect(store.contentHits).toHaveLength(1);
		expect(store.contentTotalMatches).toBe(3);
		expect(store.contentTotalSessions).toBe(1);
		expect(store.contentHasMore).toBe(true);
	});

	it('an id-shaped query runs listSessions ONLY, never findText or searchContent', async () => {
		const uuid = '11111111-0000-0000-0000-000000000001';
		mockList.mockResolvedValue([buildSessionSummary({ id: uuid })]);
		const store = createSessionListStore();
		store.setSearch(uuid);
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);

		expect(mockList).toHaveBeenCalledWith({ search: uuid });
		expect(mockFind).not.toHaveBeenCalled();
		expect(mockSearchContent).not.toHaveBeenCalled();
		expect(store.searchHits).toHaveLength(1);
		expect(store.bodyHits).toEqual([]);
		expect(store.contentHits).toEqual([]);
	});

	it('captures a searchContent error into contentSearchError without touching other lanes', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1' })]);
		mockSearchContent.mockRejectedValue(new ApiError('scan failed', 500, '/api/content/search'));
		const store = createSessionListStore();
		store.setSearch('auth');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);

		expect(store.contentSearchError).toBe('scan failed');
		expect(store.contentHits).toEqual([]);
		expect(store.searchHits).toHaveLength(1);
		expect(store.searchError).toBeNull();
		expect(store.bodySearchError).toBeNull();
	});

	it('surfaces full_match_count, truncation, and the window note', async () => {
		mockFind.mockResolvedValue(
			findResponse({
				full_match_count: 437,
				matches_truncated: true,
				matches: [buildUnifiedTextMatch()],
				window_note: 'Searched the last 7 days.'
			})
		);
		const store = createSessionListStore();
		store.setSearch('error');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);

		expect(store.bodyFullMatchCount).toBe(437);
		expect(store.bodyMatchesTruncated).toBe(true);
		expect(store.bodyWindowNote).toBe('Searched the last 7 days.');
	});

	it('captures a findText error into bodySearchError without touching searchHits', async () => {
		const { ApiError } = await import('$lib/api');
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1' })]);
		mockFind.mockRejectedValue(new ApiError('scan failed', 500, '/api/body/find'));
		const store = createSessionListStore();
		store.setSearch('auth');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);

		expect(store.bodySearchError).toBe('scan failed');
		expect(store.bodyHits).toEqual([]);
		expect(store.searchHits).toHaveLength(1);
		expect(store.searchError).toBeNull();
	});

	it('dropping below the gate clears all lanes', async () => {
		mockList.mockResolvedValue([buildSessionSummary({ id: 'sess-1' })]);
		mockFind.mockResolvedValue(
			findResponse({ matches: [buildUnifiedTextMatch()], full_match_count: 1 })
		);
		mockSearchContent.mockResolvedValue(contentResponse({ total_matches: 1, total_sessions: 1 }));
		const store = createSessionListStore();
		store.setSearch('auth');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);
		expect(store.bodyHits).toHaveLength(1);
		expect(store.contentTotalMatches).toBe(1);

		store.setSearch('a');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		expect(store.searchHits).toEqual([]);
		expect(store.bodyHits).toEqual([]);
		expect(store.bodyFullMatchCount).toBe(0);
		expect(store.bodyWindowNote).toBeNull();
		expect(store.contentHits).toEqual([]);
		expect(store.contentTotalMatches).toBe(0);
	});

	it('seq guard drops a stale resolve when the query changes mid-flight', async () => {
		mockList.mockResolvedValue([]);
		const stale = buildUnifiedTextMatch({ request_id: 1, matched: 'stale' });
		const fresh = buildUnifiedTextMatch({ request_id: 2, matched: 'fresh' });
		mockFind
			.mockImplementationOnce(
				() =>
					new Promise((resolve) =>
						setTimeout(() => resolve(findResponse({ matches: [stale], full_match_count: 1 })), 1000)
					)
			)
			.mockResolvedValueOnce(findResponse({ matches: [fresh], full_match_count: 1 }));

		const store = createSessionListStore();
		store.setSearch('aaaa');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		store.setSearch('bbbb');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);
		await vi.advanceTimersByTimeAsync(2000);

		expect(store.bodyHits).toHaveLength(1);
		expect(store.bodyHits[0]?.matched).toBe('fresh');
	});

	it('seq guard also drops a stale searchContent resolve when the query changes mid-flight', async () => {
		mockList.mockResolvedValue([]);
		mockFind.mockResolvedValue(findResponse());
		mockSearchContent
			.mockImplementationOnce(
				() =>
					new Promise((resolve) =>
						setTimeout(
							() => resolve(contentResponse({ total_matches: 1, total_sessions: 1 })),
							1000
						)
					)
			)
			.mockResolvedValueOnce(contentResponse({ total_matches: 2, total_sessions: 2 }));

		const store = createSessionListStore();
		store.setSearch('aaaa');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		store.setSearch('bbbb');
		await vi.advanceTimersByTimeAsync(SESSION_SEARCH_DEBOUNCE_MS);
		await vi.advanceTimersByTimeAsync(0);
		await vi.advanceTimersByTimeAsync(2000);

		expect(store.contentTotalMatches).toBe(2);
		expect(store.contentTotalSessions).toBe(2);
	});
});
