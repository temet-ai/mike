import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import type { DiagnosticStats } from '$lib/types';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		getDiagnosticsStats: vi.fn()
	};
});

import { getDiagnosticsStats } from '$lib/api';
import { createDiagnosticsStore } from './diagnostics.svelte';

const mockStats = vi.mocked(getDiagnosticsStats);

function makeStats(overrides: Partial<DiagnosticStats> = {}): DiagnosticStats {
	return {
		totals_by_category: {
			justifying_shortcuts: 3,
			deflecting_blame: 1
		},
		totals_by_analyzer: {
			justifying_shortcuts_analyzer: 3,
			deflecting_blame_analyzer: 1
		},
		last_24h: 4,
		...overrides
	};
}

beforeEach(() => {
	vi.useFakeTimers();
	mockStats.mockReset();
	mockStats.mockResolvedValue(makeStats());
});

afterEach(() => {
	vi.useRealTimers();
});

describe('refresh', () => {
	it('populates stats from the API', async () => {
		mockStats.mockResolvedValue(
			makeStats({
				totals_by_category: { overselling: 7 },
				totals_by_analyzer: { overselling_analyzer: 7 },
				last_24h: 7
			})
		);
		const store = createDiagnosticsStore();
		await store.refresh();
		expect(store.stats).not.toBeNull();
		expect(store.stats?.totals_by_category.overselling).toBe(7);
		expect(store.stats?.last_24h).toBe(7);
		expect(store.error).toBeNull();
		expect(store.lastFetchedAt).not.toBeNull();
	});

	it('dedupes overlapping refresh() calls into one fetch', async () => {
		mockStats.mockResolvedValue(makeStats());
		const store = createDiagnosticsStore();
		const a = store.refresh();
		const b = store.refresh();
		await Promise.all([a, b]);
		expect(mockStats).toHaveBeenCalledTimes(1);
	});

	it('captures ApiError into `error` and leaves stats untouched', async () => {
		const { ApiError } = await import('$lib/api');
		mockStats.mockRejectedValue(new ApiError('boom', 500, '/api/diagnostics/stats'));
		const store = createDiagnosticsStore();
		await store.refresh();
		expect(store.error).toBe('boom');
		expect(store.stats).toBeNull();
	});
});

describe('handleEvent', () => {
	it('DiagnosticReady triggers refresh', async () => {
		const store = createDiagnosticsStore();
		await store.refresh();
		expect(mockStats).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'DiagnosticReady', id: 42, count: 3 });
		await vi.advanceTimersByTimeAsync(0);
		expect(mockStats).toHaveBeenCalledTimes(2);
	});

	it('other events are ignored (no refresh scheduled)', async () => {
		const store = createDiagnosticsStore();
		await store.refresh();
		expect(mockStats).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'TextDelta', id: 1, text: 'hi' });
		store.handleEvent({ type: 'ThinkingDelta', id: 1, thinking: 't' });
		store.handleEvent({
			type: 'ResponseComplete',
			id: 1,
			stop_reason: 'end_turn',
			usage: null
		});
		store.handleEvent({ type: 'ResponseError', id: 1, error: 'nope' });
		store.handleEvent({ type: 'Lagged', skipped: 99 });

		await vi.advanceTimersByTimeAsync(0);
		expect(mockStats).toHaveBeenCalledTimes(1);
	});
});

describe('polling', () => {
	it('startPolling fires refresh at the configured interval', async () => {
		const store = createDiagnosticsStore();
		store.startPolling(1_000);
		expect(mockStats).toHaveBeenCalledTimes(0);

		await vi.advanceTimersByTimeAsync(1_000);
		expect(mockStats).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(1_000);
		expect(mockStats).toHaveBeenCalledTimes(2);

		await vi.advanceTimersByTimeAsync(1_000);
		expect(mockStats).toHaveBeenCalledTimes(3);

		store.stopPolling();
	});

	it('stopPolling cancels the timer', async () => {
		const store = createDiagnosticsStore();
		store.startPolling(500);
		await vi.advanceTimersByTimeAsync(500);
		expect(mockStats).toHaveBeenCalledTimes(1);

		store.stopPolling();
		await vi.advanceTimersByTimeAsync(5_000);
		expect(mockStats).toHaveBeenCalledTimes(1);
	});

	it('multiple startPolling calls coalesce into a single timer', async () => {
		const store = createDiagnosticsStore();
		store.startPolling(1_000);
		store.startPolling(1_000);
		store.startPolling(1_000);

		await vi.advanceTimersByTimeAsync(1_000);
		expect(mockStats).toHaveBeenCalledTimes(1);

		store.stopPolling();
	});
});
