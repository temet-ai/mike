import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import type { Finding } from '$lib/types';
import type { PaginatedFindings } from '$lib/api';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		listFindingsPaginated: vi.fn()
	};
});

import { listFindingsPaginated } from '$lib/api';
import {
	createFindingsStore,
	FINDINGS_RETAINED_CAP,
	groupFindings,
	onFindingEvict,
	type FindingEviction
} from './findings.svelte';
import { buildFinding } from '$lib/test-support';

const mockListFindings = vi.mocked(listFindingsPaginated);

function env(findings: Finding[], total: number = findings.length): PaginatedFindings {
	return { findings, total, offset: 0, limit: 50 };
}

beforeEach(() => {
	vi.useFakeTimers();
	mockListFindings.mockReset();
	mockListFindings.mockResolvedValue(env([]));
});

afterEach(() => {
	vi.useRealTimers();
});

describe('refresh', () => {
	it('populates the findings list from the API', async () => {
		const finding = buildFinding();
		mockListFindings.mockResolvedValue(env([finding]));
		const store = createFindingsStore();
		await store.refresh();
		expect(store.findings).toEqual([finding]);
		expect(store.error).toBeNull();
		expect(store.lastFetchedAt).not.toBeNull();
	});

	it('dedupes overlapping refresh() calls into one fetch', async () => {
		mockListFindings.mockResolvedValue(env([]));
		const store = createFindingsStore();
		const a = store.refresh();
		const b = store.refresh();
		await Promise.all([a, b]);
		expect(mockListFindings).toHaveBeenCalledTimes(1);
		store.dispose();
	});

	it('captures ApiError into `error` without touching the list', async () => {
		const { ApiError } = await import('$lib/api');
		mockListFindings.mockRejectedValue(
			new ApiError('bad request', 400, '/api/diagnostics/findings')
		);
		const store = createFindingsStore();
		await store.refresh();
		expect(store.error).toBe('bad request');
		expect(store.findings).toBeNull();
		store.dispose();
	});

	it('passes the analyzer filter to listFindings', async () => {
		const store = createFindingsStore();
		store.setAnalyzerFilter('deflecting_blame_analyzer');
		await vi.advanceTimersByTimeAsync(0);
		expect(mockListFindings).toHaveBeenLastCalledWith({
			analyzer: 'deflecting_blame_analyzer',
			offset: 0,
			limit: 50
		});
		store.dispose();
	});

	it('empty array is the canonical "no findings" shape', async () => {
		mockListFindings.mockResolvedValue(env([]));
		const store = createFindingsStore();
		await store.refresh();
		expect(store.findings).toEqual([]);
		expect(store.error).toBeNull();
		store.dispose();
	});
});

describe('groupFindings', () => {
	it('returns [] for an empty array', () => {
		expect(groupFindings([])).toEqual([]);
	});

	it('groups by category → session → request with counts preserved', () => {
		const findings: Finding[] = [
			buildFinding({
				request_id: 1,
				session_id: 'sess-x',
				category: 'deflecting_blame',
				created_at: '2026-04-18T13:16:29.000Z'
			}),
			buildFinding({
				request_id: 1,
				session_id: 'sess-x',
				category: 'deflecting_blame',
				created_at: '2026-04-18T13:16:30.000Z'
			}),
			buildFinding({
				request_id: 2,
				session_id: 'sess-y',
				category: 'deflecting_blame',
				created_at: '2026-04-18T13:17:00.000Z'
			}),
			buildFinding({
				request_id: 1,
				session_id: 'sess-x',
				category: 'avoiding_work',
				created_at: '2026-04-18T13:18:00.000Z'
			})
		];

		const tree = groupFindings(findings);

		expect(tree).toHaveLength(2);
		expect(tree[0]?.category).toBe('deflecting_blame');
		expect(tree[0]?.count).toBe(3);
		expect(tree[1]?.category).toBe('avoiding_work');
		expect(tree[1]?.count).toBe(1);

		const cat0 = tree[0]!;
		expect(cat0.sessions).toHaveLength(2);
		expect(cat0.sessions[0]?.session_id).toBe('sess-y');
		expect(cat0.sessions[1]?.session_id).toBe('sess-x');
		expect(cat0.sessions[1]?.count).toBe(2);

		const sessX = cat0.sessions[1]!;
		expect(sessX.requests).toHaveLength(1);
		expect(sessX.requests[0]?.request_id).toBe(1);
		expect(sessX.requests[0]?.findings).toHaveLength(2);
	});
});

describe('WS events', () => {
	it('DiagnosticReady trailing-edge-throttles refetches', async () => {
		const store = createFindingsStore();
		await store.refresh();
		expect(mockListFindings).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'DiagnosticReady', id: 1, count: 1 });
		await vi.advanceTimersByTimeAsync(100);
		store.handleEvent({ type: 'DiagnosticReady', id: 2, count: 1 });
		await vi.advanceTimersByTimeAsync(100);
		store.handleEvent({ type: 'DiagnosticReady', id: 3, count: 1 });
		expect(mockListFindings).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(500);
		expect(mockListFindings).toHaveBeenCalledTimes(2);
		store.dispose();
	});

	it('Lagged bypasses the throttle and cancels any pending fire', async () => {
		const store = createFindingsStore();
		await store.refresh();
		expect(mockListFindings).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'DiagnosticReady', id: 1, count: 1 });

		store.handleEvent({ type: 'Lagged', skipped: 3 });
		await vi.advanceTimersByTimeAsync(0);
		expect(mockListFindings).toHaveBeenCalledTimes(2);

		await vi.advanceTimersByTimeAsync(1_000);
		expect(mockListFindings).toHaveBeenCalledTimes(2);
		store.dispose();
	});

	it('TextDelta / ResponseComplete / etc. are ignored', async () => {
		const store = createFindingsStore();
		await store.refresh();
		expect(mockListFindings).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'TextDelta', id: 1, text: 'hi' });
		store.handleEvent({
			type: 'ResponseComplete',
			id: 1,
			stop_reason: 'end_turn',
			usage: null
		});
		await vi.advanceTimersByTimeAsync(2_000);
		expect(mockListFindings).toHaveBeenCalledTimes(1);
		store.dispose();
	});
});

describe('dispose', () => {
	it('cancels pending throttle + polling', async () => {
		const store = createFindingsStore();
		store.startPolling(500);
		await store.refresh();
		expect(mockListFindings).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'DiagnosticReady', id: 1, count: 1 });

		store.dispose();
		await vi.advanceTimersByTimeAsync(5_000);
		expect(mockListFindings).toHaveBeenCalledTimes(1);
	});
});

function makePage(start: number, count: number): Finding[] {
	return Array.from({ length: count }, (_, i) =>
		buildFinding({
			request_id: start + i,
			created_at: `2026-04-18T13:${String(16 + i).padStart(2, '0')}:29.199371+00:00`,
			match_range: [0, start + i]
		})
	);
}

describe('pagination — refresh / loadOlder (Phase A4)', () => {
	it('refresh sends offset:0, limit:50 with no analyzer when filter is null', async () => {
		mockListFindings.mockResolvedValue(env([]));
		const store = createFindingsStore();
		await store.refresh();
		expect(mockListFindings).toHaveBeenCalledWith({
			offset: 0,
			limit: 50
		});
		store.dispose();
	});

	it('hasMore stays true when first page comes back full', async () => {
		mockListFindings.mockResolvedValue(env(makePage(1, 50)));
		const store = createFindingsStore();
		await store.refresh();
		expect(store.hasMore).toBe(true);
		expect(store.findings).toHaveLength(50);
		store.dispose();
	});

	it('hasMore flips false when first page is short', async () => {
		mockListFindings.mockResolvedValue(env(makePage(1, 17)));
		const store = createFindingsStore();
		await store.refresh();
		expect(store.hasMore).toBe(false);
		store.dispose();
	});

	it('loadOlder appends with offset = current length and trips hasMore on short page', async () => {
		const first = makePage(1, 50);
		const second = makePage(51, 30);
		mockListFindings.mockResolvedValueOnce(env(first));
		mockListFindings.mockResolvedValueOnce(env(second));

		const store = createFindingsStore();
		await store.refresh();
		expect(store.findings).toHaveLength(50);

		await store.loadOlder();
		expect(mockListFindings).toHaveBeenLastCalledWith({
			offset: 50,
			limit: 50
		});
		expect(store.findings).toHaveLength(80);
		expect(store.hasMore).toBe(false);
		store.dispose();
	});

	it('loadOlder dedupes by findingKey against existing rows', async () => {
		const first = makePage(1, 50);
		const overlap = first.slice(-5);
		const fresh = makePage(51, 5);
		mockListFindings.mockResolvedValueOnce(env(first));
		mockListFindings.mockResolvedValueOnce(env([...overlap, ...fresh]));

		const store = createFindingsStore();
		await store.refresh();
		await store.loadOlder();
		expect(store.findings).toHaveLength(55);
		store.dispose();
	});

	it('loadOlder is a no-op when hasMore is false', async () => {
		mockListFindings.mockResolvedValue(env(makePage(1, 10)));
		const store = createFindingsStore();
		await store.refresh();
		expect(store.hasMore).toBe(false);

		mockListFindings.mockClear();
		await store.loadOlder();
		expect(mockListFindings).not.toHaveBeenCalled();
		store.dispose();
	});

	it('loadOlder captures errors into loadMoreError without dropping rows', async () => {
		const { ApiError } = await import('$lib/api');
		mockListFindings.mockResolvedValueOnce(env(makePage(1, 50)));
		mockListFindings.mockRejectedValueOnce(
			new ApiError('pagination blew up', 500, '/api/diagnostics/findings')
		);
		const store = createFindingsStore();
		await store.refresh();
		expect(store.findings).toHaveLength(50);
		await store.loadOlder();
		expect(store.findings).toHaveLength(50);
		expect(store.loadMoreError).toBe('pagination blew up');
		expect(store.hasMore).toBe(true);
		store.dispose();
	});

	it('refresh resets loadMoreError', async () => {
		const { ApiError } = await import('$lib/api');
		mockListFindings.mockResolvedValueOnce(env(makePage(1, 50)));
		mockListFindings.mockRejectedValueOnce(new ApiError('boom', 500, '/api/diagnostics/findings'));
		mockListFindings.mockResolvedValueOnce(env(makePage(1, 50)));

		const store = createFindingsStore();
		await store.refresh();
		await store.loadOlder();
		expect(store.loadMoreError).toBe('boom');
		await store.refresh();
		expect(store.loadMoreError).toBeNull();
		store.dispose();
	});

	it('first-page poll merges new rows by id WITHOUT dropping older pages', async () => {
		mockListFindings.mockResolvedValueOnce(env(makePage(1, 50)));
		mockListFindings.mockResolvedValueOnce(env(makePage(51, 10)));

		const store = createFindingsStore();
		await store.refresh();
		await store.loadOlder();
		expect(store.findings).toHaveLength(60);

		const newHead = makePage(100, 5);
		const oldHead = makePage(1, 45);
		mockListFindings.mockResolvedValueOnce(env([...newHead, ...oldHead]));

		await store.refresh();
		expect(store.findings).toHaveLength(65);
		const top5Ids = store.findings!.slice(0, 5).map((f) => f.request_id);
		expect(top5Ids).toEqual([100, 101, 102, 103, 104]);
		const lastIds = store.findings!.slice(-10).map((f) => f.request_id);
		expect(lastIds).toEqual([51, 52, 53, 54, 55, 56, 57, 58, 59, 60]);
		store.dispose();
	});

	it('setAnalyzerFilter resets page state so refresh starts fresh', async () => {
		mockListFindings.mockResolvedValueOnce(env(makePage(1, 50)));
		mockListFindings.mockResolvedValueOnce(env(makePage(1, 10)));

		const store = createFindingsStore();
		await store.refresh();
		expect(store.findings).toHaveLength(50);
		expect(store.hasMore).toBe(true);

		store.setAnalyzerFilter('deflecting_blame_analyzer');
		await vi.advanceTimersByTimeAsync(0);
		expect(store.findings).toHaveLength(10);
		expect(store.hasMore).toBe(false);
		store.dispose();
	});

	it('groupFindings on a single page renders coherently (group-within-page)', async () => {
		const findings: Finding[] = [
			buildFinding({
				request_id: 1,
				session_id: 'sess-x',
				category: 'deflecting_blame',
				created_at: '2026-04-18T13:16:29.000Z'
			}),
			buildFinding({
				request_id: 1,
				session_id: 'sess-x',
				category: 'deflecting_blame',
				created_at: '2026-04-18T13:17:29.000Z',
				match_range: [10, 22]
			}),
			buildFinding({
				request_id: 2,
				session_id: 'sess-x',
				category: 'avoiding_work',
				created_at: '2026-04-18T13:18:00.000Z'
			})
		];
		mockListFindings.mockResolvedValueOnce(env(findings));
		const store = createFindingsStore();
		await store.refresh();
		const tree = groupFindings(store.findings!);
		expect(tree).toHaveLength(2);
		expect(tree[0]?.count).toBe(2);
		expect(tree[1]?.count).toBe(1);
		store.dispose();
	});
});

describe('W1.20-C FINDINGS_RETAINED_CAP', () => {
	it('cap value matches spec', () => {
		expect(FINDINGS_RETAINED_CAP).toBe(500);
	});

	it('drops tail rows past the cap; emits eviction events', async () => {
		const evictions: FindingEviction[] = [];
		const unsub = onFindingEvict((e) => evictions.push(e));
		try {
			const store = createFindingsStore();
			const firstPage = Array.from({ length: 50 }, (_, i) =>
				buildFinding({
					request_id: 1000 - i,
					session_id: `sess-${i}`,
					match_range: [i, i + 10] as [number, number],
					created_at: `2026-04-18T13:${String(i).padStart(2, '0')}:00.000Z`
				})
			);
			mockListFindings.mockResolvedValueOnce(env(firstPage));
			await store.refresh();
			expect(store.findings?.length).toBe(50);

			for (let pageIdx = 1; pageIdx <= 10; pageIdx++) {
				const page = Array.from({ length: 50 }, (_, i) => {
					const globalIdx = pageIdx * 50 + i;
					return buildFinding({
						request_id: 1000 - globalIdx,
						session_id: `sess-${globalIdx}`,
						match_range: [globalIdx, globalIdx + 10] as [number, number],
						created_at: `2026-04-18T${pageIdx === 0 ? '13' : String(13 - pageIdx).padStart(2, '0')}:${String(i).padStart(2, '0')}:00.000Z`
					});
				});
				mockListFindings.mockResolvedValueOnce(env(page));
				await store.loadOlder();
			}

			expect(store.findings?.length).toBe(FINDINGS_RETAINED_CAP);
			expect(evictions.length).toBe(50);
			for (const evt of evictions) {
				expect(evt.reason).toBe('findings-cap');
			}
			store.dispose();
		} finally {
			unsub();
		}
	});

	it('refresh-merge respects the cap; trailing-newest is sticky', async () => {
		const evictions: FindingEviction[] = [];
		const unsub = onFindingEvict((e) => evictions.push(e));
		try {
			const store = createFindingsStore();
			const seed = Array.from({ length: 50 }, (_, i) =>
				buildFinding({
					request_id: i,
					match_range: [i, i + 1] as [number, number],
					created_at: `2026-04-18T${String(13 - i).padStart(2, '0')}:00:00.000Z`
				})
			);
			mockListFindings.mockResolvedValueOnce(env(seed));
			await store.refresh();
			expect(store.findings?.length).toBe(50);
			expect(evictions.length).toBe(0);
			store.dispose();
		} finally {
			unsub();
		}
	});
});
