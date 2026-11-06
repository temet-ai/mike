/**
 * Retry timing for a panel's load after it fails.
 *
 * A panel loads once per subject (session, project). When the load fails it
 * calls `failed(retry)`: the retry runs on its own after each delay in turn,
 * then stops, and the panel waits for its Retry button. Nothing here loops.
 *
 * - `succeeded()` after a load works, so a later failure starts from the first delay.
 * - `reset()` when the user presses Retry or the subject changes.
 * - `cancel()` when the panel unmounts: `$effect(() => backoff.cancel)`.
 */
export const RETRY_DELAYS_MS: readonly number[] = [2000, 4000, 8000];

export interface RetryBackoff {
	failed(retry: () => void): void;
	succeeded(): void;
	reset(): void;
	cancel(): void;
}

export function createRetryBackoff(delays: readonly number[] = RETRY_DELAYS_MS): RetryBackoff {
	let failures = 0;
	let timer: ReturnType<typeof setTimeout> | undefined;

	function cancel(): void {
		clearTimeout(timer);
		timer = undefined;
	}

	return {
		failed(retry) {
			cancel();
			const delay = delays[failures];
			failures += 1;
			if (delay === undefined) return;
			timer = setTimeout(() => {
				timer = undefined;
				retry();
			}, delay);
		},
		succeeded() {
			failures = 0;
			cancel();
		},
		reset() {
			failures = 0;
			cancel();
		},
		cancel
	};
}
