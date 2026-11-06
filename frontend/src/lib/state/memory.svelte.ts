import { getMemoryBreakdown, MemoryApiError, type MemoryBreakdown } from '$lib/api/memory';

export const DEFAULT_POLL_MS = 3_000;

export interface MemoryStore {
	readonly snapshot: MemoryBreakdown | null;
	readonly loading: boolean;
	readonly error: string | null;
	readonly lastFetchedAt: number | null;
	refresh(): Promise<void>;
	start(intervalMs?: number): () => void;
}

interface MutableState {
	snapshot: MemoryBreakdown | null;
	loading: boolean;
	error: string | null;
	lastFetchedAt: number | null;
}

export function createMemoryStore(top: number = 10): MemoryStore {
	const s = $state<MutableState>({
		snapshot: null,
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
				const fetched = await getMemoryBreakdown(top);
				s.snapshot = fetched;
				s.error = null;
				s.lastFetchedAt = Date.now();
			} catch (err) {
				s.error = err instanceof MemoryApiError ? err.message : String(err);
			} finally {
				s.loading = false;
				inFlight = null;
			}
		})();
		inFlight = promise;
		return promise;
	}

	function start(intervalMs: number = DEFAULT_POLL_MS): () => void {
		if (pollTimer === null) {
			void refresh();
			pollTimer = setInterval(() => {
				void refresh();
			}, intervalMs);
		}
		return () => {
			if (pollTimer !== null) {
				clearInterval(pollTimer);
				pollTimer = null;
			}
		};
	}

	return {
		get snapshot() {
			return s.snapshot;
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
		start
	};
}
