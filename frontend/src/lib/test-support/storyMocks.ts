import type {
	SessionDetail,
	SessionSummary,
	RequestDetail,
	ProxyEvent,
	OperatorTurnDto,
	UnifiedTextMatch
} from '$lib/types';
import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
import type { SessionListStore } from '$lib/stores/sessions.svelte';

export interface MockDetailStoreOverrides {
	detail?: SessionDetail | null;
	loading?: boolean;
	loadingOlder?: boolean;
	error?: string | null;
	loadOlderError?: string | null;
	notFound?: boolean;
	hasOlder?: boolean;
	olderCeilingReached?: boolean;
	currentId?: string | null;
	detailById?: Map<number, RequestDetail>;
	operatorTurns?: OperatorTurnDto[];
	promptKindFilter?: string | null;
}

export function buildMockDetailStore(overrides: MockDetailStoreOverrides = {}): SessionDetailStore {
	const detail = overrides.detail !== undefined ? overrides.detail : null;
	const detailById = overrides.detailById ?? new Map<number, RequestDetail>();
	return {
		get detail() {
			return detail;
		},
		get loading() {
			return overrides.loading ?? false;
		},
		get loadingOlder() {
			return overrides.loadingOlder ?? false;
		},
		get error() {
			return overrides.error ?? null;
		},
		get loadOlderError() {
			return overrides.loadOlderError ?? null;
		},
		get notFound() {
			return overrides.notFound ?? false;
		},
		get hasOlder() {
			return overrides.hasOlder ?? false;
		},
		get olderCeilingReached() {
			return overrides.olderCeilingReached ?? false;
		},
		get currentId() {
			return overrides.currentId !== undefined ? overrides.currentId : (detail?.summary.id ?? null);
		},
		get details() {
			return detailById;
		},
		get detailErrors() {
			return new Set<number>();
		},
		load: async () => {},
		refresh: async () => {},
		loadOlder: async () => {},
		loadAll: async () => {},
		loadDetail: async () => {},
		cancelDetail: () => {},
		predecessorOf: () => null,
		get operatorTurns() {
			return overrides.operatorTurns ?? [];
		},
		get promptKindFilter() {
			return overrides.promptKindFilter ?? 'operator';
		},
		refreshOperatorTurns: () => {},
		setPromptKindFilter: () => {},
		handleEvent: () => {}
	};
}

export interface MockSessionListStoreOverrides {
	sessions?: SessionSummary[];
	loading?: boolean;
	loadingMore?: boolean;
	error?: string | null;
	loadMoreError?: string | null;
	hasMore?: boolean;
	liveIds?: string[];
}

export function buildMockSessionListStore(
	overrides: MockSessionListStoreOverrides = {}
): SessionListStore {
	const liveIds = new Set(overrides.liveIds ?? []);
	return {
		get sessions() {
			return overrides.sessions ?? [];
		},
		get loading() {
			return overrides.loading ?? false;
		},
		get loadingMore() {
			return overrides.loadingMore ?? false;
		},
		get error() {
			return overrides.error ?? null;
		},
		get loadMoreError() {
			return overrides.loadMoreError ?? null;
		},
		get hasMore() {
			return overrides.hasMore ?? false;
		},
		get __wsTrackingSize() {
			return 0;
		},
		get searchHits() {
			return [] as SessionSummary[];
		},
		get searchLoading() {
			return false;
		},
		get searchError() {
			return null;
		},
		get bodyHits() {
			return [] as UnifiedTextMatch[];
		},
		get bodySearchLoading() {
			return false;
		},
		get bodySearchError() {
			return null;
		},
		get bodyFullMatchCount() {
			return 0;
		},
		get bodyMatchesTruncated() {
			return false;
		},
		get bodyWindowNote() {
			return null;
		},
		get contentHits() {
			return [];
		},
		get contentSearchLoading() {
			return false;
		},
		get contentSearchError() {
			return null;
		},
		get contentTotalMatches() {
			return 0;
		},
		get contentTotalSessions() {
			return 0;
		},
		get contentHasMore() {
			return false;
		},
		get hydratedSessions() {
			return [];
		},
		get hydratingSessions() {
			return false;
		},
		get hydrationError() {
			return null;
		},
		searchSessionById: (_sessionId: string) => null,
		setSearch(_q: string) {},
		refresh: async () => {},
		loadMore: async () => {},
		loadMoreForProject: async () => {},
		isProjectLoadingMore: () => false,
		projectLoadMoreError: () => null,
		isProjectExhausted: () => false,
		togglePin: async () => {},
		toggleProjectPin: async () => {},
		handleEvent: (_event: ProxyEvent) => {},
		latestWsEventForSession: (_sessionId: string): ProxyEvent | null => null,
		observeStatus: () => {},
		isRecentlyActive: (sessionId: string, _now: number) => liveIds.has(sessionId),
		registerSessionRequests: () => {},
		dispose: () => {}
	};
}
