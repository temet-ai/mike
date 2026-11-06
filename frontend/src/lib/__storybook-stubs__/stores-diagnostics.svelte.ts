import type { DiagnosticStats, ProxyEvent } from '../types';
import { getDiagnosticsStats, ApiError } from '$lib/api';

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

export interface DiagnosticsStoreStub extends DiagnosticsStore {
	__state: {
		stats: DiagnosticStats | null;
		loading: boolean;
		error: string | null;
		lastFetchedAt: number | null;
	};
}

export function createDiagnosticsStore(): DiagnosticsStoreStub {
	const __state = $state({
		stats: null as DiagnosticStats | null,
		loading: false,
		error: null as string | null,
		lastFetchedAt: null as number | null
	});

	async function refresh(): Promise<void> {
		__state.loading = true;
		try {
			const fetched = await getDiagnosticsStats();
			__state.stats = fetched;
			__state.error = null;
			__state.lastFetchedAt = Date.now();
		} catch (err) {
			__state.error = err instanceof ApiError ? err.message : String(err);
		} finally {
			__state.loading = false;
		}
	}

	return {
		__state,
		get stats() {
			return __state.stats;
		},
		get loading() {
			return __state.loading;
		},
		get error() {
			return __state.error;
		},
		get lastFetchedAt() {
			return __state.lastFetchedAt;
		},
		refresh,
		handleEvent: () => {},
		startPolling: () => {},
		stopPolling: () => {}
	};
}
