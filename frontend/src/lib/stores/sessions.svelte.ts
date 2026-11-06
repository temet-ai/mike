import {
	listSessions,
	patchSessionPin,
	patchProjectPin,
	findText,
	searchContent,
	ApiError
} from '$lib/api';
import type {
	ProxyEvent,
	SessionSummary,
	UnifiedTextMatch,
	ContentSearchSessionGroup
} from '$lib/types';
import type { StatusKind } from '$lib/status';

export const SESSION_SEARCH_MIN_QUERY = 4;

export const SESSION_BODY_SEARCH_MAX_MATCHES = 50;

export function looksLikeSessionId(query: string): boolean {
	const t = query.trim().toLowerCase();
	if (t.length === 0) return false;
	if (t.startsWith('hash-')) return true;
	if (/^[0-9a-f]{2,}(?:-[0-9a-f]+)+$/.test(t)) return true;
	if (/^[0-9a-f]{16,}$/.test(t)) return true;
	return false;
}

export const SESSION_SEARCH_DEBOUNCE_MS = 200;

export { SESSION_LIST_STORE_KEY } from '$lib/context-keys';

export const THROTTLE_MS = 500;

export const PAGE_SIZE = 50;

export const SESSION_LIST_CACHE_KEY = 'mike:session-list:v1';

export const RECENT_ACTIVE_WINDOW_MS = 30 * 60 * 1000;

export const LIVE_STICKY_GRACE_MS = 60_000;

export const LIVE_STREAMING_STALE_MS = 3 * 60 * 1000;

export interface SessionListStore {
	readonly sessions: SessionSummary[];
	readonly loading: boolean;
	readonly loadingMore: boolean;
	readonly error: string | null;
	readonly loadMoreError: string | null;
	readonly hasMore: boolean;
	readonly searchHits: SessionSummary[];
	readonly searchLoading: boolean;
	readonly searchError: string | null;
	readonly bodyHits: UnifiedTextMatch[];
	readonly bodySearchLoading: boolean;
	readonly bodySearchError: string | null;
	readonly bodyFullMatchCount: number;
	readonly bodyMatchesTruncated: boolean;
	readonly bodyWindowNote: string | null;
	readonly contentHits: ContentSearchSessionGroup[];
	readonly contentSearchLoading: boolean;
	readonly contentSearchError: string | null;
	readonly contentTotalMatches: number;
	readonly contentTotalSessions: number;
	readonly contentHasMore: boolean;
	readonly hydratedSessions: SessionSummary[];
	readonly hydratingSessions: boolean;
	readonly hydrationError: string | null;
	searchSessionById(sessionId: string): SessionSummary | null;
	setSearch(query: string): void;
	refresh(): Promise<void>;
	loadMore(): Promise<void>;
	loadMoreForProject(project: string): Promise<void>;
	isProjectLoadingMore(project: string): boolean;
	projectLoadMoreError(project: string): string | null;
	isProjectExhausted(project: string): boolean;
	togglePin(sessionId: string, next: boolean): Promise<void>;
	toggleProjectPin(path: string, next: boolean): Promise<void>;
	handleEvent(event: ProxyEvent): void;
	latestWsEventForSession(sessionId: string): ProxyEvent | null;
	observeStatus(sessionId: string, kind: StatusKind, now: number): void;
	isRecentlyActive(sessionId: string, now: number): boolean;
	registerSessionRequests(sessionId: string, requestIds: number[]): void;
	dispose(): void;
	readonly __wsTrackingSize: number;
}

interface MutableState {
	sessions: SessionSummary[];
	loading: boolean;
	loadingMore: boolean;
	error: string | null;
	loadMoreError: string | null;
	hasMore: boolean;
	searchHits: SessionSummary[];
	searchLoading: boolean;
	searchError: string | null;
	bodyHits: UnifiedTextMatch[];
	bodySearchLoading: boolean;
	bodySearchError: string | null;
	bodyFullMatchCount: number;
	bodyMatchesTruncated: boolean;
	bodyWindowNote: string | null;
	contentHits: ContentSearchSessionGroup[];
	contentSearchLoading: boolean;
	contentSearchError: string | null;
	contentTotalMatches: number;
	contentTotalSessions: number;
	contentHasMore: boolean;
	hydratedSessions: SessionSummary[];
	hydratingSessions: boolean;
	hydrationError: string | null;
	projectLoadingMore: Record<string, boolean>;
	projectLoadMoreError: Record<string, string | null>;
	projectExhausted: Record<string, boolean>;
}

function sameSummary(a: SessionSummary, b: SessionSummary): boolean {
	return (
		a.id === b.id &&
		a.project === b.project &&
		a.first_seen === b.first_seen &&
		a.last_seen === b.last_seen &&
		a.request_count === b.request_count &&
		a.last_status === b.last_status &&
		a.title === b.title &&
		a.pinned === b.pinned &&
		a.avg_latency_ms === b.avg_latency_ms &&
		a.models.length === b.models.length &&
		a.models.every((m, i) => m === b.models[i])
	);
}

function reconcile(prev: SessionSummary[], next: SessionSummary[]): SessionSummary[] {
	const byId = new Map(prev.map((sess) => [sess.id, sess]));
	return next.map((n) => {
		const old = byId.get(n.id);
		return old && sameSummary(old, n) ? old : n;
	});
}

function readCachedFirstPage(): SessionSummary[] {
	try {
		if (typeof localStorage === 'undefined') return [];
		const raw = localStorage.getItem(SESSION_LIST_CACHE_KEY);
		if (!raw) return [];
		const parsed: unknown = JSON.parse(raw);
		return Array.isArray(parsed) ? (parsed as SessionSummary[]) : [];
	} catch {
		return [];
	}
}

function writeCachedFirstPage(sessions: SessionSummary[]): void {
	try {
		if (typeof localStorage === 'undefined') return;
		localStorage.setItem(SESSION_LIST_CACHE_KEY, JSON.stringify(sessions.slice(0, PAGE_SIZE)));
	} catch {}
}

export function createSessionListStore(): SessionListStore {
	const cachedFirstPage = readCachedFirstPage();
	const s = $state<MutableState>({
		sessions: cachedFirstPage,
		loading: cachedFirstPage.length === 0,
		loadingMore: false,
		error: null,
		loadMoreError: null,
		hasMore: true,
		searchHits: [],
		searchLoading: false,
		searchError: null,
		bodyHits: [],
		bodySearchLoading: false,
		bodySearchError: null,
		bodyFullMatchCount: 0,
		bodyMatchesTruncated: false,
		bodyWindowNote: null,
		contentHits: [],
		contentSearchLoading: false,
		contentSearchError: null,
		contentTotalMatches: 0,
		contentTotalSessions: 0,
		contentHasMore: false,
		hydratedSessions: [],
		hydratingSessions: false,
		hydrationError: null,
		projectLoadingMore: {},
		projectLoadMoreError: {},
		projectExhausted: {}
	});

	let throttleTimer: ReturnType<typeof setTimeout> | null = null;
	let inFlight: Promise<void> | null = null;
	let loadMoreInFlight: Promise<void> | null = null;

	let searchTimer: ReturnType<typeof setTimeout> | null = null;
	let searchSeq = 0;

	function clearSearchTimer(): void {
		if (searchTimer !== null) {
			clearTimeout(searchTimer);
			searchTimer = null;
		}
	}

	function clearBodySearch(): void {
		s.bodyHits = [];
		s.bodySearchLoading = false;
		s.bodySearchError = null;
		s.bodyFullMatchCount = 0;
		s.bodyMatchesTruncated = false;
		s.bodyWindowNote = null;
	}

	function clearContentSearch(): void {
		s.contentHits = [];
		s.contentSearchLoading = false;
		s.contentSearchError = null;
		s.contentTotalMatches = 0;
		s.contentTotalSessions = 0;
		s.contentHasMore = false;
	}

	function clearHydration(): void {
		s.hydratedSessions = [];
		s.hydratingSessions = false;
		s.hydrationError = null;
	}

	function resetSearch(): void {
		clearSearchTimer();
		s.searchHits = [];
		s.searchLoading = false;
		s.searchError = null;
		clearBodySearch();
		clearContentSearch();
		clearHydration();
		searchSeq += 1;
	}

	async function runSessionSearch(rawQuery: string, seq: number): Promise<void> {
		try {
			const fetched = await listSessions({ search: rawQuery });
			if (seq !== searchSeq) return;
			s.searchHits = fetched;
			s.searchError = null;
		} catch (err) {
			if (seq !== searchSeq) return;
			s.searchError = err instanceof ApiError ? err.message : String(err);
			s.searchHits = [];
		} finally {
			if (seq === searchSeq) {
				s.searchLoading = false;
			}
		}
	}

	async function runBodySearch(rawQuery: string, seq: number): Promise<void> {
		try {
			const res = await findText({
				needle: rawQuery,
				source: 'response',
				max_matches: SESSION_BODY_SEARCH_MAX_MATCHES
			});
			if (seq !== searchSeq) return;
			s.bodyHits = res.matches;
			s.bodyFullMatchCount = res.full_match_count;
			s.bodyMatchesTruncated = res.matches_truncated;
			s.bodyWindowNote = res.window_note ?? null;
			s.bodySearchError = null;
		} catch (err) {
			if (seq !== searchSeq) return;
			s.bodySearchError = err instanceof ApiError ? err.message : String(err);
			s.bodyHits = [];
			s.bodyFullMatchCount = 0;
			s.bodyMatchesTruncated = false;
			s.bodyWindowNote = null;
		} finally {
			if (seq === searchSeq) {
				s.bodySearchLoading = false;
			}
		}
	}

	async function runContentSearch(rawQuery: string, seq: number): Promise<void> {
		try {
			const res = await searchContent({ q: rawQuery });
			if (seq !== searchSeq) return;
			s.contentHits = res.sessions;
			s.contentTotalMatches = res.total_matches;
			s.contentTotalSessions = res.total_sessions;
			s.contentHasMore = res.has_more;
			s.contentSearchError = null;
		} catch (err) {
			if (seq !== searchSeq) return;
			s.contentSearchError = err instanceof ApiError ? err.message : String(err);
			s.contentHits = [];
			s.contentTotalMatches = 0;
			s.contentTotalSessions = 0;
			s.contentHasMore = false;
		} finally {
			if (seq === searchSeq) {
				s.contentSearchLoading = false;
			}
		}
	}

	async function hydrateMissingSessions(seq: number): Promise<void> {
		const known = new Set<string>();
		for (const sess of s.sessions) known.add(sess.id);
		for (const sess of s.searchHits) known.add(sess.id);
		const needed = new Set<string>();
		for (const g of s.contentHits) {
			if (!known.has(g.session_id)) needed.add(g.session_id);
		}
		for (const m of s.bodyHits) {
			if (!known.has(m.session_id)) needed.add(m.session_id);
		}
		if (needed.size === 0) {
			s.hydratedSessions = [];
			s.hydratingSessions = false;
			return;
		}
		try {
			const fetched = await listSessions({ ids: [...needed] });
			if (seq !== searchSeq) return;
			s.hydratedSessions = fetched;
			s.hydrationError = null;
		} catch (err) {
			if (seq !== searchSeq) return;
			s.hydratedSessions = [];
			s.hydrationError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (seq === searchSeq) {
				s.hydratingSessions = false;
			}
		}
	}

	function searchSessionById(sessionId: string): SessionSummary | null {
		return (
			s.searchHits.find((sess) => sess.id === sessionId) ??
			s.sessions.find((sess) => sess.id === sessionId) ??
			s.hydratedSessions.find((sess) => sess.id === sessionId) ??
			null
		);
	}

	function setSearch(query: string): void {
		const q = query.trim();
		if (q.length < SESSION_SEARCH_MIN_QUERY) {
			resetSearch();
			return;
		}
		clearSearchTimer();
		searchTimer = setTimeout(() => {
			searchTimer = null;
			const seq = ++searchSeq;
			s.searchLoading = true;
			const sessionSearchDone = runSessionSearch(q, seq);
			let bodySearchDone: Promise<void> = Promise.resolve();
			let contentSearchDone: Promise<void> = Promise.resolve();
			if (looksLikeSessionId(q)) {
				clearBodySearch();
				clearContentSearch();
			} else {
				s.bodySearchLoading = true;
				bodySearchDone = runBodySearch(q, seq);
				s.contentSearchLoading = true;
				contentSearchDone = runContentSearch(q, seq);
			}
			s.hydratingSessions = true;
			void Promise.all([sessionSearchDone, bodySearchDone, contentSearchDone]).then(() => {
				if (seq !== searchSeq) return;
				void hydrateMissingSessions(seq);
			});
		}, SESSION_SEARCH_DEBOUNCE_MS);
	}

	const wsEventByRequestId = new Map<number, ProxyEvent>();
	const requestToSession = new Map<number, string>();
	const sessionToRequests = new Map<string, Set<number>>();

	const lastLiveAt = new Map<string, number>();

	function clearWsTracking(): void {
		wsEventByRequestId.clear();
	}

	function pruneWsTracking(currentSessionIds: Set<string>): void {
		for (const [sessionId, reqIds] of sessionToRequests) {
			if (!currentSessionIds.has(sessionId)) {
				for (const rid of reqIds) {
					requestToSession.delete(rid);
					wsEventByRequestId.delete(rid);
				}
				sessionToRequests.delete(sessionId);
			}
		}
	}

	async function refresh(): Promise<void> {
		if (inFlight) return inFlight;
		s.loading = true;
		const promise = (async (): Promise<void> => {
			try {
				const fetched = await listSessions({ offset: 0, limit: PAGE_SIZE });
				s.sessions = reconcile(s.sessions, fetched);
				writeCachedFirstPage(fetched);
				s.error = null;
				s.loadMoreError = null;
				s.hasMore = fetched.length >= PAGE_SIZE;
				clearWsTracking();
				const currentIds = new Set(fetched.map((sess) => sess.id));
				pruneWsTracking(currentIds);
				s.projectLoadingMore = {};
				s.projectLoadMoreError = {};
				s.projectExhausted = {};
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

	async function loadMore(): Promise<void> {
		if (loadMoreInFlight) return loadMoreInFlight;
		if (!s.hasMore || s.loading) return;
		s.loadingMore = true;
		const promise = (async (): Promise<void> => {
			try {
				const offset = s.sessions.length;
				const fetched = await listSessions({ offset, limit: PAGE_SIZE });
				const existingIds = new Set(s.sessions.map((sess) => sess.id));
				const appended = fetched.filter((sess) => !existingIds.has(sess.id));
				if (appended.length > 0) {
					s.sessions = [...s.sessions, ...appended];
				}
				s.hasMore = fetched.length >= PAGE_SIZE;
				s.loadMoreError = null;
			} catch (err) {
				s.loadMoreError = err instanceof ApiError ? err.message : String(err);
			} finally {
				s.loadingMore = false;
				loadMoreInFlight = null;
			}
		})();
		loadMoreInFlight = promise;
		return promise;
	}

	const projectLoadMoreInFlight = new Map<string, Promise<void>>();

	async function loadMoreForProject(project: string): Promise<void> {
		const existing = projectLoadMoreInFlight.get(project);
		if (existing) return existing;
		if (s.projectExhausted[project]) return;
		s.projectLoadingMore = { ...s.projectLoadingMore, [project]: true };
		s.projectLoadMoreError = { ...s.projectLoadMoreError, [project]: null };
		const promise = (async (): Promise<void> => {
			try {
				const offset = s.sessions.filter((sess) => sess.project === project).length;
				const fetched = await listSessions({ project, offset, limit: PAGE_SIZE });
				const existingIds = new Set(s.sessions.map((sess) => sess.id));
				const appended = fetched.filter((sess) => !existingIds.has(sess.id));
				if (appended.length > 0) {
					s.sessions = [...s.sessions, ...appended];
				}
				s.projectExhausted = { ...s.projectExhausted, [project]: fetched.length < PAGE_SIZE };
			} catch (err) {
				s.projectLoadMoreError = {
					...s.projectLoadMoreError,
					[project]: err instanceof ApiError ? err.message : String(err)
				};
			} finally {
				s.projectLoadingMore = { ...s.projectLoadingMore, [project]: false };
				projectLoadMoreInFlight.delete(project);
			}
		})();
		projectLoadMoreInFlight.set(project, promise);
		return promise;
	}

	async function togglePin(sessionId: string, next: boolean): Promise<void> {
		const idx = s.sessions.findIndex((sess) => sess.id === sessionId);
		if (idx === -1) return;
		const current = s.sessions[idx];
		if (!current) return;
		const previous = current.pinned;
		s.sessions = s.sessions.map((sess, i) => (i === idx ? { ...sess, pinned: next } : sess));
		try {
			const res = await patchSessionPin(sessionId, next);
			s.sessions = s.sessions.map((sess) =>
				sess.id === sessionId ? { ...sess, pinned: res.pinned } : sess
			);
		} catch (err) {
			s.sessions = s.sessions.map((sess) =>
				sess.id === sessionId ? { ...sess, pinned: previous } : sess
			);
			s.error = err instanceof ApiError ? err.message : String(err);
		}
	}

	async function toggleProjectPin(path: string, next: boolean): Promise<void> {
		const previousById = new Map<string, boolean | undefined>();
		for (const sess of s.sessions) {
			if (sess.project === path) previousById.set(sess.id, sess.pinned);
		}
		if (previousById.size === 0) return;
		s.sessions = s.sessions.map((sess) =>
			sess.project === path ? { ...sess, pinned: next } : sess
		);
		try {
			const res = await patchProjectPin(path, next);
			s.sessions = s.sessions.map((sess) =>
				sess.project === path ? { ...sess, pinned: res.pinned } : sess
			);
		} catch (err) {
			s.sessions = s.sessions.map((sess) =>
				previousById.has(sess.id) ? { ...sess, pinned: previousById.get(sess.id) } : sess
			);
			s.error = err instanceof ApiError ? err.message : String(err);
		}
	}

	function clearThrottle(): void {
		if (throttleTimer !== null) {
			clearTimeout(throttleTimer);
			throttleTimer = null;
		}
	}

	function scheduleThrottled(): void {
		clearThrottle();
		throttleTimer = setTimeout(() => {
			throttleTimer = null;
			void refresh();
		}, THROTTLE_MS);
	}

	function handleEvent(event: ProxyEvent): void {
		if (event.type === 'Lagged') {
			clearThrottle();
			void refresh();
			return;
		}
		if (event.type === 'RowChanged') {
			if (event.table === 'sessions' || (event.table === 'requests' && event.op === 'INSERT')) {
				scheduleThrottled();
			}
			return;
		}
		switch (event.type) {
			case 'ModelConfirmed':
			case 'ResponseComplete':
			case 'ResponseError':
			case 'UsageReceived':
			case 'TextDelta':
			case 'ThinkingDelta':
			case 'ToolUseStart':
			case 'ToolInputDelta':
			case 'ToolUseEnd':
				wsEventByRequestId.set(event.id, event);
				return;
		}
	}

	function latestWsEventForSession(sessionId: string): ProxyEvent | null {
		const reqIds = sessionToRequests.get(sessionId);
		if (!reqIds || reqIds.size === 0) return null;

		let latest: ProxyEvent | null = null;
		for (const rid of reqIds) {
			const ev = wsEventByRequestId.get(rid);
			if (ev) latest = ev;
		}
		return latest;
	}

	function observeStatus(sessionId: string, kind: StatusKind, now: number): void {
		if (kind === 'streaming' || kind === 'pending') {
			lastLiveAt.set(sessionId, now);
		}
	}

	function isRecentlyActive(sessionId: string, now: number): boolean {
		const at = lastLiveAt.get(sessionId);
		if (at === undefined) return false;
		return now - at < LIVE_STICKY_GRACE_MS;
	}

	function registerSessionRequests(sessionId: string, requestIds: number[]): void {
		const existing = sessionToRequests.get(sessionId);
		if (existing) {
			for (const rid of existing) {
				requestToSession.delete(rid);
			}
		}
		const set = new Set(requestIds);
		sessionToRequests.set(sessionId, set);
		for (const rid of requestIds) {
			requestToSession.set(rid, sessionId);
		}
	}

	function isProjectLoadingMore(project: string): boolean {
		return s.projectLoadingMore[project] ?? false;
	}

	function projectLoadMoreErrorFor(project: string): string | null {
		return s.projectLoadMoreError[project] ?? null;
	}

	function isProjectExhausted(project: string): boolean {
		return s.projectExhausted[project] ?? false;
	}

	function dispose(): void {
		clearThrottle();
		clearSearchTimer();
		wsEventByRequestId.clear();
		requestToSession.clear();
		sessionToRequests.clear();
		lastLiveAt.clear();
		projectLoadMoreInFlight.clear();
	}

	return {
		get sessions() {
			return s.sessions;
		},
		get loading() {
			return s.loading;
		},
		get loadingMore() {
			return s.loadingMore;
		},
		get error() {
			return s.error;
		},
		get loadMoreError() {
			return s.loadMoreError;
		},
		get hasMore() {
			return s.hasMore;
		},
		get searchHits() {
			return s.searchHits;
		},
		get searchLoading() {
			return s.searchLoading;
		},
		get searchError() {
			return s.searchError;
		},
		get bodyHits() {
			return s.bodyHits;
		},
		get bodySearchLoading() {
			return s.bodySearchLoading;
		},
		get bodySearchError() {
			return s.bodySearchError;
		},
		get bodyFullMatchCount() {
			return s.bodyFullMatchCount;
		},
		get bodyMatchesTruncated() {
			return s.bodyMatchesTruncated;
		},
		get bodyWindowNote() {
			return s.bodyWindowNote;
		},
		get contentHits() {
			return s.contentHits;
		},
		get contentSearchLoading() {
			return s.contentSearchLoading;
		},
		get contentSearchError() {
			return s.contentSearchError;
		},
		get contentTotalMatches() {
			return s.contentTotalMatches;
		},
		get contentTotalSessions() {
			return s.contentTotalSessions;
		},
		get contentHasMore() {
			return s.contentHasMore;
		},
		get hydratedSessions() {
			return s.hydratedSessions;
		},
		get hydratingSessions() {
			return s.hydratingSessions;
		},
		get hydrationError() {
			return s.hydrationError;
		},
		searchSessionById,
		get __wsTrackingSize() {
			return requestToSession.size;
		},
		setSearch,
		refresh,
		loadMore,
		loadMoreForProject,
		isProjectLoadingMore,
		projectLoadMoreError: projectLoadMoreErrorFor,
		isProjectExhausted,
		togglePin,
		toggleProjectPin,
		handleEvent,
		latestWsEventForSession,
		observeStatus,
		isRecentlyActive,
		registerSessionRequests,
		dispose
	};
}
