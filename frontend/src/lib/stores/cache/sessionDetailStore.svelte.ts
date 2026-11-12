import {
	getRequest,
	getRequestSummary,
	getAccumulatedText,
	getSession,
	listSessionOperatorTurns,
	ApiError
} from '$lib/api';
import { isSidecarOrigin } from '$lib/turn-origins';
import type {
	ModelBreakdownEntry,
	OperatorTurnDto,
	ProxyEvent,
	RequestDetail,
	RequestSummary,
	SessionDetail,
	RuntimeSessionObservation,
	RuntimeItemPage,
	SessionQuality,
	SessionSummary,
	SessionTokenUsage
} from '$lib/types';
import {
	loadSessionPage,
	batchWarmSessionPages,
	invalidateSession,
	loadDetail as cacheLoadDetail,
	putDetailMemoryOnly,
	onCacheEviction
} from './unifiedCache.svelte';

export const PAGE_SIZE = 50;

export const BURST_FETCH_SIZE = 500;

const SESSION_CACHE_CAP = 10;

export const PAGES_CAP_PER_SESSION = 32;

export const PAGES_RESIDENT_CAP_PER_SESSION = 200;

export const WALK_BACK_PAGES = 5;

export const DETAILS_CAP_PER_SESSION = 60;

export interface SessionPageEviction {
	sessionId: string;
	offset: number;
	rows: number;
	reason: 'pages-cap';
}

type SessionPageEvictionListener = (event: SessionPageEviction) => void;

const sessionPageEvictionListeners = new Set<SessionPageEvictionListener>();

export function onSessionPageEvict(listener: SessionPageEvictionListener): () => void {
	sessionPageEvictionListeners.add(listener);
	return () => {
		sessionPageEvictionListeners.delete(listener);
	};
}

function emitSessionPageEviction(event: SessionPageEviction): void {
	for (const listener of sessionPageEvictionListeners) {
		try {
			listener(event);
		} catch (err) {
			console.warn('[sessionDetailStore] page eviction listener threw', err);
		}
	}
}

export interface DetailEviction {
	sessionId: string | null;
	requestId: number;
	reason: 'details-cap' | 'cancel-pressure';
}

type DetailEvictionListener = (event: DetailEviction) => void;

const detailEvictionListeners = new Set<DetailEvictionListener>();

export function onDetailEviction(listener: DetailEvictionListener): () => void {
	detailEvictionListeners.add(listener);
	return () => {
		detailEvictionListeners.delete(listener);
	};
}

function emitDetailEviction(event: DetailEviction): void {
	for (const listener of detailEvictionListeners) {
		try {
			listener(event);
		} catch (err) {
			console.warn('[sessionDetailStore] detail eviction listener threw', err);
		}
	}
}

onCacheEviction((evt) => {
	if (evt.source === 'memory-session-page') {
		const id = String(evt.id);
		const colonIdx = id.lastIndexOf(':');
		if (colonIdx > 0) {
			const sessionId = id.slice(0, colonIdx);
			const offset = Number(id.slice(colonIdx + 1));
			emitSessionPageEviction({
				sessionId,
				offset,
				rows: 0,
				reason: 'pages-cap'
			});
		}
	}
});

interface CachedSession {
	summary: SessionSummary;
	quality: SessionQuality | null;
	modelBreakdown: ModelBreakdownEntry[];
	tokenUsage: SessionTokenUsage | null;
	observation: RuntimeSessionObservation | null;
	runtimeItems: RuntimeItemPage | null;
	total: number;
	pages: Map<number, RequestSummary[]>;
	lastTouchedAt: number;
}

const sessionCache = new Map<string, CachedSession>();

function evictPagesUntilWithinCap(entry: CachedSession, sessionId: string): void {
	while (entry.pages.size > PAGES_RESIDENT_CAP_PER_SESSION) {
		let evictOffset: number | null = null;
		for (const offset of entry.pages.keys()) {
			if (evictOffset === null || offset < evictOffset) evictOffset = offset;
		}
		if (evictOffset === null) return;
		const rows = entry.pages.get(evictOffset);
		entry.pages.delete(evictOffset);
		emitSessionPageEviction({
			sessionId,
			offset: evictOffset,
			rows: rows?.length ?? 0,
			reason: 'pages-cap'
		});
	}
}

function touchCache(id: string): void {
	const entry = sessionCache.get(id);
	if (!entry) return;
	entry.lastTouchedAt = Date.now();
	sessionCache.delete(id);
	sessionCache.set(id, entry);
}

function evictOldestIfFull(): void {
	if (sessionCache.size < SESSION_CACHE_CAP) return;
	const oldestKey = sessionCache.keys().next().value;
	if (oldestKey !== undefined) sessionCache.delete(oldestKey);
}

function dropCache(id: string): void {
	sessionCache.delete(id);
}

export function __clearSessionCacheForTests(): void {
	sessionCache.clear();
}

export function __sessionCachePagesForTests(id: string): Map<number, number> | null {
	const entry = sessionCache.get(id);
	if (!entry) return null;
	const out = new Map<number, number>();
	for (const [offset, rows] of entry.pages) {
		out.set(offset, rows.length);
	}
	return out;
}

function projectFromCache(entry: CachedSession): SessionDetail {
	const offsets = Array.from(entry.pages.keys()).sort((a, b) => a - b);
	const requests: RequestSummary[] = [];
	const seen = new Set<number>();
	let firstOffset: number | null = null;
	let lastLimit = PAGE_SIZE;
	for (const off of offsets) {
		const page = entry.pages.get(off);
		if (!page) continue;
		if (firstOffset === null) firstOffset = off;
		for (const r of page) {
			if (seen.has(r.id)) continue;
			seen.add(r.id);
			requests.push(r);
		}
		lastLimit = page.length;
	}
	return {
		summary: entry.summary,
		quality: entry.quality,
		model_breakdown: entry.modelBreakdown,
		token_usage: entry.tokenUsage ?? undefined,
		observation: entry.observation,
		runtime_items: entry.runtimeItems,
		requests,
		total_requests: entry.total,
		offset: firstOffset ?? 0,
		limit: lastLimit
	};
}

export interface SessionDetailStore {
	readonly detail: SessionDetail | null;
	readonly loading: boolean;
	readonly loadingOlder: boolean;
	readonly error: string | null;
	readonly loadOlderError: string | null;
	readonly notFound: boolean;
	readonly hasOlder: boolean;
	readonly olderCeilingReached: boolean;
	readonly currentId: string | null;
	readonly details: Map<number, RequestDetail>;
	readonly detailErrors: Set<number>;
	readonly operatorTurns: OperatorTurnDto[];
	readonly promptKindFilter: string | null;
	load(id: string): Promise<void>;
	refresh(): Promise<void>;
	loadOlder(): Promise<void>;
	loadAll(): Promise<void>;
	loadDetail(id: number): Promise<void>;
	cancelDetail(id: number): void;
	predecessorOf(rid: number): number | null;
	refreshOperatorTurns(): void;
	handleEvent(event: ProxyEvent): void;
	setPromptKindFilter(kind: string | null): void;
}

interface MutableState {
	detail: SessionDetail | null;
	loading: boolean;
	loadingOlder: boolean;
	error: string | null;
	loadOlderError: string | null;
	notFound: boolean;
	currentId: string | null;
	details: Map<number, RequestDetail>;
	detailErrors: Set<number>;
	operatorTurns: OperatorTurnDto[];
	promptKindFilter: string | null;
}

export function createSessionDetailStore(): SessionDetailStore {
	const connState = $state<MutableState>({
		detail: null,
		loading: false,
		loadingOlder: false,
		error: null,
		loadOlderError: null,
		notFound: false,
		currentId: null,
		details: new Map<number, RequestDetail>(),
		detailErrors: new Set<number>(),
		operatorTurns: [],
		promptKindFilter: null
	});

	let inFlight: Promise<void> | null = null;
	let inFlightId: string | null = null;
	let loadOlderInFlight: Promise<void> | null = null;
	let loadAllDoneFor: string | null = null;
	let loadAllInFlight: Promise<void> | null = null;
	let turnsFetchEpoch = 0;
	const pendingRowFetches = new Set<number>();

	function fetchOperatorTurns(sid: string): void {
		const epoch = ++turnsFetchEpoch;
		void listSessionOperatorTurns(sid, {
			order: 'asc',
			promptKind: connState.promptKindFilter ?? undefined
		})
			.then((turns) => {
				if (connState.currentId !== sid) return;
				if (turnsFetchEpoch !== epoch) return;
				connState.operatorTurns = turns;
			})
			.catch(() => {});
	}

	function refreshOperatorTurns(): void {
		if (connState.currentId) {
			fetchOperatorTurns(connState.currentId);
		}
	}

	function setPromptKindFilter(kind: string | null): void {
		if (connState.promptKindFilter === kind) return;
		connState.promptKindFilter = kind;
		refreshOperatorTurns();
	}

	type DetailFetchEntry = {
		promise: Promise<void>;
		cancelled: boolean;
	};
	const detailInFlight = new Map<number, DetailFetchEntry>();

	async function fetchInitialPage(id: string): Promise<{
		summary: SessionSummary;
		quality: SessionQuality | null;
		modelBreakdown: ModelBreakdownEntry[];
		tokenUsage: SessionTokenUsage | null;
		observation: RuntimeSessionObservation | null;
		runtimeItems: RuntimeItemPage | null;
		total: number;
		pageOffset: number;
		page: RequestSummary[];
	}> {
		const fetched = await getSession(id, {
			order: 'desc',
			offset: 0,
			limit: PAGE_SIZE
		});
		const total = fetched.total_requests ?? fetched.requests.length;
		const page = fetched.requests.toReversed();
		return {
			summary: fetched.summary,
			quality: fetched.quality ?? null,
			modelBreakdown: fetched.model_breakdown ?? [],
			tokenUsage: fetched.token_usage ?? null,
			observation: fetched.observation ?? null,
			runtimeItems: fetched.runtime_items ?? null,
			total,
			pageOffset: Math.max(0, total - page.length),
			page
		};
	}

	function fetchOnce(id: string): Promise<void> {
		if (inFlight && inFlightId === id) return inFlight;
		fetchOperatorTurns(id);
		const promise = (async (): Promise<void> => {
			connState.loading = true;
			connState.error = null;
			connState.notFound = false;
			try {
				const cached = sessionCache.get(id);
				if (cached) {
					if (connState.currentId !== id) return;
					connState.detail = projectFromCache(cached);
					touchCache(id);
					void batchWarmSessionPages(id);
					return;
				}

				const {
					summary,
					quality,
					modelBreakdown,
					tokenUsage,
					observation,
					runtimeItems,
					total,
					pageOffset,
					page
				} = await fetchInitialPage(id);
				if (connState.currentId !== id) return;

				evictOldestIfFull();
				const entry: CachedSession = {
					summary,
					quality,
					modelBreakdown,
					tokenUsage,
					observation,
					runtimeItems,
					total,
					pages: new Map([[pageOffset, page]]),
					lastTouchedAt: Date.now()
				};
				sessionCache.set(id, entry);
				evictPagesUntilWithinCap(entry, id);
				void loadSessionPage(id, pageOffset, async () => page, PAGE_SIZE);

				connState.detail = projectFromCache(entry);
			} catch (err) {
				if (connState.currentId !== id) return;
				if (err instanceof ApiError && err.status === 404) {
					connState.notFound = true;
					connState.detail = null;
				} else {
					connState.error = err instanceof ApiError ? err.message : String(err);
					connState.detail = null;
				}
			} finally {
				if (connState.currentId === id) {
					connState.loading = false;
				}
				if (inFlightId === id) {
					inFlight = null;
					inFlightId = null;
				}
			}
		})();
		inFlight = promise;
		inFlightId = id;
		return promise;
	}

	function load(id: string): Promise<void> {
		if (
			connState.currentId === id &&
			connState.detail !== null &&
			connState.detail.summary.id === id &&
			inFlight === null
		) {
			return Promise.resolve();
		}
		if (connState.currentId !== id) {
			connState.details = new Map();
			connState.detailErrors = new Set();
			detailInFlight.clear();
			connState.loadOlderError = null;
			loadAllDoneFor = null;
			connState.operatorTurns = [];
			loadAllInFlight = null;
		}
		connState.currentId = id;
		return fetchOnce(id);
	}

	function refresh(): Promise<void> {
		const id = connState.currentId;
		if (id === null) return Promise.resolve();
		dropCache(id);
		void invalidateSession(id);
		loadAllDoneFor = null;
		loadAllInFlight = null;
		return fetchOnce(id);
	}

	function plannedWalkBack(entry: CachedSession): Array<{ offset: number; length: number }> {
		const offsets = Array.from(entry.pages.keys()).sort((a, b) => a - b);
		const earliestKnown = offsets.length > 0 ? offsets[0]! : entry.total;
		if (earliestKnown <= 0) return [];
		const room = PAGES_RESIDENT_CAP_PER_SESSION - entry.pages.size;
		if (room <= 0) return [];
		const wanted = Math.min(WALK_BACK_PAGES, room);
		const planned: Array<{ offset: number; length: number }> = [];
		let upper = earliestKnown;
		for (let i = 0; i < wanted && upper > 0; i++) {
			const offset = Math.max(0, upper - PAGE_SIZE);
			planned.push({ offset, length: upper - offset });
			upper = offset;
		}
		return planned.reverse();
	}

	async function loadOlder(): Promise<void> {
		if (loadOlderInFlight) return loadOlderInFlight;
		const id = connState.currentId;
		if (id === null) return;
		const entry = sessionCache.get(id);
		if (!entry) return;
		const planned = plannedWalkBack(entry);
		if (planned.length === 0) return;
		const runStart = planned[0]!.offset;
		const runLimit = planned.reduce((sum, p) => sum + p.length, 0);

		connState.loadingOlder = true;
		connState.loadOlderError = null;
		const promise = (async (): Promise<void> => {
			try {
				let runFetch: Promise<RequestSummary[]> | null = null;
				const fetchRun = (): Promise<RequestSummary[]> => {
					runFetch ??= getSession(id, {
						order: 'asc',
						offset: runStart,
						limit: runLimit
					}).then((fetched) => fetched.requests);
					return runFetch;
				};
				const loaded = await Promise.all(
					planned.map(({ offset, length }) =>
						loadSessionPage(
							id,
							offset,
							async () => {
								const rows = await fetchRun();
								const from = offset - runStart;
								return rows.slice(from, from + length);
							},
							length
						).then((rows) => ({ offset, rows }))
					)
				);
				if (connState.currentId !== id) return;
				const fresh = sessionCache.get(id);
				if (!fresh) return;
				for (const { offset, rows } of loaded) {
					fresh.pages.set(offset, rows);
				}
				evictPagesUntilWithinCap(fresh, id);
				connState.detail = projectFromCache(fresh);
				touchCache(id);
			} catch (err) {
				if (connState.currentId !== id) return;
				connState.loadOlderError = err instanceof ApiError ? err.message : String(err);
			} finally {
				if (connState.currentId === id) {
					connState.loadingOlder = false;
				}
				loadOlderInFlight = null;
			}
		})();
		loadOlderInFlight = promise;
		return promise;
	}

	async function loadAll(): Promise<void> {
		const id = connState.currentId;
		if (id === null) return;
		if (loadAllDoneFor === id) return;
		if (loadAllInFlight) return loadAllInFlight;

		const promise = (async (): Promise<void> => {
			if (!sessionCache.has(id)) {
				await fetchOnce(id);
			}
			if (connState.currentId !== id) return;
			const entry = sessionCache.get(id);
			if (!entry) return;

			const total = entry.total;
			if (total <= 0) {
				loadAllDoneFor = id;
				return;
			}

			const allOffsets: number[] = [];
			for (let off = 0; off < total; off += PAGE_SIZE) allOffsets.push(off);
			const capped =
				allOffsets.length > PAGES_CAP_PER_SESSION
					? allOffsets.slice(allOffsets.length - PAGES_CAP_PER_SESSION)
					: allOffsets;
			const missing = capped.filter((off) => !entry.pages.has(off));
			if (missing.length === 0) {
				loadAllDoneFor = id;
				return;
			}

			connState.loadingOlder = true;
			connState.loadOlderError = null;
			try {
				const runs: number[][] = [];
				for (const off of [...missing].sort((a, b) => a - b)) {
					const open = runs[runs.length - 1];
					if (
						open !== undefined &&
						off === open[open.length - 1]! + PAGE_SIZE &&
						open.length * PAGE_SIZE < BURST_FETCH_SIZE
					) {
						open.push(off);
					} else {
						runs.push([off]);
					}
				}

				const runResults = await Promise.all(
					runs.map(async (run) => {
						const runStart = run[0]!;
						let runFetch: Promise<RequestSummary[]> | null = null;
						const fetchRun = (): Promise<RequestSummary[]> => {
							runFetch ??= getSession(id, {
								order: 'asc',
								offset: runStart,
								limit: run.length * PAGE_SIZE
							}).then((fetched) => fetched.requests);
							return runFetch;
						};
						const settled = await Promise.allSettled(
							run.map((off) =>
								loadSessionPage(
									id,
									off,
									async () => {
										const rows = await fetchRun();
										const from = off - runStart;
										return rows.slice(from, from + PAGE_SIZE);
									},
									PAGE_SIZE
								).then((rows) => ({ off, rows }))
							)
						);
						const loaded: Array<{ off: number; rows: RequestSummary[] }> = [];
						let failure: unknown = null;
						for (const outcome of settled) {
							if (outcome.status === 'fulfilled') loaded.push(outcome.value);
							else failure ??= outcome.reason;
						}
						return { loaded, failure };
					})
				);
				const results = runResults.flatMap((r) => r.loaded);
				const failure = runResults.find((r) => r.failure !== null)?.failure ?? null;
				if (connState.currentId !== id) return;
				const fresh = sessionCache.get(id);
				if (!fresh) return;
				for (const { off, rows } of results) {
					fresh.pages.set(off, rows);
				}
				evictPagesUntilWithinCap(fresh, id);
				connState.detail = projectFromCache(fresh);
				touchCache(id);
				if (failure !== null) {
					connState.loadOlderError =
						failure instanceof ApiError ? failure.message : String(failure);
				} else {
					loadAllDoneFor = id;
				}
			} catch (err) {
				if (connState.currentId !== id) return;
				connState.loadOlderError = err instanceof ApiError ? err.message : String(err);
			} finally {
				if (connState.currentId === id) {
					connState.loadingOlder = false;
				}
				loadAllInFlight = null;
			}
		})();
		loadAllInFlight = promise;
		return promise;
	}

	function loadDetail(rid: number): Promise<void> {
		if (connState.details.has(rid)) return Promise.resolve();
		const existing = detailInFlight.get(rid);
		if (existing) {
			existing.cancelled = false;
			return existing.promise;
		}
		const entry: DetailFetchEntry = {
			cancelled: false,
			promise: Promise.resolve()
		};
		entry.promise = (async (): Promise<void> => {
			try {
				const fetched = await cacheLoadDetail(rid, async (id) => {
					const detail = await getRequest(id);
					if (
						detail.is_streaming &&
						detail.status !== 'Streaming' &&
						detail.accumulated_text === undefined
					) {
						try {
							detail.accumulated_text = await getAccumulatedText(id);
						} catch {}
					}
					return detail;
				});
				if (!fetched) {
					if (!entry.cancelled && connState.currentId !== null) {
						const next = new Set(connState.detailErrors);
						next.add(rid);
						connState.detailErrors = next;
					}
					return;
				}
				if (entry.cancelled) return;
				if (connState.currentId === null) return;
				if (fetched.session_id !== connState.currentId) return;
				const next = new Map(connState.details);
				next.set(rid, fetched);
				connState.details = next;
				if (connState.detailErrors.has(rid)) {
					const cleared = new Set(connState.detailErrors);
					cleared.delete(rid);
					connState.detailErrors = cleared;
				}
				evictDetailsUntilWithinCap();
			} catch {
				if (!entry.cancelled && connState.currentId !== null) {
					const next = new Set(connState.detailErrors);
					next.add(rid);
					connState.detailErrors = next;
				}
			} finally {
				const current = detailInFlight.get(rid);
				if (current === entry) {
					detailInFlight.delete(rid);
				}
			}
		})();
		detailInFlight.set(rid, entry);
		return entry.promise;
	}

	function cancelDetail(rid: number): void {
		const existing = detailInFlight.get(rid);
		if (existing) {
			existing.cancelled = true;
		}
		if (connState.details.has(rid)) {
			const next = new Map(connState.details);
			next.delete(rid);
			connState.details = next;
			emitDetailEviction({
				sessionId: connState.currentId,
				requestId: rid,
				reason: 'cancel-pressure'
			});
		}
	}

	function evictDetailsUntilWithinCap(): void {
		while (connState.details.size > DETAILS_CAP_PER_SESSION) {
			const head = connState.details.keys().next();
			if (head.done) return;
			const rid = head.value;
			const next = new Map(connState.details);
			next.delete(rid);
			connState.details = next;
			emitDetailEviction({
				sessionId: connState.currentId,
				requestId: rid,
				reason: 'details-cap'
			});
		}
	}

	function predecessorOf(rid: number): number | null {
		const detail = connState.details.get(rid);
		const detailPred = detail?.predecessor_request_id;
		if (typeof detailPred === 'number') return detailPred;

		const summaries = connState.detail?.requests ?? [];
		const summary = summaries.find((r) => r.id === rid);
		const summaryPred = summary?.predecessor_request_id;
		if (typeof summaryPred === 'number') return summaryPred;

		if (summary === undefined || summaries.length === 0) return null;
		const idx = summaries.findIndex((r) => r.id === rid);
		if (idx < 0) return null;
		const me = summary;

		const normalize = (o: string | undefined): string => {
			const v = o ?? 'primary';
			return v === 'primary_first' ? 'primary' : v;
		};
		const origin = normalize(me.turn_origin);
		if (isSidecarOrigin(origin)) {
			return null;
		}

		for (let j = idx - 1; j >= 0; j--) {
			const candidate = summaries[j];
			if (!candidate) continue;
			const candOrigin = normalize(candidate.turn_origin);
			if (candOrigin !== origin) continue;
			if (origin === 'subagent' && candidate.subagent_persona !== me.subagent_persona) {
				continue;
			}
			return candidate.id;
		}

		return null;
	}

	function pageRowIndex(
		entry: CachedSession,
		rid: number
	): { rows: RequestSummary[]; idx: number } | null {
		for (const rows of entry.pages.values()) {
			const idx = rows.findIndex((r) => r.id === rid);
			if (idx !== -1) return { rows, idx };
		}
		return null;
	}

	async function applyRowChange(sid: string, rid: number, op: 'INSERT' | 'UPDATE'): Promise<void> {
		if (pendingRowFetches.has(rid)) return;
		const preEntry = sessionCache.get(sid);
		if (!preEntry) return;
		if (op === 'UPDATE' && pageRowIndex(preEntry, rid) === null) return;

		pendingRowFetches.add(rid);
		try {
			const fetched = await getRequestSummary(rid);
			if (connState.currentId !== sid) return;
			if (fetched.session_id !== sid) return;
			const entry = sessionCache.get(sid);
			if (!entry) return;

			const located = pageRowIndex(entry, rid);
			if (located) {
				located.rows[located.idx] = fetched;
			} else {
				const offsets = Array.from(entry.pages.keys()).sort((a, b) => a - b);
				const tailOffset = offsets.length > 0 ? offsets[offsets.length - 1]! : 0;
				const tailPage = entry.pages.get(tailOffset);
				if (tailPage) tailPage.push(fetched);
				else entry.pages.set(0, [fetched]);
				entry.total += 1;
				entry.summary = { ...entry.summary, request_count: entry.total };
				evictPagesUntilWithinCap(entry, sid);
			}
			connState.detail = projectFromCache(entry);
			touchCache(sid);
		} catch {
		} finally {
			pendingRowFetches.delete(rid);
		}
	}

	function handleEvent(event: ProxyEvent): void {
		if (event.type !== 'RowChanged') return;
		if (event.table !== 'requests') return;
		const sid = connState.currentId;
		if (sid === null || event.session_id !== sid) return;
		const rid = Number(event.id);
		if (!Number.isFinite(rid)) return;
		if (event.op === 'INSERT' || event.op === 'UPDATE') {
			void applyRowChange(sid, rid, event.op);
		}
	}

	return {
		get detail() {
			return connState.detail;
		},
		get loading() {
			return connState.loading;
		},
		get loadingOlder() {
			return connState.loadingOlder;
		},
		get error() {
			return connState.error;
		},
		get loadOlderError() {
			return connState.loadOlderError;
		},
		get notFound() {
			return connState.notFound;
		},
		get hasOlder() {
			const d = connState.detail;
			if (d === null) return false;
			return (d.offset ?? 0) > 0;
		},
		get olderCeilingReached() {
			const d = connState.detail;
			if (d === null) return false;
			if ((d.offset ?? 0) <= 0) return false;
			const id = connState.currentId;
			if (id === null) return false;
			const entry = sessionCache.get(id);
			if (!entry) return false;
			return entry.pages.size >= PAGES_RESIDENT_CAP_PER_SESSION;
		},
		get currentId() {
			return connState.currentId;
		},
		get details() {
			return connState.details;
		},
		get detailErrors() {
			return connState.detailErrors;
		},
		get operatorTurns() {
			return connState.operatorTurns;
		},
		get promptKindFilter() {
			return connState.promptKindFilter;
		},
		load,
		refresh,
		loadOlder,
		loadAll,
		loadDetail,
		cancelDetail,
		predecessorOf,
		refreshOperatorTurns,
		setPromptKindFilter,
		handleEvent
	};
}

export { putDetailMemoryOnly };
