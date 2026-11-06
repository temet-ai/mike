import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestDetail, RequestSummary, SessionDetail, SessionSummary } from '$lib/types';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		getRequest: vi.fn(),
		getRequestSummary: vi.fn(),
		getSession: vi.fn(),
		getAccumulatedText: vi.fn().mockResolvedValue(''),
		listSessionOperatorTurns: vi.fn().mockResolvedValue([])
	};
});

import { getRequest, getRequestSummary, getSession } from '$lib/api';
import type { ProxyEvent } from '$lib/types';
import {
	__clearSessionCacheForTests,
	__sessionCachePagesForTests,
	createSessionDetailStore,
	DETAILS_CAP_PER_SESSION,
	onDetailEviction,
	onSessionPageEvict,
	BURST_FETCH_SIZE,
	PAGE_SIZE,
	PAGES_CAP_PER_SESSION,
	PAGES_RESIDENT_CAP_PER_SESSION,
	WALK_BACK_PAGES,
	type DetailEviction,
	type SessionPageEviction
} from './sessionDetailStore.svelte';
import { __resetCacheForTests } from './unifiedCache.svelte';
import { __deleteIdbForTests } from './cacheIdb';

const mockGetRequest = vi.mocked(getRequest);
const mockGetRequestSummary = vi.mocked(getRequestSummary);
const mockGetSession = vi.mocked(getSession);

function rowChanged(
	overrides: Partial<Extract<ProxyEvent, { type: 'RowChanged' }>> = {}
): ProxyEvent {
	return {
		type: 'RowChanged',
		table: 'requests',
		id: '1',
		session_id: 'sess-live',
		op: 'INSERT',
		...overrides
	};
}

function makeSummary(id: string, request_count: number): SessionSummary {
	return {
		id,
		project: null,
		project_id: null,
		cwd: null,
		project_resolution: 'unresolved',
		first_seen: '2026-04-01T00:00:00Z',
		last_seen: '2026-04-01T01:00:00Z',
		request_count,
		models: ['claude-sonnet-4-5'],
		last_status: 'Complete',
		title: null
	};
}

function makeRow(id: number, sessionId: string): RequestSummary {
	return {
		id,
		method: 'POST',
		path: '/v1/messages',
		model: 'claude-sonnet-4-5',
		status: 'Complete',
		is_streaming: false,
		message_count: id,
		max_tokens: null,
		created_at: `2026-04-01T00:${String(id).padStart(2, '0')}:00.000Z`,
		completed_at: `2026-04-01T00:${String(id).padStart(2, '0')}:01.000Z`,
		session_id: sessionId,
		project: null,
		human_prompt: '',
		turn_type: 'user_prompt',
		response_tool_count: 0,
		response_tools_used: [],
		tool_names: []
	};
}

function makePage(
	sessionId: string,
	offset: number,
	count: number = PAGE_SIZE,
	total: number = 1000
): SessionDetail {
	return {
		summary: makeSummary(sessionId, total),
		requests: Array.from({ length: count }, (_, i) => makeRow(offset + i + 1, sessionId)),
		total_requests: total,
		offset,
		limit: count
	};
}

function makeDescPage(
	sessionId: string,
	ascOffset: number,
	count: number = PAGE_SIZE,
	total: number = 1000
): SessionDetail {
	const asc = makePage(sessionId, ascOffset, count, total);
	return { ...asc, requests: asc.requests.toReversed(), offset: 0 };
}

function makeDetail(id: number, sessionId: string): RequestDetail {
	return {
		id,
		upstream_request_id: null,
		upstream_message_id: null,
		session_id: sessionId,
		project: null,
		human_prompt: '',
		turn_type: 'user_prompt',
		method: 'POST',
		path: '/v1/messages',
		request_headers: [],
		request_body_size: 0,
		model: 'claude-sonnet-4-5',
		is_streaming: false,
		message_count: id,
		is_tool_loop: false,
		last_user_message: '',
		tool_names: [],
		params: {
			model: 'claude-sonnet-4-5',
			max_tokens: null,
			temperature: null,
			top_p: null,
			top_k: null,
			stop_sequences: null,
			thinking: null,
			tool_choice: null
		},
		status: 'Complete',
		response_headers: [],
		response_body_size: 100,
		stop_reason: null,
		token_usage: null,
		accumulated_text: '',
		accumulated_thinking: '',
		tool_calls: [],
		created_at: `2026-04-01T00:${String(id).padStart(2, '0')}:00.000Z`,
		completed_at: `2026-04-01T00:${String(id).padStart(2, '0')}:01.000Z`,
		has_request_body: true,
		has_response_body: true,
		diagnostics: []
	} as unknown as RequestDetail;
}

beforeEach(async () => {
	mockGetRequest.mockReset();
	mockGetRequestSummary.mockReset();
	mockGetSession.mockReset();
	__clearSessionCacheForTests();
	__resetCacheForTests();
	await __deleteIdbForTests();
});

function routeGetSession(sid: string, total: number): void {
	mockGetSession.mockImplementation(async (id, params) => {
		const p = (params ?? {}) as { order?: string; offset?: number; limit?: number };
		if (id !== sid) throw new Error(`unexpected session id ${id}`);
		if (p.order === 'desc') {
			const ascOffset = Math.max(0, total - PAGE_SIZE);
			return makeDescPage(sid, ascOffset, total - ascOffset, total);
		}
		const offset = p.offset ?? 0;
		const limit = p.limit ?? PAGE_SIZE;
		return makePage(sid, offset, Math.min(limit, total - offset), total);
	});
}

describe('page residency and the older-request walk-back', () => {
	const EAGER_PAGES = PAGES_CAP_PER_SESSION;
	const RESIDENT_PAGES = PAGES_RESIDENT_CAP_PER_SESSION;

	async function loadEagerWindow(
		sid: string,
		totalPages: number
	): Promise<ReturnType<typeof createSessionDetailStore>> {
		routeGetSession(sid, totalPages * PAGE_SIZE);
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		return store;
	}

	it('the eager fetch budget is well below the resident ceiling', () => {
		expect(PAGES_CAP_PER_SESSION).toBeLessThan(PAGES_RESIDENT_CAP_PER_SESSION);
		expect(WALK_BACK_PAGES).toBeGreaterThanOrEqual(1);
		expect(WALK_BACK_PAGES).toBeLessThanOrEqual(
			PAGES_RESIDENT_CAP_PER_SESSION - PAGES_CAP_PER_SESSION
		);
	});

	it('loadOlder after the eager burst lowers detail.offset and adds rows at the front', async () => {
		const sid = 'sess-walkback';
		const evictions: SessionPageEviction[] = [];
		const unsub = onSessionPageEvict((e) => {
			if (e.sessionId === sid) evictions.push(e);
		});
		try {
			const totalPages = EAGER_PAGES + 10;
			const store = await loadEagerWindow(sid, totalPages);
			const offsetBefore = store.detail!.offset ?? 0;
			const firstIdBefore = store.detail!.requests[0]!.id;
			const countBefore = store.detail!.requests.length;
			const lastIdBefore = store.detail!.requests.at(-1)!.id;
			expect(store.hasOlder).toBe(true);
			expect(store.olderCeilingReached).toBe(false);

			await store.loadOlder();

			const walkedRows = WALK_BACK_PAGES * PAGE_SIZE;
			expect(store.detail!.offset).toBe(offsetBefore - walkedRows);
			expect(store.detail!.requests[0]!.id).toBe(firstIdBefore - walkedRows);
			expect(store.detail!.requests.length).toBe(countBefore + walkedRows);
			expect(store.detail!.requests.at(-1)!.id).toBe(lastIdBefore);
			expect(evictions).toHaveLength(0);
		} finally {
			unsub();
		}
	});

	it('one walk-back is one request covering WALK_BACK_PAGES pages', async () => {
		const sid = 'sess-walkback-run';
		const store = await loadEagerWindow(sid, EAGER_PAGES + 10);
		const offsetBefore = store.detail!.offset ?? 0;
		const callsBefore = mockGetSession.mock.calls.length;

		await store.loadOlder();

		expect(mockGetSession.mock.calls.length).toBe(callsBefore + 1);
		expect(mockGetSession).toHaveBeenLastCalledWith(sid, {
			order: 'asc',
			offset: offsetBefore - WALK_BACK_PAGES * PAGE_SIZE,
			limit: WALK_BACK_PAGES * PAGE_SIZE
		});
		const pages = __sessionCachePagesForTests(sid)!;
		for (let i = 1; i <= WALK_BACK_PAGES; i++) {
			expect(pages.get(offsetBefore - i * PAGE_SIZE)).toBe(PAGE_SIZE);
		}
	});

	it('the walk-back clamps at offset 0 and the session start becomes reachable', async () => {
		const sid = 'sess-walkback-start';
		const total = 2 * PAGE_SIZE + 20;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(store.detail!.offset).toBe(total - PAGE_SIZE);

		await store.loadOlder();

		expect(mockGetSession).toHaveBeenLastCalledWith(sid, {
			order: 'asc',
			offset: 0,
			limit: total - PAGE_SIZE
		});
		expect(store.detail!.offset).toBe(0);
		expect(store.detail!.requests.map((r) => r.id)).toEqual(
			Array.from({ length: total }, (_, i) => i + 1)
		);
		expect(store.hasOlder).toBe(false);
		expect(store.olderCeilingReached).toBe(false);
	});

	it('walking back stops at the resident ceiling instead of fetching and evicting', async () => {
		const sid = 'sess-ceiling';
		const totalPages = RESIDENT_PAGES + 20;
		const trailingOffset = (totalPages - 1) * PAGE_SIZE;
		const store = await loadEagerWindow(sid, totalPages);

		let guard = 0;
		while (store.hasOlder && !store.olderCeilingReached) {
			if (++guard > totalPages) throw new Error('walk-back did not terminate');
			const offsetBefore = store.detail!.offset ?? 0;
			await store.loadOlder();
			const pages = __sessionCachePagesForTests(sid)!;
			expect(store.detail!.offset).toBeLessThan(offsetBefore);
			expect(pages.has(store.detail!.offset ?? 0)).toBe(true);
			expect(pages.size).toBeLessThanOrEqual(RESIDENT_PAGES);
		}

		const pages = __sessionCachePagesForTests(sid)!;
		expect(pages.size).toBe(RESIDENT_PAGES);
		expect(pages.has(trailingOffset)).toBe(true);
		const offsets = [...pages.keys()].sort((a, b) => a - b);
		for (let i = 1; i < offsets.length; i++) {
			expect(offsets[i]! - offsets[i - 1]!).toBe(PAGE_SIZE);
		}
		expect(store.hasOlder).toBe(true);
		expect(store.olderCeilingReached).toBe(true);

		const callsBefore = mockGetSession.mock.calls.length;
		const offsetAtCeiling = store.detail!.offset ?? 0;
		const countAtCeiling = store.detail!.requests.length;
		await store.loadOlder();
		expect(mockGetSession.mock.calls.length).toBe(callsBefore);
		expect(store.detail!.offset).toBe(offsetAtCeiling);
		expect(store.detail!.requests.length).toBe(countAtCeiling);
		expect(__sessionCachePagesForTests(sid)!.size).toBe(RESIDENT_PAGES);
		expect(store.loadOlderError).toBeNull();
		expect(store.loadingOlder).toBe(false);
	});

	it('a live INSERT does not evict the pages the operator walked back to', async () => {
		const sid = 'sess-live-walkback';
		const totalPages = EAGER_PAGES + 10;
		const total = totalPages * PAGE_SIZE;
		const store = await loadEagerWindow(sid, totalPages);
		await store.loadOlder();
		await store.loadOlder();
		const walkedOffset = store.detail!.offset ?? 0;
		const firstId = store.detail!.requests[0]!.id;
		const countBefore = store.detail!.requests.length;
		expect(__sessionCachePagesForTests(sid)!.has(walkedOffset)).toBe(true);

		mockGetRequestSummary.mockResolvedValueOnce(makeRow(total + 1, sid));
		store.handleEvent(rowChanged({ session_id: sid, id: String(total + 1), op: 'INSERT' }));
		await vi.waitFor(() => expect(store.detail!.requests.length).toBe(countBefore + 1));

		expect(__sessionCachePagesForTests(sid)!.has(walkedOffset)).toBe(true);
		expect(store.detail!.offset).toBe(walkedOffset);
		expect(store.detail!.requests[0]!.id).toBe(firstId);
		expect(store.detail!.requests.at(-1)!.id).toBe(total + 1);
	});

	it('olderCeilingReached stays false while there is nothing older to load', async () => {
		const sid = 'sess-ceiling-short';
		routeGetSession(sid, 5);
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(store.hasOlder).toBe(false);
		expect(store.olderCeilingReached).toBe(false);
	});
});

describe('W1.19 per-session details LRU eviction', () => {
	it('cap value matches the spec', () => {
		expect(DETAILS_CAP_PER_SESSION).toBe(60);
	});

	it('hydrates 60 bands without eviction', async () => {
		const sid = 'sess-e';
		const evictions: DetailEviction[] = [];
		const unsub = onDetailEviction((e) => evictions.push(e));
		try {
			mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 50, 50, 100));
			const store = createSessionDetailStore();
			await store.load(sid);
			for (let i = 1; i <= 60; i++) {
				mockGetRequest.mockResolvedValueOnce(makeDetail(i, sid));
			}
			for (let i = 1; i <= 60; i++) {
				await store.loadDetail(i);
			}
			expect(store.details.size).toBe(60);
			expect(evictions).toHaveLength(0);
		} finally {
			unsub();
		}
	});

	it('hydrating band 61 evicts the oldest entry, emits details-cap', async () => {
		const sid = 'sess-f';
		const evictions: DetailEviction[] = [];
		const unsub = onDetailEviction((e) => evictions.push(e));
		try {
			mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 50, 50, 100));
			const store = createSessionDetailStore();
			await store.load(sid);
			for (let i = 1; i <= 61; i++) {
				mockGetRequest.mockResolvedValueOnce(makeDetail(i, sid));
			}
			for (let i = 1; i <= 61; i++) {
				await store.loadDetail(i);
			}
			expect(store.details.size).toBe(60);
			expect(store.details.has(1)).toBe(false);
			expect(store.details.has(61)).toBe(true);
			expect(evictions).toHaveLength(1);
			expect(evictions[0]).toMatchObject({
				sessionId: sid,
				requestId: 1,
				reason: 'details-cap'
			});
		} finally {
			unsub();
		}
	});

	it('cancelDetail on an already-evicted id is a no-op', async () => {
		const sid = 'sess-g';
		const evictions: DetailEviction[] = [];
		const unsub = onDetailEviction((e) => evictions.push(e));
		try {
			mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 50, 50, 100));
			const store = createSessionDetailStore();
			await store.load(sid);
			for (let i = 1; i <= 61; i++) {
				mockGetRequest.mockResolvedValueOnce(makeDetail(i, sid));
			}
			for (let i = 1; i <= 61; i++) {
				await store.loadDetail(i);
			}
			expect(evictions).toHaveLength(1);
			store.cancelDetail(1);
			expect(evictions).toHaveLength(1);
		} finally {
			unsub();
		}
	});

	it('cancelDetail on a hydrated id evicts with cancel-pressure', async () => {
		const sid = 'sess-h';
		const evictions: DetailEviction[] = [];
		const unsub = onDetailEviction((e) => evictions.push(e));
		try {
			mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 50, 50, 100));
			const store = createSessionDetailStore();
			await store.load(sid);
			mockGetRequest.mockResolvedValueOnce(makeDetail(42, sid));
			await store.loadDetail(42);
			expect(store.details.has(42)).toBe(true);
			store.cancelDetail(42);
			expect(store.details.has(42)).toBe(false);
			expect(evictions).toHaveLength(1);
			expect(evictions[0]).toMatchObject({
				sessionId: sid,
				requestId: 42,
				reason: 'cancel-pressure'
			});
		} finally {
			unsub();
		}
	});

	it('session change resets details entirely', async () => {
		const sidA = 'sess-i';
		const sidB = 'sess-j';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sidA, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sidA);
		mockGetRequest.mockResolvedValueOnce(makeDetail(7, sidA));
		await store.loadDetail(7);
		expect(store.details.size).toBe(1);

		mockGetSession.mockResolvedValueOnce(makeDescPage(sidB, 0, 5, 5));
		await store.load(sidB);
		expect(store.details.size).toBe(0);
	});
});

describe('W2.6 predecessorOf', () => {
	function makeLineageRow(
		id: number,
		sessionId: string,
		turn_origin:
			| 'primary'
			| 'primary_first'
			| 'subagent'
			| 'title_sidecar'
			| 'quota_probe'
			| 'advisor_sidecar',
		subagent_persona: string | null = null
	): RequestSummary {
		return {
			id,
			method: 'POST',
			path: '/v1/messages',
			model: 'claude-sonnet-4-5',
			status: 'Complete',
			is_streaming: false,
			message_count: id,
			max_tokens: null,
			created_at: `2026-04-01T00:${String(id).padStart(2, '0')}:00.000Z`,
			completed_at: `2026-04-01T00:${String(id).padStart(2, '0')}:01.000Z`,
			session_id: sessionId,
			project: null,
			human_prompt: '',
			turn_type: 'user_prompt',
			turn_origin,
			subagent_persona,
			response_tool_count: 0,
			response_tools_used: [],
			tool_names: []
		};
	}

	async function loadStoreWithLineage(
		sid: string,
		rows: RequestSummary[]
	): Promise<ReturnType<typeof createSessionDetailStore>> {
		const total = rows.length;
		const page = {
			summary: makeSummary(sid, total),
			requests: rows.toReversed(),
			total_requests: total,
			offset: 0,
			limit: total
		} as SessionDetail;
		mockGetSession.mockResolvedValueOnce(page);
		const store = createSessionDetailStore();
		await store.load(sid);
		return store;
	}

	it('honors backend predecessor_request_id on hydrated RequestDetail when present', async () => {
		const sid = 'sess-pred-a';
		const rows = [makeLineageRow(10, sid, 'primary'), makeLineageRow(12, sid, 'primary')];
		const store = await loadStoreWithLineage(sid, rows);
		const detail = {
			...makeDetail(12, sid),
			predecessor_request_id: 99
		} as RequestDetail & { predecessor_request_id: number };
		mockGetRequest.mockResolvedValueOnce(detail);
		await store.loadDetail(12);
		expect(store.predecessorOf(12)).toBe(99);
	});

	it('falls back to client-walk over loaded requests when backend field absent', async () => {
		const sid = 'sess-pred-b';
		const rows = [
			makeLineageRow(10, sid, 'primary'),
			makeLineageRow(11, sid, 'primary'),
			makeLineageRow(12, sid, 'primary')
		];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(12)).toBe(11);
		expect(store.predecessorOf(11)).toBe(10);
		expect(store.predecessorOf(10)).toBeNull();
	});

	it('returns null for title_sidecar / quota_probe / advisor_sidecar', async () => {
		const sid = 'sess-pred-c';
		const rows = [
			makeLineageRow(10, sid, 'primary'),
			makeLineageRow(11, sid, 'title_sidecar'),
			makeLineageRow(12, sid, 'quota_probe'),
			makeLineageRow(13, sid, 'advisor_sidecar')
		];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(11)).toBeNull();
		expect(store.predecessorOf(12)).toBeNull();
		expect(store.predecessorOf(13)).toBeNull();
	});

	it('subagent walks back to the prior turn with the SAME subagent_persona', async () => {
		const sid = 'sess-pred-d';
		const rows = [
			makeLineageRow(10, sid, 'primary'),
			makeLineageRow(11, sid, 'subagent', 'frontend-developer'),
			makeLineageRow(12, sid, 'primary')
		];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(12)).toBe(10);
		expect(store.predecessorOf(11)).toBeNull();
	});

	it('interleaved subagents keep independent lineage chains', async () => {
		const sid = 'sess-pred-e';
		const rows = [
			makeLineageRow(20, sid, 'subagent', 'frontend-developer'),
			makeLineageRow(21, sid, 'subagent', 'rust-developer'),
			makeLineageRow(22, sid, 'subagent', 'frontend-developer'),
			makeLineageRow(23, sid, 'subagent', 'rust-developer')
		];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(22)).toBe(20);
		expect(store.predecessorOf(23)).toBe(21);
		expect(store.predecessorOf(20)).toBeNull();
		expect(store.predecessorOf(21)).toBeNull();
	});

	it('returns null when the predecessor is on an evicted page (pre-W2.4 fallback)', async () => {
		const sid = 'sess-pred-f';
		const rows = [makeLineageRow(901, sid, 'primary')];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(901)).toBeNull();
	});

	it('primary_first row has no predecessor (it opens the lineage)', async () => {
		const sid = 'sess-pred-g';
		const rows = [makeLineageRow(10, sid, 'primary_first'), makeLineageRow(11, sid, 'primary')];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(10)).toBeNull();
	});

	it('a subsequent primary chains back to primary_first as its predecessor', async () => {
		const sid = 'sess-pred-h';
		const rows = [makeLineageRow(10, sid, 'primary_first'), makeLineageRow(11, sid, 'primary')];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(11)).toBe(10);
	});

	it('primary_first does not chain to an unrelated subagent or sidecar', async () => {
		const sid = 'sess-pred-i';
		const rows = [
			makeLineageRow(9, sid, 'subagent', 'frontend-developer'),
			makeLineageRow(10, sid, 'primary_first')
		];
		const store = await loadStoreWithLineage(sid, rows);
		expect(store.predecessorOf(10)).toBeNull();
	});
});

describe('order param wiring', () => {
	it('cold cache issues exactly one desc fetch for the latest page', async () => {
		const sid = 'sess-ord-a';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 950, PAGE_SIZE, 1000));
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(mockGetSession).toHaveBeenCalledTimes(1);
		expect(mockGetSession).toHaveBeenCalledWith(sid, {
			order: 'desc',
			offset: 0,
			limit: PAGE_SIZE
		});
	});

	it('reverses the desc page so detail.requests stays chronological ascending', async () => {
		const sid = 'sess-ord-b';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 950, PAGE_SIZE, 1000));
		const store = createSessionDetailStore();
		await store.load(sid);
		const ids = store.detail!.requests.map((r) => r.id);
		expect(ids[0]).toBe(951);
		expect(ids[ids.length - 1]).toBe(1000);
		expect(ids).toEqual([...ids].sort((a, b) => a - b));
		expect(__sessionCachePagesForTests(sid)!.has(950)).toBe(true);
	});

	it('short sessions (total <= PAGE_SIZE) land at asc offset 0 in one fetch', async () => {
		const sid = 'sess-ord-c';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(mockGetSession).toHaveBeenCalledTimes(1);
		expect(store.detail!.requests.map((r) => r.id)).toEqual([1, 2, 3, 4, 5]);
		expect(store.detail!.offset).toBe(0);
		expect(store.hasOlder).toBe(false);
	});

	it('empty sessions resolve with no rows and no crash', async () => {
		const sid = 'sess-ord-d';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 0, 0));
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(store.detail).not.toBeNull();
		expect(store.detail!.requests).toEqual([]);
		expect(store.detail!.total_requests).toBe(0);
		expect(store.hasOlder).toBe(false);
	});

	it('loadOlder pages with explicit order=asc at the asc walk-back offset', async () => {
		const sid = 'sess-ord-e';
		const runRows = WALK_BACK_PAGES * PAGE_SIZE;
		const runStart = 950 - runRows;
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 950, PAGE_SIZE, 1000));
		const store = createSessionDetailStore();
		await store.load(sid);
		mockGetSession.mockResolvedValueOnce(makePage(sid, runStart, runRows, 1000));
		await store.loadOlder();
		expect(mockGetSession).toHaveBeenLastCalledWith(sid, {
			order: 'asc',
			offset: runStart,
			limit: runRows
		});
		const ids = store.detail!.requests.map((r) => r.id);
		expect(ids[0]).toBe(runStart + 1);
		expect(ids[ids.length - 1]).toBe(1000);
	});
});

describe('loadAll parallel full-session load', () => {
	function ascOffsetsCalled(): number[] {
		return mockGetSession.mock.calls
			.map((c) => (c[1] ?? {}) as { order?: string; offset?: number })
			.filter((p) => p.order === 'asc')
			.map((p) => p.offset ?? 0)
			.sort((a, b) => a - b);
	}

	function ascCallsMade(): Array<{ offset: number; limit: number }> {
		return mockGetSession.mock.calls
			.map((c) => (c[1] ?? {}) as { order?: string; offset?: number; limit?: number })
			.filter((p) => p.order === 'asc')
			.map((p) => ({ offset: p.offset ?? 0, limit: p.limit ?? PAGE_SIZE }))
			.sort((a, b) => a.offset - b.offset);
	}

	function ascOffsetsCovered(): number[] {
		const covered: number[] = [];
		for (const { offset, limit } of ascCallsMade()) {
			for (let off = offset; off < offset + limit; off += PAGE_SIZE) covered.push(off);
		}
		return covered.sort((a, b) => a - b);
	}

	it('within-cap session: loads every page in one burst, hasOlder false', async () => {
		const sid = 'sess-all-a';
		const total = 5 * PAGE_SIZE;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		expect(store.detail!.requests.length).toBe(total);
		expect(store.detail!.requests[0]!.id).toBe(1);
		expect(store.detail!.requests[total - 1]!.id).toBe(total);
		expect(store.hasOlder).toBe(false);
		const pages = __sessionCachePagesForTests(sid)!;
		expect(pages.size).toBe(5);
		const expectedAscOffsets = Array.from(
			{ length: total / PAGE_SIZE - 1 },
			(_, i) => i * PAGE_SIZE
		);
		expect(ascOffsetsCovered()).toEqual(expectedAscOffsets);
		expect(ascCallsMade()).toEqual([{ offset: 0, limit: 4 * PAGE_SIZE }]);
	});

	it('coalesces a long burst into BURST_FETCH_SIZE-sized requests', async () => {
		const sid = 'sess-all-burst';
		const runPages = BURST_FETCH_SIZE / PAGE_SIZE;
		const total = (runPages + 3) * PAGE_SIZE;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		expect(ascCallsMade()).toEqual([
			{ offset: 0, limit: BURST_FETCH_SIZE },
			{ offset: BURST_FETCH_SIZE, limit: 2 * PAGE_SIZE }
		]);
		expect(store.detail!.requests.length).toBe(total);
	});

	it('does NOT re-fetch the cold (trailing) page already resident', async () => {
		const sid = 'sess-all-b';
		const total = 3 * PAGE_SIZE;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		expect(ascOffsetsCovered()).toEqual([0, 50]);
		expect(ascCallsMade()).toEqual([{ offset: 0, limit: 2 * PAGE_SIZE }]);
	});

	it('is idempotent: a second loadAll issues no further fetches', async () => {
		const sid = 'sess-all-c';
		const total = 4 * PAGE_SIZE;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		const callsAfterFirst = mockGetSession.mock.calls.length;
		await store.loadAll();
		await store.loadAll();
		expect(mockGetSession.mock.calls.length).toBe(callsAfterFirst);
	});

	it('over-cap session: loads only the newest cap pages, hasOlder true', async () => {
		const sid = 'sess-all-d';
		const totalPages = PAGES_CAP_PER_SESSION + 5;
		const total = totalPages * PAGE_SIZE;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		const pages = __sessionCachePagesForTests(sid)!;
		expect(pages.size).toBe(PAGES_CAP_PER_SESSION);
		const lowestResident = Math.min(...pages.keys());
		expect(lowestResident).toBe((totalPages - PAGES_CAP_PER_SESSION) * PAGE_SIZE);
		expect(store.hasOlder).toBe(true);
		const ascOffsets = ascOffsetsCalled();
		expect(new Set(ascOffsets).size).toBe(ascOffsets.length);
	});

	it('one page reject leaves loadAllDoneFor unset; next loadAll re-bursts', async () => {
		const sid = 'sess-all-err';
		const total = 4 * PAGE_SIZE;
		let failOffset50 = true;
		mockGetSession.mockImplementation(async (id, params) => {
			const pp = (params ?? {}) as { order?: string; offset?: number; limit?: number };
			if (id !== sid) throw new Error(`unexpected session id ${id}`);
			if (pp.order === 'desc') {
				const ascOffset = Math.max(0, total - PAGE_SIZE);
				return makeDescPage(sid, ascOffset, total - ascOffset, total);
			}
			const offset = pp.offset ?? 0;
			const limit = pp.limit ?? PAGE_SIZE;
			if (failOffset50 && offset <= PAGE_SIZE && PAGE_SIZE < offset + limit) {
				throw new Error('page 50 boom');
			}
			return makePage(sid, offset, Math.min(limit, total - offset), total);
		});
		const store = createSessionDetailStore();
		await store.load(sid);
		await store.loadAll();
		expect(store.loadOlderError).toBe('Error: page 50 boom');
		const callsAfterFirst = mockGetSession.mock.calls.length;
		failOffset50 = false;
		await store.loadAll();
		expect(mockGetSession.mock.calls.length).toBeGreaterThan(callsAfterFirst);
		expect(store.loadOlderError).toBeNull();
		expect(store.detail!.requests.length).toBe(total);
	});

	it('concurrent loadAll calls share one burst', async () => {
		const sid = 'sess-all-e';
		const total = 4 * PAGE_SIZE;
		routeGetSession(sid, total);
		const store = createSessionDetailStore();
		await store.load(sid);
		const callsBefore = mockGetSession.mock.calls.length;
		await Promise.all([store.loadAll(), store.loadAll(), store.loadAll()]);
		const ascOffsets = ascOffsetsCalled();
		expect(new Set(ascOffsets).size).toBe(ascOffsets.length);
		expect(mockGetSession.mock.calls.length).toBe(callsBefore + 1);
		expect(ascCallsMade()).toEqual([{ offset: 0, limit: 3 * PAGE_SIZE }]);
	});
});

describe('W6 hasOlder reactive after load() alone', () => {
	it('is true on a multi-page session immediately after load(), before any loadAll/loadOlder', async () => {
		const sid = 'sess-hasolder';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 950, PAGE_SIZE, 1000));
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(store.detail!.offset).toBe(950);
		expect(store.hasOlder).toBe(true);
	});
});

describe('W7a detailErrors surface', () => {
	it('failed loadDetail adds the id; a later successful loadDetail removes it', async () => {
		const sid = 'sess-detailerr';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(store.detailErrors.size).toBe(0);

		mockGetRequest.mockRejectedValueOnce(new Error('detail boom'));
		await store.loadDetail(3);
		expect(store.detailErrors.has(3)).toBe(true);
		expect(store.details.has(3)).toBe(false);

		mockGetRequest.mockResolvedValueOnce(makeDetail(3, sid));
		await store.loadDetail(3);
		expect(store.detailErrors.has(3)).toBe(false);
		expect(store.details.has(3)).toBe(true);
	});

	it('session change resets detailErrors', async () => {
		const sidA = 'sess-detailerr-a';
		const sidB = 'sess-detailerr-b';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sidA, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sidA);
		mockGetRequest.mockRejectedValueOnce(new Error('boom'));
		await store.loadDetail(2);
		expect(store.detailErrors.has(2)).toBe(true);

		mockGetSession.mockResolvedValueOnce(makeDescPage(sidB, 0, 5, 5));
		await store.load(sidB);
		expect(store.detailErrors.size).toBe(0);
	});
});

describe('handleEvent — live RowChanged{requests} updates', () => {
	it('INSERT for the current session appends the fetched row and bumps total_requests', async () => {
		const sid = 'sess-live';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);
		expect(store.detail!.requests.length).toBe(5);
		expect(store.detail!.total_requests).toBe(5);
		expect(store.detail!.summary.request_count).toBe(5);

		mockGetRequestSummary.mockResolvedValueOnce(makeRow(6, sid));
		store.handleEvent(rowChanged({ session_id: sid, id: '6', op: 'INSERT' }));

		await vi.waitFor(() => expect(store.detail!.requests.length).toBe(6));
		expect(store.detail!.total_requests).toBe(6);
		expect(store.detail!.requests.some((r) => r.id === 6)).toBe(true);
		expect(mockGetRequestSummary).toHaveBeenCalledWith(6);
		expect(store.detail!.summary.request_count).toBe(6);
	});

	it('UPDATE for an already-loaded row replaces it with the fresh fetch in place', async () => {
		const sid = 'sess-live-2';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);
		const before = store.detail!.requests.find((r) => r.id === 3)!;
		expect(before.status).toBe('Complete');

		const patched = { ...makeRow(3, sid), status: 'Error: boom' };
		mockGetRequestSummary.mockResolvedValueOnce(patched);
		store.handleEvent(rowChanged({ session_id: sid, id: '3', op: 'UPDATE' }));

		await vi.waitFor(() =>
			expect(store.detail!.requests.find((r) => r.id === 3)!.status).toBe('Error: boom')
		);
		expect(store.detail!.requests.length).toBe(5);
		expect(store.detail!.total_requests).toBe(5);
	});

	it('UPDATE for a row that is NOT loaded is a no-op — no fetch, nothing to patch', async () => {
		const sid = 'sess-live-3';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);

		store.handleEvent(rowChanged({ session_id: sid, id: '999', op: 'UPDATE' }));
		await Promise.resolve();
		expect(mockGetRequestSummary).not.toHaveBeenCalled();
		expect(store.detail!.requests.length).toBe(5);
	});

	it('ignores a RowChanged for a different session than the one currently loaded', async () => {
		const sid = 'sess-live-4';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);

		store.handleEvent(rowChanged({ session_id: 'some-other-session', id: '6', op: 'INSERT' }));
		await Promise.resolve();
		expect(mockGetRequestSummary).not.toHaveBeenCalled();
		expect(store.detail!.requests.length).toBe(5);
	});

	it('ignores RowChanged events for tables other than requests', async () => {
		const sid = 'sess-live-5';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);

		store.handleEvent(rowChanged({ table: 'sessions', session_id: sid, id: sid, op: 'UPDATE' }));
		await Promise.resolve();
		expect(mockGetRequestSummary).not.toHaveBeenCalled();
	});

	it('is a no-op before any session has been loaded — no cache entry to update', () => {
		const store = createSessionDetailStore();
		expect(() =>
			store.handleEvent(rowChanged({ session_id: 'sess-never-loaded', id: '1', op: 'INSERT' }))
		).not.toThrow();
		expect(mockGetRequestSummary).not.toHaveBeenCalled();
	});

	it('does not double-fetch when the same row id fires twice before the first fetch resolves', async () => {
		const sid = 'sess-live-6';
		mockGetSession.mockResolvedValueOnce(makeDescPage(sid, 0, 5, 5));
		const store = createSessionDetailStore();
		await store.load(sid);

		let resolveFetch!: (row: RequestSummary) => void;
		mockGetRequestSummary.mockImplementationOnce(
			() => new Promise((resolve) => (resolveFetch = resolve))
		);
		store.handleEvent(rowChanged({ session_id: sid, id: '6', op: 'INSERT' }));
		store.handleEvent(rowChanged({ session_id: sid, id: '6', op: 'INSERT' }));
		resolveFetch(makeRow(6, sid));

		await vi.waitFor(() => expect(store.detail!.requests.length).toBe(6));
		expect(mockGetRequestSummary).toHaveBeenCalledTimes(1);
	});
});
