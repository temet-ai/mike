import { ApiError, listFindingsPaginated } from '$lib/api';
import type { Finding, ProxyEvent } from '$lib/types';

export const DEFAULT_POLL_MS = 10_000;
const DIAGNOSTIC_THROTTLE_MS = 500;
export const PAGE_SIZE = 50;

export const FINDINGS_RETAINED_CAP = 500;

export interface FindingEviction {
	findingKey: string;
	reason: 'findings-cap';
}

type FindingEvictionListener = (event: FindingEviction) => void;

const findingEvictionListeners = new Set<FindingEvictionListener>();

function emitFindingEviction(event: FindingEviction): void {
	for (const listener of findingEvictionListeners) {
		try {
			listener(event);
		} catch (err) {
			console.warn('[findings] eviction listener threw', err);
		}
	}
}

export function onFindingEvict(listener: FindingEvictionListener): () => void {
	findingEvictionListeners.add(listener);
	return () => {
		findingEvictionListeners.delete(listener);
	};
}

export interface FindingsStore {
	readonly findings: Finding[] | null;
	readonly total: number | null;
	readonly analyzerFilter: string | null;
	readonly sessionFilter: string | null;
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

interface MutableState {
	findings: Finding[] | null;
	total: number | null;
	analyzerFilter: string | null;
	sessionFilter: string | null;
	loading: boolean;
	error: string | null;
	loadingMore: boolean;
	hasMore: boolean;
	loadMoreError: string | null;
	lastFetchedAt: number | null;
}

function findingKey(f: Finding): string {
	return `${f.request_id}:${f.analyzer}:${f.match_range[0]}-${f.match_range[1]}`;
}

export function createFindingsStore(): FindingsStore {
	const s = $state<MutableState>({
		findings: null,
		total: null,
		analyzerFilter: null,
		sessionFilter: null,
		loading: false,
		error: null,
		loadingMore: false,
		hasMore: true,
		loadMoreError: null,
		lastFetchedAt: null
	});

	let inFlight: Promise<void> | null = null;
	let loadOlderInFlight: Promise<void> | null = null;
	let pollTimer: ReturnType<typeof setInterval> | null = null;
	let throttleTimer: ReturnType<typeof setTimeout> | null = null;

	function capRetainedFindings(): void {
		const list = s.findings;
		if (list === null || list.length <= FINDINGS_RETAINED_CAP) return;
		const dropped = list.slice(FINDINGS_RETAINED_CAP);
		s.findings = list.slice(0, FINDINGS_RETAINED_CAP);
		for (const f of dropped) {
			emitFindingEviction({
				findingKey: findingKey(f),
				reason: 'findings-cap'
			});
		}
	}

	async function refresh(): Promise<void> {
		if (inFlight) return inFlight;
		s.loading = true;
		const analyzer = s.analyzerFilter;
		const sessionId = s.sessionFilter;
		const promise = (async (): Promise<void> => {
			try {
				const params: Parameters<typeof listFindingsPaginated>[0] = {
					offset: 0,
					limit: PAGE_SIZE
				};
				if (analyzer !== null) params.analyzer = analyzer;
				if (sessionId !== null) params.session_id = sessionId;
				const page = await listFindingsPaginated(params);
				const fetched = page.findings;
				if (s.analyzerFilter !== analyzer || s.sessionFilter !== sessionId) {
					return;
				}
				s.total = page.total;
				if (s.findings === null) {
					s.findings = fetched;
				} else {
					const fetchedKeys = new Set(fetched.map(findingKey));
					const olderRows = s.findings.filter((f) => !fetchedKeys.has(findingKey(f)));
					s.findings = [...fetched, ...olderRows];
				}
				capRetainedFindings();
				s.hasMore = fetched.length >= PAGE_SIZE;
				s.error = null;
				s.loadMoreError = null;
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

	async function loadOlder(): Promise<void> {
		if (loadOlderInFlight) return loadOlderInFlight;
		if (!s.hasMore || s.loading) return;
		s.loadingMore = true;
		const analyzer = s.analyzerFilter;
		const sessionId = s.sessionFilter;
		const promise = (async (): Promise<void> => {
			try {
				const offset = s.findings === null ? 0 : s.findings.length;
				const params: Parameters<typeof listFindingsPaginated>[0] = {
					offset,
					limit: PAGE_SIZE
				};
				if (analyzer !== null) params.analyzer = analyzer;
				if (sessionId !== null) params.session_id = sessionId;
				const page = await listFindingsPaginated(params);
				const fetched = page.findings;
				if (s.analyzerFilter !== analyzer || s.sessionFilter !== sessionId) return;
				s.total = page.total;
				const existingKeys = new Set((s.findings ?? []).map(findingKey));
				const appended = fetched.filter((f) => !existingKeys.has(findingKey(f)));
				if (appended.length > 0) {
					s.findings = [...(s.findings ?? []), ...appended];
					capRetainedFindings();
				}
				s.hasMore = fetched.length >= PAGE_SIZE;
				s.loadMoreError = null;
			} catch (err) {
				s.loadMoreError = err instanceof ApiError ? err.message : String(err);
			} finally {
				s.loadingMore = false;
				loadOlderInFlight = null;
			}
		})();
		loadOlderInFlight = promise;
		return promise;
	}

	function clearThrottle(): void {
		if (throttleTimer !== null) {
			clearTimeout(throttleTimer);
			throttleTimer = null;
		}
	}

	function handleEvent(event: ProxyEvent): void {
		if (event.type === 'DiagnosticReady') {
			clearThrottle();
			throttleTimer = setTimeout(() => {
				throttleTimer = null;
				void refresh();
			}, DIAGNOSTIC_THROTTLE_MS);
		} else if (event.type === 'Lagged') {
			clearThrottle();
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

	function setAnalyzerFilter(analyzer: string | null): void {
		if (s.analyzerFilter === analyzer) return;
		s.analyzerFilter = analyzer;
		s.findings = null;
		s.total = null;
		s.hasMore = true;
		s.loadMoreError = null;
		void refresh();
	}

	function setSessionFilter(sessionId: string | null): void {
		if (s.sessionFilter === sessionId) return;
		s.sessionFilter = sessionId;
		s.findings = null;
		s.total = null;
		s.hasMore = true;
		s.loadMoreError = null;
		void refresh();
	}

	function dispose(): void {
		stopPolling();
		clearThrottle();
	}

	return {
		get findings() {
			return s.findings;
		},
		get total() {
			return s.total;
		},
		get analyzerFilter() {
			return s.analyzerFilter;
		},
		get sessionFilter() {
			return s.sessionFilter;
		},
		get loading() {
			return s.loading;
		},
		get error() {
			return s.error;
		},
		get loadingMore() {
			return s.loadingMore;
		},
		get hasMore() {
			return s.hasMore;
		},
		get loadMoreError() {
			return s.loadMoreError;
		},
		get lastFetchedAt() {
			return s.lastFetchedAt;
		},
		refresh,
		loadOlder,
		handleEvent,
		startPolling,
		stopPolling,
		setAnalyzerFilter,
		setSessionFilter,
		dispose
	};
}

export interface RequestGroup {
	request_id: number;
	request: Finding['request'];
	newest_created_at: string;
	findings: Finding[];
}

export interface SessionGroup {
	session_id: string;
	project: string | null;
	model: string | null;
	count: number;
	newest_created_at: string;
	requests: RequestGroup[];
}

export interface CategoryGroup {
	category: Finding['category'];
	count: number;
	sessions: SessionGroup[];
}

export function groupFindings(findings: Finding[]): CategoryGroup[] {
	const byCategory = new Map<Finding['category'], Map<string, Map<number, Finding[]>>>();

	for (const finding of findings) {
		let sessionMap = byCategory.get(finding.category);
		if (!sessionMap) {
			sessionMap = new Map();
			byCategory.set(finding.category, sessionMap);
		}
		let requestMap = sessionMap.get(finding.session_id);
		if (!requestMap) {
			requestMap = new Map();
			sessionMap.set(finding.session_id, requestMap);
		}
		const list = requestMap.get(finding.request_id);
		if (!list) {
			requestMap.set(finding.request_id, [finding]);
		} else {
			list.push(finding);
		}
	}

	const result: CategoryGroup[] = [];
	for (const [category, sessionMap] of byCategory) {
		const sessions: SessionGroup[] = [];
		let categoryCount = 0;
		for (const [sessionId, requestMap] of sessionMap) {
			const requests: RequestGroup[] = [];
			let sessionCount = 0;
			let sessionNewest = '';
			let sessionProject: string | null = null;
			let sessionModel: string | null = null;
			for (const [requestId, list] of requestMap) {
				const sorted = list.slice().sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
				const newest = sorted[0];
				if (!newest) continue;
				requests.push({
					request_id: requestId,
					request: newest.request,
					newest_created_at: newest.created_at,
					findings: sorted
				});
				sessionCount += sorted.length;
				if (newest.created_at > sessionNewest) {
					sessionNewest = newest.created_at;
				}
				if (sessionProject === null) sessionProject = newest.request.project;
				if (sessionModel === null) sessionModel = newest.request.model;
			}
			requests.sort((a, b) => (a.newest_created_at < b.newest_created_at ? 1 : -1));
			sessions.push({
				session_id: sessionId,
				project: sessionProject,
				model: sessionModel,
				count: sessionCount,
				newest_created_at: sessionNewest,
				requests
			});
			categoryCount += sessionCount;
		}
		sessions.sort((a, b) => (a.newest_created_at < b.newest_created_at ? 1 : -1));
		result.push({ category, count: categoryCount, sessions });
	}

	result.sort((a, b) => {
		if (b.count !== a.count) return b.count - a.count;
		return a.category < b.category ? -1 : 1;
	});
	return result;
}
