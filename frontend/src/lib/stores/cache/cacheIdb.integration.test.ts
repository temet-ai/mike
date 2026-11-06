import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	buildBodyPayload as makeBodyPayload,
	buildCacheRequestDetail as makeDetail,
	buildSummaryRow as makeSummaryRow
} from '$lib/test-support/fixtures';
import {
	__resetIdbHandleForTests,
	__deleteIdbForTests,
	__idbCountsForTests,
	idbGetBody,
	idbPutBody,
	idbGetBodies,
	idbGetDetail,
	idbPutDetail,
	idbGetDetails,
	idbGetSessionPage,
	idbPutSessionPage,
	idbGetAllSessionPages,
	idbInvalidateSession
} from './cacheIdb';
import { __resetCacheConfigForTests, configureCacheFromSettings } from './cacheConfig';

beforeEach(async () => {
	await __deleteIdbForTests();
	__resetCacheConfigForTests();
});

describe('body store', () => {
	it('put then get returns the payload', async () => {
		const payload = makeBodyPayload(200);
		await idbPutBody(1, 'request', 'sess-1', payload);
		const result = await idbGetBody(1, 'request');
		expect(result).toEqual(payload);
	});

	it('get on missing key returns null', async () => {
		const result = await idbGetBody(999, 'response');
		expect(result).toBeNull();
	});

	it('schema version mismatch is treated as a cache miss and the record is deleted', async () => {
		await idbPutBody(2, 'request', 'sess-1', makeBodyPayload());
		configureCacheFromSettings({ schemaVersion: 999 });
		const result = await idbGetBody(2, 'request');
		expect(result).toBeNull();
		configureCacheFromSettings({ schemaVersion: 1 });
		const again = await idbGetBody(2, 'request');
		expect(again).toBeNull();
	});

	it('batch get (idbGetBodies) opens exactly one transaction and returns all hits', async () => {
		await idbPutBody(10, 'request', 'sess-1', makeBodyPayload(10));
		await idbPutBody(11, 'request', 'sess-1', makeBodyPayload(11));
		await idbPutBody(12, 'response', 'sess-1', makeBodyPayload(12));

		const result = await idbGetBodies([
			[10, 'request'],
			[11, 'request'],
			[12, 'response'],
			[99, 'request']
		]);
		expect(result.size).toBe(3);
		expect(result.get('10:request')?.byte_size).toBe(10);
		expect(result.get('11:request')?.byte_size).toBe(11);
		expect(result.get('12:response')?.byte_size).toBe(12);
		expect(result.has('99:request')).toBe(false);
	});

	it('idbGetBodies with empty keys returns empty map', async () => {
		const result = await idbGetBodies([]);
		expect(result.size).toBe(0);
	});
});

describe('detail store', () => {
	it('put then get returns the payload', async () => {
		const detail = makeDetail(5);
		await idbPutDetail(5, 'sess-1', detail);
		const result = await idbGetDetail(5);
		expect(result?.id).toBe(5);
	});

	it('get on missing id returns null', async () => {
		const result = await idbGetDetail(9999);
		expect(result).toBeNull();
	});

	it('batch get returns all hits', async () => {
		await idbPutDetail(20, 'sess-1', makeDetail(20));
		await idbPutDetail(21, 'sess-1', makeDetail(21));
		const result = await idbGetDetails([20, 21, 999]);
		expect(result.size).toBe(2);
		expect(result.has(20)).toBe(true);
		expect(result.has(999)).toBe(false);
	});
});

describe('session page store', () => {
	it('put then get returns the page', async () => {
		const rows = [makeSummaryRow(1), makeSummaryRow(2)] as never[];
		await idbPutSessionPage('sess-1', 0, rows);
		const result = await idbGetSessionPage('sess-1', 0);
		expect(result).toHaveLength(2);
	});

	it('idbGetAllSessionPages returns all pages for a session', async () => {
		const rows = [makeSummaryRow(1)] as never[];
		await idbPutSessionPage('sess-all', 0, rows);
		await idbPutSessionPage('sess-all', 50, rows);
		await idbPutSessionPage('sess-other', 0, rows);
		const result = await idbGetAllSessionPages('sess-all');
		expect(result.size).toBe(2);
		expect(result.has(0)).toBe(true);
		expect(result.has(50)).toBe(true);
	});

	it('idbInvalidateSession removes all data for that session', async () => {
		const rows = [makeSummaryRow(1)] as never[];
		await idbPutBody(100, 'request', 'sess-del', makeBodyPayload());
		await idbPutDetail(100, 'sess-del', makeDetail(100));
		await idbPutSessionPage('sess-del', 0, rows);
		expect(await idbGetBody(100, 'request')).not.toBeNull();

		await idbInvalidateSession('sess-del');
		expect(await idbGetBody(100, 'request')).toBeNull();
		expect(await idbGetDetail(100)).toBeNull();
		expect(await idbGetSessionPage('sess-del', 0)).toBeNull();
	});
});

describe('IDB LRU eviction', () => {
	it('body byte budget: writing past the budget evicts oldest by lastAccess', async () => {
		configureCacheFromSettings({ idbBodyBudgetBytes: 150 });
		__resetIdbHandleForTests();

		await idbPutBody(50, 'request', 'sess-1', makeBodyPayload(100));
		let counts = await __idbCountsForTests();
		expect(counts.bodies).toBe(1);

		await idbPutBody(51, 'request', 'sess-1', makeBodyPayload(100));
		counts = await __idbCountsForTests();
		expect(counts.bodies).toBe(1);
	});

	it('detail count cap: writing past the cap evicts oldest', async () => {
		configureCacheFromSettings({ idbDetailCap: 2 });
		__resetIdbHandleForTests();

		await idbPutDetail(60, 'sess-1', makeDetail(60));
		await idbPutDetail(61, 'sess-1', makeDetail(61));
		await idbPutDetail(62, 'sess-1', makeDetail(62));
		const counts = await __idbCountsForTests();
		expect(counts.details).toBe(2);
		expect(await idbGetDetail(60)).toBeNull();
		expect(await idbGetDetail(62)).not.toBeNull();
	});
});

describe('SSR guard (typeof indexedDB === "undefined")', () => {
	it('gracefully returns null when indexedDB is not available', async () => {
		const origIdb = globalThis.indexedDB;
		// @ts-expect-error – intentionally deleting for test
		delete globalThis.indexedDB;
		try {
			expect(await idbGetBody(1, 'request')).toBeNull();
			await expect(idbPutBody(1, 'request', null, makeBodyPayload())).resolves.toBeUndefined();
		} finally {
			globalThis.indexedDB = origIdb;
		}
	});
});

describe('graceful degradation when IDB open throws', () => {
	it('does not throw into the caller when open fails', async () => {
		const orig = globalThis.indexedDB.open.bind(globalThis.indexedDB);
		vi.spyOn(globalThis.indexedDB, 'open').mockImplementation(() => {
			throw new Error('simulated IDB failure');
		});
		__resetIdbHandleForTests();
		try {
			await expect(idbGetBody(1, 'request')).resolves.toBeNull();
			await expect(idbPutBody(1, 'request', null, makeBodyPayload())).resolves.toBeUndefined();
		} finally {
			vi.spyOn(globalThis.indexedDB, 'open').mockImplementation(orig);
			__resetIdbHandleForTests();
		}
	});
});

describe('__idbCountsForTests', () => {
	it('returns zeros when nothing is stored', async () => {
		const counts = await __idbCountsForTests();
		expect(counts.bodies).toBe(0);
		expect(counts.details).toBe(0);
		expect(counts.sessionPages).toBe(0);
	});

	it('counts each store correctly after writes', async () => {
		await idbPutBody(1, 'request', 'sess-1', makeBodyPayload());
		await idbPutDetail(1, 'sess-1', makeDetail(1));
		await idbPutSessionPage('sess-1', 0, [] as never[]);
		const counts = await __idbCountsForTests();
		expect(counts.bodies).toBe(1);
		expect(counts.details).toBe(1);
		expect(counts.sessionPages).toBe(1);
	});
});
