import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	buildBodyPayload as makeBodyPayload,
	buildCacheRequestDetail as makeDetail,
	buildSummaryRow as makeSummaryRow
} from '$lib/test-support/fixtures';
import {
	__resetCacheForTests,
	__cacheSizesForTests,
	peekBody,
	loadBody,
	batchWarmBodies,
	peekDetail,
	loadDetail,
	putDetailMemoryOnly,
	peekSessionPage,
	loadSessionPage,
	batchWarmSessionPages,
	invalidateSession,
	onCacheEviction
} from './unifiedCache.svelte';
import { __resetIdbHandleForTests, __deleteIdbForTests, __idbCountsForTests } from './cacheIdb';
import * as cacheIdb from './cacheIdb';
import { __resetCacheConfigForTests, configureCacheFromSettings } from './cacheConfig';
import { __resetMemoryEvictionListenersForTests } from './cacheMemory';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		getRequestBody: vi.fn(),
		getResponseBody: vi.fn(),
		getRequestSentBody: vi.fn()
	};
});

import { getRequestBody, getResponseBody, getRequestSentBody } from '$lib/api';

const mockGetRequestBody = vi.mocked(getRequestBody);
const mockGetResponseBody = vi.mocked(getResponseBody);
const mockGetRequestSentBody = vi.mocked(getRequestSentBody);

beforeEach(async () => {
	__resetCacheForTests();
	await __deleteIdbForTests();
	__resetCacheConfigForTests();
	__resetMemoryEvictionListenersForTests();
	vi.clearAllMocks();
});

describe('peekBody', () => {
	it('returns undefined on miss', () => {
		expect(peekBody(1, 'request')).toBeUndefined();
	});

	it('returns the value after loadBody resolves', async () => {
		mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload());
		await loadBody(1, 'request');
		expect(peekBody(1, 'request')).not.toBeUndefined();
	});
});

describe('loadBody read-through', () => {
	it('memory hit: does not call the network', async () => {
		mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload(10));
		await loadBody(1, 'request');
		vi.clearAllMocks();
		const result = await loadBody(1, 'request');
		expect(mockGetRequestBody).not.toHaveBeenCalled();
		expect(result.byte_size).toBe(10);
	});

	it('IDB hit after memory reset: does not call the network', async () => {
		mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload(20));
		await loadBody(1, 'request', undefined, 'sess-1');
		__resetCacheForTests();
		__resetIdbHandleForTests();
		vi.clearAllMocks();

		const result = await loadBody(1, 'request');
		expect(mockGetRequestBody).not.toHaveBeenCalled();
		expect(result.byte_size).toBe(20);
	});

	it('network fetch stores result in memory AND IDB for request kind', async () => {
		mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload(30));
		await loadBody(1, 'request', undefined, 'sess-1');
		expect((await __idbCountsForTests()).bodies).toBe(1);
	});

	it('Streaming response body NOT written to IDB', async () => {
		mockGetResponseBody.mockResolvedValueOnce(makeBodyPayload(40));
		await loadBody(1, 'response', 'Streaming', 'sess-1');
		expect((await __idbCountsForTests()).bodies).toBe(0);
	});

	it('Complete response body IS written to IDB', async () => {
		mockGetResponseBody.mockResolvedValueOnce(makeBodyPayload(50));
		await loadBody(1, 'response', 'Complete', 'sess-1');
		expect((await __idbCountsForTests()).bodies).toBe(1);
	});

	it('coalesces concurrent fetches for the same key into one network call', async () => {
		mockGetRequestBody.mockResolvedValue(makeBodyPayload());
		await Promise.all([loadBody(2, 'request'), loadBody(2, 'request'), loadBody(2, 'request')]);
		expect(mockGetRequestBody).toHaveBeenCalledTimes(1);
	});
});

describe('loadBody IDB timeout fallback', () => {
	it('returns the network payload when idbGetBody hangs past 1500ms', async () => {
		const spy = vi.spyOn(cacheIdb, 'idbGetBody').mockReturnValue(new Promise(() => {}));
		vi.useFakeTimers();

		const expected = makeBodyPayload(99);
		mockGetRequestBody.mockResolvedValueOnce(expected);

		try {
			const bodyPromise = loadBody(99, 'request');

			await vi.advanceTimersByTimeAsync(1600);

			const result = await bodyPromise;
			expect(result.byte_size).toBe(99);
			expect(mockGetRequestBody).toHaveBeenCalledTimes(1);
		} finally {
			vi.useRealTimers();
			spy.mockRestore();
		}
	});
});

describe('batchWarmBodies', () => {
	it('populates memory from IDB in one go, making peekBody succeed', async () => {
		mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload(60));
		mockGetRequestSentBody.mockResolvedValueOnce(makeBodyPayload(61));
		await loadBody(5, 'request', undefined, 'sess-1');
		await loadBody(5, 'request_sent', undefined, 'sess-1');

		__resetCacheForTests();
		__resetIdbHandleForTests();
		vi.clearAllMocks();

		await batchWarmBodies('sess-1', [5], ['request', 'request_sent']);
		expect(mockGetRequestBody).not.toHaveBeenCalled();
		expect(peekBody(5, 'request')?.byte_size).toBe(60);
		expect(peekBody(5, 'request_sent')?.byte_size).toBe(61);
	});

	it('skips keys already in memory', async () => {
		mockGetRequestBody.mockResolvedValue(makeBodyPayload(70));
		await loadBody(6, 'request');
		vi.clearAllMocks();
		await batchWarmBodies('sess-1', [6], ['request']);
		expect(mockGetRequestBody).not.toHaveBeenCalled();
	});
});

describe('loadDetail', () => {
	it('memory hit: does not call the fetcher', async () => {
		const fetcher = vi.fn().mockResolvedValue(makeDetail(10));
		await loadDetail(10, fetcher);
		vi.clearAllMocks();
		await loadDetail(10, fetcher);
		expect(fetcher).not.toHaveBeenCalled();
	});

	it('persistence across simulated reload: IDB → memory', async () => {
		const fetcher = vi.fn().mockResolvedValue(makeDetail(13, 'Complete'));
		await loadDetail(13, fetcher);
		await new Promise((r) => setTimeout(r, 50));
		__resetCacheForTests();
		__resetIdbHandleForTests();
		fetcher.mockClear();

		const result = await loadDetail(13, fetcher);
		expect(fetcher).not.toHaveBeenCalled();
		expect(result?.id).toBe(13);
	});

	it('coalesces concurrent fetches for the same id into one network call', async () => {
		const fetcher = vi.fn().mockResolvedValue(makeDetail(14, 'Complete'));
		await Promise.all([loadDetail(14, fetcher), loadDetail(14, fetcher), loadDetail(14, fetcher)]);
		expect(fetcher).toHaveBeenCalledTimes(1);
	});
});

describe('putDetailMemoryOnly', () => {
	it('makes peekDetail succeed without a network call', () => {
		const detail = makeDetail(20, 'Streaming');
		putDetailMemoryOnly(detail);
		expect(peekDetail(20)).toBeDefined();
	});
});

describe('loadSessionPage', () => {
	const rows = [makeSummaryRow(1), makeSummaryRow(2)] as never[];

	it('memory hit: does not call the fetcher', async () => {
		const fetcher = vi.fn().mockResolvedValue(rows);
		await loadSessionPage('sess-a', 0, fetcher);
		fetcher.mockClear();
		await loadSessionPage('sess-a', 0, fetcher);
		expect(fetcher).not.toHaveBeenCalled();
	});

	it('IDB hit after memory reset: does not call the fetcher', async () => {
		const fetcher = vi.fn().mockResolvedValue(rows);
		await loadSessionPage('sess-b', 0, fetcher);
		__resetCacheForTests();
		__resetIdbHandleForTests();
		fetcher.mockClear();
		const result = await loadSessionPage('sess-b', 0, fetcher);
		expect(fetcher).not.toHaveBeenCalled();
		expect(result).toHaveLength(2);
	});

	it('coalesces concurrent fetches for the same key into one fetcher call', async () => {
		const fetcher = vi.fn().mockResolvedValue(rows);
		await Promise.all([
			loadSessionPage('sess-z', 0, fetcher),
			loadSessionPage('sess-z', 0, fetcher),
			loadSessionPage('sess-z', 0, fetcher)
		]);
		expect(fetcher).toHaveBeenCalledTimes(1);
	});

	describe('partial pages (live tail) are never treated as cacheable', () => {
		const short = [makeSummaryRow(1), makeSummaryRow(2)] as never[];
		const whole = [makeSummaryRow(1), makeSummaryRow(2), makeSummaryRow(3)] as never[];

		it('re-runs the fetcher on a memory-cached page shorter than pageSize', async () => {
			const fetcher = vi.fn().mockResolvedValue(short);
			await loadSessionPage('tail-mem', 0, fetcher, 3);
			fetcher.mockClear();
			await loadSessionPage('tail-mem', 0, fetcher, 3);
			expect(fetcher).toHaveBeenCalledTimes(1);
		});

		it('does not persist a page shorter than pageSize to IDB', async () => {
			const fetcher = vi.fn().mockResolvedValue(short);
			await loadSessionPage('tail-idb', 0, fetcher, 3);
			expect(await cacheIdb.idbGetSessionPage('tail-idb', 0)).toBeNull();
		});

		it('still caches a page that fills pageSize', async () => {
			const fetcher = vi.fn().mockResolvedValue(whole);
			await loadSessionPage('whole', 0, fetcher, 3);
			__resetCacheForTests();
			__resetIdbHandleForTests();
			fetcher.mockClear();
			const result = await loadSessionPage('whole', 0, fetcher, 3);
			expect(fetcher).not.toHaveBeenCalled();
			expect(result).toHaveLength(3);
		});

		it('surfaces rows appended after a partial page was first read', async () => {
			const fetcher = vi.fn().mockResolvedValueOnce(short).mockResolvedValueOnce(whole);
			const first = await loadSessionPage('grew', 0, fetcher, 3);
			expect(first).toHaveLength(2);
			__resetCacheForTests();
			__resetIdbHandleForTests();
			const second = await loadSessionPage('grew', 0, fetcher, 3);
			expect(second).toHaveLength(3);
		});

		it('evicts and refetches a short page already stored by an older build', async () => {
			await cacheIdb.idbPutSessionPage('poisoned', 50, short);
			__resetCacheForTests();
			__resetIdbHandleForTests();
			const fetcher = vi.fn().mockResolvedValue(whole);
			const result = await loadSessionPage('poisoned', 50, fetcher, 3);
			expect(fetcher).toHaveBeenCalledTimes(1);
			expect(result).toHaveLength(3);
			expect(await cacheIdb.idbGetSessionPage('poisoned', 50)).toHaveLength(3);
		});
	});
});

describe('batchWarmSessionPages', () => {
	it('populates all pages for a session from IDB', async () => {
		const rows = [makeSummaryRow(1)] as never[];
		const fetcher = vi.fn().mockResolvedValue(rows);
		await loadSessionPage('sess-c', 0, fetcher);
		await loadSessionPage('sess-c', 50, fetcher);
		__resetCacheForTests();
		__resetIdbHandleForTests();
		fetcher.mockClear();

		await batchWarmSessionPages('sess-c');
		expect(peekSessionPage('sess-c', 0)).toBeDefined();
		expect(peekSessionPage('sess-c', 50)).toBeDefined();
		expect(fetcher).not.toHaveBeenCalled();
	});
});

describe('invalidateSession', () => {
	it('clears memory and IDB for the named session', async () => {
		const rows = [makeSummaryRow(1)] as never[];
		const fetcher = vi.fn().mockResolvedValue(rows);
		await loadSessionPage('sess-d', 0, fetcher);
		await new Promise((r) => setTimeout(r, 20));
		await invalidateSession('sess-d');
		expect(peekSessionPage('sess-d', 0)).toBeUndefined();
		const counts = await __idbCountsForTests();
		expect(counts.sessionPages).toBe(0);
	});
});

describe('onCacheEviction', () => {
	it('fires memory-body eviction when byte cap is exceeded', async () => {
		configureCacheFromSettings({ bodyMemoryByteCap: 150, bodyMemoryEntryCap: 1000 });
		__resetCacheForTests();
		const events: { source: string }[] = [];
		const unsub = onCacheEviction((e) => events.push(e));
		try {
			mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload(100));
			mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload(100));
			await loadBody(30, 'request');
			await loadBody(31, 'request');
			expect(events.some((e) => e.source === 'memory-body')).toBe(true);
		} finally {
			unsub();
		}
	});

	it('unsubscribe stops future events', async () => {
		const events: unknown[] = [];
		const unsub = onCacheEviction((e) => events.push(e));
		unsub();
		mockGetRequestBody.mockResolvedValueOnce(makeBodyPayload());
		await loadBody(40, 'request');
		expect(events).toHaveLength(0);
	});
});

describe('__cacheSizesForTests', () => {
	it('returns zeros on a fresh reset', () => {
		expect(__cacheSizesForTests()).toEqual({ bodies: 0, details: 0, sessionPages: 0 });
	});

	it('counts entries correctly after loads', async () => {
		mockGetRequestBody.mockResolvedValue(makeBodyPayload());
		await loadBody(50, 'request');
		putDetailMemoryOnly(makeDetail(50));
		const sizes = __cacheSizesForTests();
		expect(sizes.bodies).toBe(1);
		expect(sizes.details).toBe(1);
	});
});
