import type { Finding, ProxyEvent } from '../types';
import { listFindings, ApiError } from '$lib/api';
import { PAGE_SIZE } from '../stores/findings.svelte';
import type { FindingEviction } from '../stores/findings.svelte';

export {
	DEFAULT_POLL_MS,
	PAGE_SIZE,
	FINDINGS_RETAINED_CAP,
	onFindingEvict,
	groupFindings
} from '../stores/findings.svelte';
export type {
	FindingEviction,
	RequestGroup,
	SessionGroup,
	CategoryGroup
} from '../stores/findings.svelte';

type FindingEvictionListener = (event: FindingEviction) => void;
const findingEvictionListeners = new Set<FindingEvictionListener>();

export function emitFindingEviction(event: FindingEviction): void {
	for (const listener of findingEvictionListeners) {
		try {
			listener(event);
		} catch (err) {
			console.warn('[findings-stub] eviction listener threw', err);
		}
	}
}

export interface FindingsStore {
	readonly findings: Finding[] | null;
	readonly analyzerFilter: string | null;
	readonly sessionFilter: string | null;
	readonly total: number | null;
	readonly loading: boolean;
	readonly error: string | null;
	readonly loadingMore: boolean;
	readonly hasMore: boolean;
	readonly loadMoreError: string | null;
	readonly lastFetchedAt: number | null;
	refresh(): Promise<void>;
	loadOlder(): Promise<void>;
	handleEvent(event: ProxyEvent): void;
	startPolling(intervalMs?: number): void;
	stopPolling(): void;
	setAnalyzerFilter(analyzer: string | null): void;
	setSessionFilter(sessionId: string | null): void;
	dispose(): void;
}

export interface FindingsStoreStub extends FindingsStore {
	__state: {
		findings: Finding[] | null;
		analyzerFilter: string | null;
		sessionFilter: string | null;
		total: number | null;
		loading: boolean;
		error: string | null;
		loadingMore: boolean;
		hasMore: boolean;
		loadMoreError: string | null;
		lastFetchedAt: number | null;
	};
}

export function createFindingsStore(): FindingsStoreStub {
	const __state = $state({
		findings: null as Finding[] | null,
		analyzerFilter: null as string | null,
		sessionFilter: null as string | null,
		total: null as number | null,
		loading: false,
		error: null as string | null,
		loadingMore: false,
		hasMore: true,
		loadMoreError: null as string | null,
		lastFetchedAt: null as number | null
	});

	async function refresh(): Promise<void> {
		__state.loading = true;
		const analyzer = __state.analyzerFilter;
		try {
			const params: Parameters<typeof listFindings>[0] = {
				limit: PAGE_SIZE
			};
			if (analyzer !== null) params.analyzer = analyzer;
			const fetched = await listFindings(params);
			if (__state.analyzerFilter !== analyzer) return;
			__state.findings = fetched;
			__state.hasMore = fetched.length >= PAGE_SIZE;
			__state.error = null;
			__state.lastFetchedAt = Date.now();
		} catch (err) {
			__state.error = err instanceof ApiError ? err.message : String(err);
		} finally {
			__state.loading = false;
		}
	}

	async function loadOlder(): Promise<void> {
		if (__state.loadingMore) return;
		__state.loadingMore = true;
		__state.loadMoreError = null;
		try {
			const params: Parameters<typeof listFindings>[0] = {
				limit: PAGE_SIZE
			};
			if (__state.analyzerFilter !== null) {
				params.analyzer = __state.analyzerFilter;
			}
			const more = await listFindings(params);
			__state.findings = [...(__state.findings ?? []), ...more];
			__state.hasMore = more.length >= PAGE_SIZE;
		} catch (err) {
			__state.loadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			__state.loadingMore = false;
		}
	}

	function setAnalyzerFilter(analyzer: string | null): void {
		__state.analyzerFilter = analyzer;
		void refresh();
	}

	function setSessionFilter(sessionId: string | null): void {
		__state.sessionFilter = sessionId;
		void refresh();
	}

	return {
		__state,
		get findings() {
			return __state.findings;
		},
		get analyzerFilter() {
			return __state.analyzerFilter;
		},
		get sessionFilter() {
			return __state.sessionFilter;
		},
		get total() {
			return __state.total;
		},
		get loading() {
			return __state.loading;
		},
		get error() {
			return __state.error;
		},
		get loadingMore() {
			return __state.loadingMore;
		},
		get hasMore() {
			return __state.hasMore;
		},
		get loadMoreError() {
			return __state.loadMoreError;
		},
		get lastFetchedAt() {
			return __state.lastFetchedAt;
		},
		refresh,
		loadOlder,
		handleEvent: () => {},
		startPolling: () => {},
		stopPolling: () => {},
		setAnalyzerFilter,
		setSessionFilter,
		dispose: () => {}
	};
}
