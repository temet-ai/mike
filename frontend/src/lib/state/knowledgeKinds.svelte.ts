import { getKnowledgeKinds, type KnowledgeKind } from '$lib/api/knowledgeKinds';
import { DEFAULT_POLL_MS } from '$lib/state/memory.svelte';

export interface KnowledgeKindsStore {
	readonly snapshot: KnowledgeKind[] | null;
	readonly error: string | null;
	readonly lastFetchedAt: number | null;
	refresh(): Promise<void>;
	/** Puts a row the API just answered with into the snapshot without waiting for a poll. */
	apply(row: KnowledgeKind): void;
	remove(kind: string): void;
	start(intervalMs?: number): () => void;
}

interface MutableState {
	snapshot: KnowledgeKind[] | null;
	error: string | null;
	lastFetchedAt: number | null;
}

/**
 * Polled list of knowledge kinds. A poll replaces row data; the table sorts on `kind`, so a
 * poll never changes the order. `initial` seeds the snapshot from the page load.
 */
export function createKnowledgeKindsStore(
	initial: KnowledgeKind[] | null = null
): KnowledgeKindsStore {
	const s = $state<MutableState>({
		snapshot: initial,
		error: null,
		lastFetchedAt: initial === null ? null : Date.now()
	});
	let inFlight: Promise<void> | null = null;
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	function refresh(): Promise<void> {
		if (inFlight) return inFlight;
		const promise = (async (): Promise<void> => {
			try {
				s.snapshot = await getKnowledgeKinds();
				s.error = null;
				s.lastFetchedAt = Date.now();
			} catch (err) {
				s.error = err instanceof Error ? err.message : String(err);
			} finally {
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
		get error() {
			return s.error;
		},
		get lastFetchedAt() {
			return s.lastFetchedAt;
		},
		refresh,
		apply(row) {
			s.snapshot = (s.snapshot ?? []).map((k) => (k.kind === row.kind ? row : k));
		},
		remove(kind) {
			s.snapshot = (s.snapshot ?? []).filter((k) => k.kind !== kind);
		},
		start
	};
}
