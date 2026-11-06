import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestDetail } from '$lib/types';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return { ApiError, getRequest: vi.fn() };
});

import { getRequest } from '$lib/api';
import { clearRequestDetailCache, fetchRequestDetails } from './agentUsageFetch';

const mockGetRequest = vi.mocked(getRequest);

function detail(id: number): RequestDetail {
	return { id } as unknown as RequestDetail;
}

beforeEach(() => {
	mockGetRequest.mockReset();
	clearRequestDetailCache();
});

describe('fetchRequestDetails', () => {
	it('fetches every requested id and returns their details', async () => {
		mockGetRequest.mockImplementation((id) => Promise.resolve(detail(id)));
		const results = await fetchRequestDetails([1, 2, 3], () => false);
		expect(results.map((r) => r.id).sort((a, b) => a - b)).toEqual([1, 2, 3]);
		expect(mockGetRequest).toHaveBeenCalledTimes(3);
	});

	it('never runs more than the concurrency cap of requests at once', async () => {
		let inFlight = 0;
		let maxInFlight = 0;
		mockGetRequest.mockImplementation(async (id) => {
			inFlight += 1;
			maxInFlight = Math.max(maxInFlight, inFlight);
			await Promise.resolve();
			inFlight -= 1;
			return detail(id);
		});
		const ids = Array.from({ length: 20 }, (_, i) => i + 1);
		await fetchRequestDetails(ids, () => false);
		expect(maxInFlight).toBeLessThanOrEqual(6);
	});

	it('caches a request id across calls, never re-fetching it', async () => {
		mockGetRequest.mockImplementation((id) => Promise.resolve(detail(id)));
		await fetchRequestDetails([1, 2], () => false);
		await fetchRequestDetails([1, 2, 3], () => false);
		expect(mockGetRequest).toHaveBeenCalledTimes(3);
	});

	it('stops issuing new fetches once the caller reports staleness', async () => {
		let releaseGate = (): void => {};
		const gate = new Promise<void>((resolve) => {
			releaseGate = resolve;
		});
		mockGetRequest.mockImplementation(async (id) => {
			await gate;
			return detail(id);
		});
		let stale = false;
		const ids = Array.from({ length: 12 }, (_, i) => i + 1);
		const promise = fetchRequestDetails(ids, () => stale);
		// Let the first wave of workers (bounded by the concurrency cap) reach
		// the gate and issue their fetch before anything resolves.
		await Promise.resolve();
		await Promise.resolve();
		expect(mockGetRequest.mock.calls.length).toBe(6);

		stale = true;
		releaseGate();
		await promise;

		// The first wave was already in flight and completes, but no worker
		// dequeues a second id once staleness is reported.
		expect(mockGetRequest.mock.calls.length).toBe(6);
	});
});
