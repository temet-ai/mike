import { getDiagnosticsStats, ApiError } from '$lib/api';
import type { DiagnosticStats, ProxyEvent } from '$lib/types';

export const DEFAULT_POLL_MS = 10_000;

export interface DiagnosticsStore {
	readonly stats: DiagnosticStats | null;
	readonly loading: boolean;
	readonly error: string | null;
	readonly lastFetchedAt: number | null;
	refresh(): Promise<void>;
	handleEvent(event: ProxyEvent): void;
	startPolling(intervalMs?: number): void;
	stopPolling(): void;
}

interface MutableState {
	stats: DiagnosticStats | null;
	loading: boolean;
	error: string | null;
	lastFetchedAt: number | null;
}

export function createDiagnosticsStore(): DiagnosticsStore {
	const s = $state<MutableState>({
		stats: null,
		loading: false,
		error: null,
		lastFetchedAt: null
	});

	let inFlight: Promise<void> | null = null;
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	async function refresh(): Promise<void> {
		if (inFlight) return inFlight;
		s.loading = true;
		const promise = (async (): Promise<void> => {
			try {
				const fetched = await getDiagnosticsStats();
				s.stats = fetched;
				s.error = null;
				s.lastFetchedAt = Date.now();
			} catch (err) {
				s.error = err instanceof ApiError ? err.message : String(err);
			} finally {
				s.loading = false;
				inFlight = null;
			}
		})();
		inFlight = promise;
		return promise;
	}

	function handleEvent(event: ProxyEvent): void {
		if (event.type === 'DiagnosticReady') {
			void refresh();
		}
	}

	function startPolling(intervalMs: number = DEFAULT_POLL_MS): void {
		if (pollTimer !== null) return;
		pollTimer = setInterval(() => {
			void refresh();
		}, intervalMs);
	}

	function stopPolling(): void {
		if (pollTimer !== null) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	return {
		get stats() {
			return s.stats;
		},
		get loading() {
			return s.loading;
		},
		get error() {
			return s.error;
		},
		get lastFetchedAt() {
			return s.lastFetchedAt;
		},
		refresh,
		handleEvent,
		startPolling,
		stopPolling
	};
}
