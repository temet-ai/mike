import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ToolUseDto, ToolUsesResponse } from '$lib/types';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return { ApiError, listSessionToolUses: vi.fn() };
});

import { listSessionToolUses } from '$lib/api';
import {
	loadSessionToolUses,
	invalidateSessionToolUses,
	resetSessionToolUsesCache
} from './sessionToolUses.svelte';

const mockList = vi.mocked(listSessionToolUses);

const SID = 'sess-tooluse';

function row(id: number): ToolUseDto {
	return {
		id,
		session_id: SID,
		request_id: 1000 + id,
		tool_name: 'Read',
		file_path: `/tmp/f${id}.ts`
	} as unknown as ToolUseDto;
}

function page(offset: number, count: number, total: number): ToolUsesResponse {
	return {
		session_id: SID,
		total_count: total,
		offset,
		limit: 1000,
		rows: Array.from({ length: count }, (_, i) => row(offset + i))
	};
}

beforeEach(() => {
	resetSessionToolUsesCache();
	mockList.mockReset();
});

describe('loadSessionToolUses', () => {
	it('serves concurrent callers from ONE request', async () => {
		mockList.mockResolvedValue(page(0, 3, 3));

		const [a, b, c] = await Promise.all([
			loadSessionToolUses(SID),
			loadSessionToolUses(SID),
			loadSessionToolUses(SID)
		]);

		expect(mockList).toHaveBeenCalledTimes(1);
		expect(a).toHaveLength(3);
		expect(b).toBe(a);
		expect(c).toBe(a);
	});

	it('serves a completed load from cache without re-fetching', async () => {
		mockList.mockResolvedValue(page(0, 2, 2));

		await loadSessionToolUses(SID);
		await loadSessionToolUses(SID);

		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('walks every page and returns all rows', async () => {
		mockList
			.mockResolvedValueOnce(page(0, 1000, 1500))
			.mockResolvedValueOnce(page(1000, 500, 1500));

		const rows = await loadSessionToolUses(SID);

		expect(mockList).toHaveBeenCalledTimes(2);
		expect(rows).toHaveLength(1500);
		expect(mockList).toHaveBeenNthCalledWith(1, SID, { limit: 1000, order: 'asc' });
		expect(mockList).toHaveBeenNthCalledWith(2, SID, { offset: 1000, limit: 1000, order: 'asc' });
	});

	it('stops on an empty page even when total_count overstates the rows', async () => {
		mockList.mockResolvedValueOnce(page(0, 2, 99)).mockResolvedValueOnce(page(2, 0, 99));

		const rows = await loadSessionToolUses(SID);

		expect(rows).toHaveLength(2);
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('retries after a failure issue a NEW request, never the dead promise', async () => {
		mockList.mockRejectedValueOnce(new Error('boom'));

		await expect(loadSessionToolUses(SID)).rejects.toThrow('boom');
		const callsAfterFailure = mockList.mock.calls.length;

		mockList.mockResolvedValueOnce(page(0, 4, 4));
		const rows = await loadSessionToolUses(SID);

		expect(mockList.mock.calls.length).toBeGreaterThan(callsAfterFailure);
		expect(rows).toHaveLength(4);
	});

	it('does not cache a failure', async () => {
		mockList.mockRejectedValueOnce(new Error('first'));
		await expect(loadSessionToolUses(SID)).rejects.toThrow('first');

		mockList.mockRejectedValueOnce(new Error('second'));
		await expect(loadSessionToolUses(SID)).rejects.toThrow('second');
	});

	it('rejects every joined caller, then lets the next one re-fetch', async () => {
		mockList.mockRejectedValueOnce(new Error('shared boom'));

		const results = await Promise.allSettled([loadSessionToolUses(SID), loadSessionToolUses(SID)]);
		expect(results.every((r) => r.status === 'rejected')).toBe(true);
		expect(mockList).toHaveBeenCalledTimes(1);

		mockList.mockResolvedValueOnce(page(0, 1, 1));
		await expect(loadSessionToolUses(SID)).resolves.toHaveLength(1);
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('keys by session, so two sessions do not share rows', async () => {
		mockList.mockResolvedValueOnce(page(0, 2, 2)).mockResolvedValueOnce(page(0, 5, 5));

		const a = await loadSessionToolUses('sess-a');
		const b = await loadSessionToolUses('sess-b');

		expect(a).toHaveLength(2);
		expect(b).toHaveLength(5);
		expect(mockList).toHaveBeenCalledTimes(2);
	});
});

describe('invalidateSessionToolUses', () => {
	it('forces the next load to re-fetch', async () => {
		mockList.mockResolvedValue(page(0, 2, 2));
		await loadSessionToolUses(SID);
		expect(mockList).toHaveBeenCalledTimes(1);

		invalidateSessionToolUses(SID);
		await loadSessionToolUses(SID);

		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('leaves other sessions cached', async () => {
		mockList.mockResolvedValue(page(0, 1, 1));
		await loadSessionToolUses('sess-a');
		await loadSessionToolUses('sess-b');
		expect(mockList).toHaveBeenCalledTimes(2);

		invalidateSessionToolUses('sess-a');
		await loadSessionToolUses('sess-b');

		expect(mockList).toHaveBeenCalledTimes(2);
	});
});
