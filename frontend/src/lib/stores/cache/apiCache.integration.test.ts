import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	cachedGet,
	policyForPath,
	onApiCacheUpdate,
	invalidateApiCache,
	__expireApiCacheFreshnessForTests,
	freshnessWindowMs,
	__resetApiCacheForTests,
	__apiCacheMemorySizeForTests
} from './apiCache.svelte';
import { __resetIdbHandleForTests, __deleteIdbForTests, __idbCountsForTests } from './cacheIdb';
import * as cacheIdb from './cacheIdb';
import { __resetCacheConfigForTests } from './cacheConfig';
import { __resetMemoryEvictionListenersForTests } from './cacheMemory';
import {
	getHealth,
	listSessions,
	__setRetryDelaysForTesting,
	__resetRetryDelaysForTesting
} from '$lib/api';
import { FIRST_PAGE_PATH, PAGE_SIZE } from '$lib/stores/sessions.svelte';
import { buildHealthResponse } from '$lib/test-support/healthFixtures';

beforeEach(async () => {
	__resetApiCacheForTests();
	await __deleteIdbForTests();
	__resetCacheConfigForTests();
	__resetMemoryEvictionListenersForTests();
});

describe('policyForPath', () => {
	it('classifies request bodies + detail as passthrough (timeline-owned)', () => {
		expect(policyForPath('/api/requests/42/body/request')).toBe('passthrough');
		expect(policyForPath('/api/requests/42/body/request_sent')).toBe('passthrough');
		expect(policyForPath('/api/requests/42/body/response')).toBe('passthrough');
		expect(policyForPath('/api/requests/42')).toBe('passthrough');
	});
	it('classifies immutable request input', () => {
		expect(policyForPath('/api/requests/42/stream_chunks')).toBe('immutable');
		expect(policyForPath('/api/sessions/abc-123/requests/42/deltas')).toBe('immutable');
	});
	it('classifies aggregates as swr', () => {
		expect(policyForPath('/api/sessions')).toBe('swr');
		expect(policyForPath('/api/sessions/abc?order=asc&offset=0&limit=20')).toBe('swr');
		expect(policyForPath('/api/sessions/abc/tool-uses')).toBe('swr');
		expect(policyForPath('/api/agent-dispatches')).toBe('swr');
		expect(policyForPath('/api/stats')).toBe('swr');
		expect(policyForPath('/api/settings')).toBe('swr');
	});
});

describe('cachedGet — passthrough', () => {
	it('does not cache passthrough paths (always calls the fetcher)', async () => {
		const fetcher = vi.fn().mockResolvedValue({ ok: 1 });
		await cachedGet('/api/requests/9/body/request', fetcher);
		await cachedGet('/api/requests/9/body/request', fetcher);
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(await __idbCountsForTests()).toMatchObject({ apiResponses: 0 });
	});
});

describe('cachedGet — swr', () => {
	it('memory hit: returns without re-fetching network on a second synchronous read', async () => {
		const fetcher = vi.fn().mockResolvedValue({ v: 1 });
		const first = await cachedGet('/api/stats', fetcher);
		expect(first).toEqual({ v: 1 });
		const second = await cachedGet('/api/stats', fetcher);
		expect(second).toEqual({ v: 1 });
		expect(__apiCacheMemorySizeForTests()).toBe(1);
	});

	it('persists to IDB and serves from IDB after a simulated reload', async () => {
		const fetcher = vi.fn().mockResolvedValue({ rows: [1, 2, 3] });
		await cachedGet('/api/sessions', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(await __idbCountsForTests()).toMatchObject({ apiResponses: 1 });

		__resetApiCacheForTests();
		__resetIdbHandleForTests();
		const reloadFetcher = vi.fn().mockResolvedValue({ rows: [1, 2, 3] });
		const afterReload = await cachedGet('/api/sessions', reloadFetcher);
		expect(afterReload).toEqual({ rows: [1, 2, 3] });
	});

	it('background revalidation fires onApiCacheUpdate when the value changes', async () => {
		let call = 0;
		const fetcher = vi.fn().mockImplementation(async () => ({ n: ++call }));
		await cachedGet('/api/stats', fetcher);
		const updates: unknown[] = [];
		onApiCacheUpdate('/api/stats', (v) => updates.push(v));
		__expireApiCacheFreshnessForTests();
		const v = await cachedGet('/api/stats', fetcher);
		expect(v).toEqual({ n: 1 });
		await new Promise((r) => setTimeout(r, 20));
		expect(updates).toEqual([{ n: 2 }]);
	});

	it('does NOT fire update when the revalidated value is unchanged', async () => {
		const fetcher = vi.fn().mockResolvedValue({ same: true });
		await cachedGet('/api/stats', fetcher);
		const updates: unknown[] = [];
		onApiCacheUpdate('/api/stats', (v) => updates.push(v));
		__expireApiCacheFreshnessForTests();
		await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(updates).toEqual([]);
	});

	it('coalesces concurrent cold reads into one network call', async () => {
		const fetcher = vi.fn().mockResolvedValue({ v: 1 });
		await Promise.all([
			cachedGet('/api/agents', fetcher),
			cachedGet('/api/agents', fetcher),
			cachedGet('/api/agents', fetcher)
		]);
		expect(fetcher).toHaveBeenCalledTimes(1);
	});
});

describe('cachedGet — an answer stored by an earlier page load', () => {
	const path = '/api/sessions?offset=0&limit=50';

	async function storeAtAge(target: string, value: unknown, ageMs: number): Promise<void> {
		const clock = vi.spyOn(Date, 'now').mockReturnValue(Date.now() - ageMs);
		try {
			await cachedGet(target, async () => value);
			await vi.waitFor(async () => {
				expect(await cacheIdb.idbGetApiResponse(target)).toEqual(value);
			});
		} finally {
			clock.mockRestore();
		}
		__resetApiCacheForTests();
		__resetIdbHandleForTests();
	}

	it('is not the answer once it is older than the freshness window: the network is', async () => {
		await storeAtAge(path, { rows: ['two months ago'] }, freshnessWindowMs(path) + 1_000);
		const fetcher = vi.fn().mockResolvedValue({ rows: ['today'] });
		await expect(cachedGet(path, fetcher)).resolves.toEqual({ rows: ['today'] });
		expect(fetcher).toHaveBeenCalledTimes(1);
	});

	it('does not stand in for a failed read once it is older than the freshness window', async () => {
		await storeAtAge(path, { rows: ['two months ago'] }, freshnessWindowMs(path) + 1_000);
		const fetcher = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
		await expect(cachedGet(path, fetcher)).rejects.toThrow('Failed to fetch');
	});

	it('is the answer while it is inside the freshness window', async () => {
		await storeAtAge(path, { rows: ['a moment ago'] }, 1_000);
		const fetcher = vi.fn().mockResolvedValue({ rows: ['a moment ago'] });
		await expect(cachedGet(path, fetcher)).resolves.toEqual({ rows: ['a moment ago'] });
	});

	it('is the answer at any age when the path never changes', async () => {
		const immutable = '/api/requests/7/stream_chunks';
		await storeAtAge(immutable, { chunks: 3 }, 90 * 24 * 60 * 60 * 1000);
		const fetcher = vi.fn().mockResolvedValue({ chunks: 99 });
		await expect(cachedGet(immutable, fetcher)).resolves.toEqual({ chunks: 3 });
		expect(fetcher).not.toHaveBeenCalled();
	});

	it('the session list reads its first page under the path its store listens on', async () => {
		const fetcher = vi.fn().mockResolvedValue(new Response('[]', { status: 200 }));
		vi.stubGlobal('fetch', fetcher);
		try {
			await listSessions({ offset: 0, limit: PAGE_SIZE });
			expect(fetcher).toHaveBeenCalledWith(FIRST_PAGE_PATH, expect.anything());
		} finally {
			vi.unstubAllGlobals();
		}
	});
});

describe('cachedGet — freshness window', () => {
	it('suppresses the background revalidation while the value is fresh', async () => {
		const fetcher = vi.fn().mockResolvedValue({ v: 1 });
		await cachedGet('/api/stats', fetcher);
		expect(fetcher).toHaveBeenCalledTimes(1);
		for (let i = 0; i < 10; i++) await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(fetcher).toHaveBeenCalledTimes(1);
	});

	it('revalidates again once the window has expired', async () => {
		const fetcher = vi.fn().mockResolvedValue({ v: 1 });
		await cachedGet('/api/stats', fetcher);
		expect(fetcher).toHaveBeenCalledTimes(1);
		__expireApiCacheFreshnessForTests();
		await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(fetcher).toHaveBeenCalledTimes(2);
	});

	it('a failed revalidation still opens the window — no refetch storm', async () => {
		let call = 0;
		const fetcher = vi.fn().mockImplementation(async () => {
			call++;
			if (call === 1) return { v: 1 };
			throw new Error('upstream down');
		});
		await cachedGet('/api/stats', fetcher);
		__expireApiCacheFreshnessForTests();
		await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(fetcher).toHaveBeenCalledTimes(2);
		for (let i = 0; i < 5; i++) await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(fetcher).toHaveBeenCalledTimes(2);
	});

	it('invalidation clears the freshness stamp so the next read refetches', async () => {
		const fetcher = vi.fn().mockResolvedValue({ v: 1 });
		await cachedGet('/api/stats', fetcher);
		expect(fetcher).toHaveBeenCalledTimes(1);
		await invalidateApiCache('/api/stats');
		await cachedGet('/api/stats', fetcher);
		expect(fetcher).toHaveBeenCalledTimes(2);
	});

	it('structural config gets a longer window than global counters', () => {
		expect(freshnessWindowMs('/api/sessions/abc/analyzers')).toBeGreaterThan(
			freshnessWindowMs('/api/stats')
		);
		expect(freshnessWindowMs('/api/settings')).toBeGreaterThan(freshnessWindowMs('/api/health'));
	});
});

describe('cachedGet — immutable', () => {
	it('caches forever and never revalidates', async () => {
		const fetcher = vi.fn().mockResolvedValue({ chunks: [] });
		await cachedGet('/api/requests/7/stream_chunks', fetcher);
		await cachedGet('/api/requests/7/stream_chunks', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(fetcher).toHaveBeenCalledTimes(1);
	});
});

describe('invalidateApiCache', () => {
	it('drops memory + IDB entries by prefix so the next read re-fetches', async () => {
		const fetcher = vi.fn().mockResolvedValue({ v: 1 });
		await cachedGet('/api/sessions', fetcher);
		await cachedGet('/api/sessions/abc/tool-uses', fetcher);
		await new Promise((r) => setTimeout(r, 20));
		expect(await __idbCountsForTests()).toMatchObject({ apiResponses: 2 });

		await invalidateApiCache('/api/sessions');
		expect(__apiCacheMemorySizeForTests()).toBe(0);
		expect(await __idbCountsForTests()).toMatchObject({ apiResponses: 0 });
	});
});

describe('onApiCacheUpdate — prefix subscriptions', () => {
	it('prefix listener fires when a path under the prefix revalidates with a changed value', async () => {
		let call = 0;
		const fetcher = vi.fn().mockImplementation(async () => ({ n: ++call }));
		await cachedGet('/api/sessions/abc/tool-uses', fetcher);

		const updates: unknown[] = [];
		onApiCacheUpdate('/api/sessions', (v) => updates.push(v), { prefix: true });

		__expireApiCacheFreshnessForTests();
		await cachedGet('/api/sessions/abc/tool-uses', fetcher);
		await new Promise((r) => setTimeout(r, 20));

		expect(updates).toEqual([{ n: 2 }]);
	});

	it('prefix listener does NOT fire for a path outside its prefix', async () => {
		let call = 0;
		const fetcher = vi.fn().mockImplementation(async () => ({ n: ++call }));
		await cachedGet('/api/stats', fetcher);

		const updates: unknown[] = [];
		onApiCacheUpdate('/api/sessions', (v) => updates.push(v), { prefix: true });

		await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));

		expect(updates).toEqual([]);
	});

	it('exact-path subscription still works (regression)', async () => {
		let call = 0;
		const fetcher = vi.fn().mockImplementation(async () => ({ n: ++call }));
		await cachedGet('/api/stats', fetcher);

		const updates: unknown[] = [];
		onApiCacheUpdate('/api/stats', (v) => updates.push(v));
		__expireApiCacheFreshnessForTests();
		await cachedGet('/api/stats', fetcher);
		await new Promise((r) => setTimeout(r, 20));

		expect(updates).toEqual([{ n: 2 }]);
	});

	it('unsubscribe stops further calls (prefix)', async () => {
		let call = 0;
		const fetcher = vi.fn().mockImplementation(async () => ({ n: ++call }));
		await cachedGet('/api/sessions/abc/tool-uses', fetcher);

		const updates: unknown[] = [];
		const unsub = onApiCacheUpdate('/api/sessions', (v) => updates.push(v), { prefix: true });
		unsub();

		await cachedGet('/api/sessions/abc/tool-uses', fetcher);
		await new Promise((r) => setTimeout(r, 20));

		expect(updates).toEqual([]);
	});
});

describe('graceful degradation', () => {
	it('returns the network value when IDB open throws (no cache, no throw)', async () => {
		const orig = globalThis.indexedDB.open.bind(globalThis.indexedDB);
		vi.spyOn(globalThis.indexedDB, 'open').mockImplementation(() => {
			throw new Error('simulated IDB failure');
		});
		__resetIdbHandleForTests();
		try {
			const fetcher = vi.fn().mockResolvedValue({ ok: true });
			await expect(cachedGet('/api/stats', fetcher)).resolves.toEqual({ ok: true });
		} finally {
			vi.spyOn(globalThis.indexedDB, 'open').mockImplementation(orig);
			__resetIdbHandleForTests();
		}
	});

	it('returns the network value when idbGetApiResponse hangs past 1500ms', async () => {
		const spy = vi.spyOn(cacheIdb, 'idbGetApiResponse').mockReturnValue(new Promise(() => {}));
		vi.useFakeTimers();

		const fetcher = vi.fn().mockResolvedValue({ stats: 'live' });

		try {
			const resultPromise = cachedGet('/api/stats', fetcher);

			await vi.advanceTimersByTimeAsync(1600);

			const result = await resultPromise;
			expect(result).toEqual({ stats: 'live' });
			expect(fetcher).toHaveBeenCalledTimes(1);
		} finally {
			vi.useRealTimers();
			spy.mockRestore();
		}
	});
});

describe('getHealth — fresh reachability through the real cache wrapper', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		__resetRetryDelaysForTesting();
	});

	it.each(['memory', 'persisted'] as const)(
		'rejects failed health reads instead of replaying a healthy %s cache entry',
		async (source) => {
			__setRetryDelaysForTesting([]);
			const healthy = buildHealthResponse();
			// Seed the cache as older clients did, including their persisted response.
			await cachedGet('/api/health', async () => healthy, 'swr');
			await vi.waitFor(async () => {
				expect(await cacheIdb.idbGetApiResponse('/api/health')).toEqual(healthy);
			});
			if (source === 'persisted') {
				__resetApiCacheForTests();
				__resetIdbHandleForTests();
			}
			const fetcher = vi
				.fn()
				.mockResolvedValueOnce(new Response(JSON.stringify(healthy), { status: 200 }))
				.mockRejectedValueOnce(new TypeError('Failed to fetch'));
			vi.stubGlobal('fetch', fetcher);
			await expect(getHealth()).resolves.toEqual(healthy);
			await expect(getHealth()).rejects.toThrow('Failed to fetch');
			expect(fetcher).toHaveBeenCalledTimes(2);
		}
	);
});
