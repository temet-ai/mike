import { describe, it, expect, vi, afterEach } from 'vitest';
import {
	ApiError,
	isRetryableError,
	withRetry,
	listFilterRules,
	__setRetryDelaysForTesting,
	__resetRetryDelaysForTesting
} from './api';
import { __resetApiCacheForTests } from './stores/cache/apiCache.svelte';
import type { FilterRuleDto } from './types';

__setRetryDelaysForTesting([0, 0, 0]);

afterEach(() => {
	__resetRetryDelaysForTesting();
	__setRetryDelaysForTesting([0, 0, 0]);
});

describe('isRetryableError', () => {
	it('returns true for TypeError (network / CORS / offline)', () => {
		expect(isRetryableError(new TypeError('Failed to fetch'))).toBe(true);
	});

	it.each([500, 502, 503, 504])('returns true for ApiError %d (server error)', (status) => {
		expect(isRetryableError(new ApiError('err', status, '/'))).toBe(true);
	});

	it('returns true for ApiError 429 (rate limited)', () => {
		expect(isRetryableError(new ApiError('rate limited', 429, '/'))).toBe(true);
	});

	it('returns true for ApiError 408 (request timeout)', () => {
		expect(isRetryableError(new ApiError('timeout', 408, '/'))).toBe(true);
	});

	it.each([400, 401, 403, 404])(
		'returns false for ApiError %d (permanent client error)',
		(status) => {
			expect(isRetryableError(new ApiError('err', status, '/'))).toBe(false);
		}
	);

	it('returns false for a plain Error (not ApiError, not TypeError)', () => {
		expect(isRetryableError(new Error('generic'))).toBe(false);
	});

	it('returns false for non-Error values', () => {
		expect(isRetryableError('string')).toBe(false);
		expect(isRetryableError(null)).toBe(false);
		expect(isRetryableError(42)).toBe(false);
	});
});

describe('withRetry', () => {
	it('resolves immediately when fn succeeds on the first attempt', async () => {
		const fn = vi.fn().mockResolvedValue('ok');
		await expect(withRetry(fn)).resolves.toBe('ok');
		expect(fn).toHaveBeenCalledTimes(1);
	});

	it('retries once on a retryable failure then resolves — fn called 2×', async () => {
		let calls = 0;
		const fn = vi.fn(async () => {
			calls++;
			if (calls === 1) throw new TypeError('Failed to fetch');
			return 'recovered';
		});
		await expect(withRetry(fn)).resolves.toBe('recovered');
		expect(fn).toHaveBeenCalledTimes(2);
	});

	it('retries twice on retryable failures then resolves — fn called 3×', async () => {
		let calls = 0;
		const fn = vi.fn(async () => {
			calls++;
			if (calls < 3) throw new ApiError('server error', 503, '/');
			return 'finally ok';
		});
		await expect(withRetry(fn)).resolves.toBe('finally ok');
		expect(fn).toHaveBeenCalledTimes(3);
	});

	it('throws after exhausting all retries — fn called 4× (initial + 3 retries)', async () => {
		const fn = vi.fn(async () => {
			throw new ApiError('gateway error', 502, '/api/requests/1');
		});
		await expect(withRetry(fn)).rejects.toThrow('gateway error');
		expect(fn).toHaveBeenCalledTimes(4);
	});

	it('throws on the FIRST attempt for a 404 — fn called 1× (not retried)', async () => {
		const fn = vi.fn(async () => {
			throw new ApiError('not found', 404, '/api/requests/999');
		});
		await expect(withRetry(fn)).rejects.toThrow('not found');
		expect(fn).toHaveBeenCalledTimes(1);
	});

	it('throws on the FIRST attempt for a 400 (bad request, not retried)', async () => {
		const fn = vi.fn(async () => {
			throw new ApiError('bad request', 400, '/api/foo');
		});
		await expect(withRetry(fn)).rejects.toThrow('bad request');
		expect(fn).toHaveBeenCalledTimes(1);
	});

	it('throws on the FIRST attempt for a 401 (unauthorised, not retried)', async () => {
		const fn = vi.fn(async () => {
			throw new ApiError('unauthorised', 401, '/api/foo');
		});
		await expect(withRetry(fn)).rejects.toThrow('unauthorised');
		expect(fn).toHaveBeenCalledTimes(1);
	});
});

describe('listFilterRules', () => {
	function rule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'r1',
			kind: 'text_substring',
			value: 'x',
			action: 'drop',
			text: null,
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-04-17T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 0,
			fires_7d: 0,
			last_fired_at: null,
			...overrides
		};
	}

	afterEach(() => {
		__resetApiCacheForTests();
	});

	it('hits the network on every call — never serves a cached/stale response', async () => {
		const fetchMock = vi
			.fn()
			.mockResolvedValueOnce(
				new Response(JSON.stringify([rule({ title: null })]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				})
			)
			.mockResolvedValueOnce(
				new Response(JSON.stringify([rule({ title: 'Removes the pronoun guidance' })]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				})
			);
		vi.stubGlobal('fetch', fetchMock);

		const first = await listFilterRules();
		expect(first[0]?.title).toBeNull();

		// A second call sees a title that changed out of band (e.g. a migration backfill, or an
		// MCP `update_filter_rule` call) with no cache-invalidation hook of its own — this must
		// still reach the network rather than replaying the first response.
		const second = await listFilterRules();
		expect(second[0]?.title).toBe('Removes the pronoun guidance');
		expect(fetchMock).toHaveBeenCalledTimes(2);

		vi.unstubAllGlobals();
	});
});
