import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRetryBackoff } from './retryBackoff';

describe('createRetryBackoff', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('retries after each delay in turn, then stops', () => {
		const backoff = createRetryBackoff([2000, 4000, 8000]);
		const retry = vi.fn(() => backoff.failed(retry));
		backoff.failed(retry);

		vi.advanceTimersByTime(1999);
		expect(retry).toHaveBeenCalledTimes(0);
		vi.advanceTimersByTime(1);
		expect(retry).toHaveBeenCalledTimes(1);
		vi.advanceTimersByTime(4000);
		expect(retry).toHaveBeenCalledTimes(2);
		vi.advanceTimersByTime(8000);
		expect(retry).toHaveBeenCalledTimes(3);
		vi.advanceTimersByTime(60_000);
		expect(retry).toHaveBeenCalledTimes(3);
	});

	it('reset starts again from the first delay', () => {
		const backoff = createRetryBackoff([2000, 4000]);
		const retry = vi.fn();
		backoff.failed(retry);
		vi.advanceTimersByTime(2000);
		backoff.failed(retry);
		backoff.reset();
		vi.advanceTimersByTime(4000);
		expect(retry).toHaveBeenCalledTimes(1);

		backoff.failed(retry);
		vi.advanceTimersByTime(2000);
		expect(retry).toHaveBeenCalledTimes(2);
	});

	it('cancel drops a pending retry', () => {
		const backoff = createRetryBackoff([2000]);
		const retry = vi.fn();
		backoff.failed(retry);
		backoff.cancel();
		vi.advanceTimersByTime(10_000);
		expect(retry).not.toHaveBeenCalled();
	});

	it('succeeded clears the count so the next failure waits the first delay', () => {
		const backoff = createRetryBackoff([2000, 4000]);
		const retry = vi.fn();
		backoff.failed(retry);
		vi.advanceTimersByTime(2000);
		backoff.succeeded();
		backoff.failed(retry);
		vi.advanceTimersByTime(2000);
		expect(retry).toHaveBeenCalledTimes(2);
	});
});
