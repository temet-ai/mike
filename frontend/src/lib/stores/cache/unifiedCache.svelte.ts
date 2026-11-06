import type { BodyPayload, RequestDetail, RequestSummary } from '$lib/types';
import type { BodyKind } from './cacheConfig';
import { isRequestLive } from '$lib/status';
import { getRequestBody, getResponseBody, getRequestSentBody, ApiError } from '$lib/api';
import { getCacheConfig } from './cacheConfig';
import {
	BoundedLruCache,
	emitMemoryEviction,
	onMemoryEviction,
	type EvictionSource,
	type MemoryEvictionEvent
} from './cacheMemory';
import {
	idbGetBody,
	idbPutBody,
	idbGetBodies,
	idbGetDetail,
	idbPutDetail,
	idbGetDetails,
	idbGetSessionPage,
	idbPutSessionPage,
	idbDeleteSessionPage,
	idbGetAllSessionPages,
	idbInvalidateSession,
	idbReadWithTimeout
} from './cacheIdb';

export type { BodyKind, EvictionSource };
export type { MemoryEvictionEvent as CacheEvictionEvent };

let bodyMemory: BoundedLruCache<string, BodyPayload>;
let detailMemory: BoundedLruCache<number, RequestDetail>;
let sessionPageMemory: BoundedLruCache<string, RequestSummary[]>;

function buildMemoryCaches(): void {
	const cfg = getCacheConfig();

	bodyMemory = new BoundedLruCache<string, BodyPayload>(
		cfg.bodyMemoryEntryCap,
		cfg.bodyMemoryByteCap,
		(v) => v.byte_size,
		(key, _val, bytes, reason) => {
			const [idStr, kind] = key.split(':') as [string, BodyKind];
			emitMemoryEviction({
				source: 'memory-body',
				id: Number(idStr),
				kind,
				bytes,
				reason
			});
		}
	);

	detailMemory = new BoundedLruCache<number, RequestDetail>(
		cfg.detailMemoryEntryCap,
		0,
		() => 0,
		(key, _val, bytes, reason) => {
			emitMemoryEviction({ source: 'memory-detail', id: key, bytes, reason });
		}
	);

	sessionPageMemory = new BoundedLruCache<string, RequestSummary[]>(
		cfg.sessionPageMemoryEntryCap,
		0,
		() => 0,
		(key, _val, bytes, reason) => {
			emitMemoryEviction({ source: 'memory-session-page', id: key, bytes, reason });
		}
	);
}

buildMemoryCaches();

const bodyInFlight = new Map<string, Promise<BodyPayload>>();
const detailInFlight = new Map<number, Promise<RequestDetail | null>>();
const sessionPageInFlight = new Map<string, Promise<RequestSummary[]>>();

function bodyKey(id: number, kind: BodyKind): string {
	return `${id}:${kind}`;
}

function sessionPageKey(sessionId: string, offset: number): string {
	return `${sessionId}:${offset}`;
}

export function onCacheEviction(listener: (event: MemoryEvictionEvent) => void): () => void {
	return onMemoryEviction(listener);
}

export function peekBody(id: number, kind: BodyKind): BodyPayload | undefined {
	return bodyMemory.peek(bodyKey(id, kind));
}

export async function loadBody(
	id: number,
	kind: BodyKind,
	statusForGating?: import('$lib/types').RequestStatus,
	sessionId?: string | null
): Promise<BodyPayload> {
	if (!getCacheConfig().enabled) {
		return kind === 'request'
			? getRequestBody(id)
			: kind === 'request_sent'
				? getRequestSentBody(id)
				: getResponseBody(id);
	}
	const k = bodyKey(id, kind);

	const mem = bodyMemory.get(k);
	if (mem !== undefined) return mem;

	const existing = bodyInFlight.get(k);
	if (existing) return existing;

	const fetcher =
		kind === 'request'
			? getRequestBody
			: kind === 'request_sent'
				? getRequestSentBody
				: getResponseBody;

	const promise = (async (): Promise<BodyPayload> => {
		try {
			const fromIdb = await idbReadWithTimeout(idbGetBody(id, kind));
			if (fromIdb !== null) {
				bodyMemory.set(k, fromIdb);
				return fromIdb;
			}

			const payload = await fetcher(id);
			bodyMemory.set(k, payload);

			const shouldPersist =
				kind === 'request' ||
				kind === 'request_sent' ||
				(kind === 'response' && statusForGating !== undefined && !isRequestLive(statusForGating));

			if (shouldPersist) {
				void idbPutBody(id, kind, sessionId ?? null, payload);
			}

			return payload;
		} finally {
			bodyInFlight.delete(k);
		}
	})();

	bodyInFlight.set(k, promise);
	return promise;
}

export function describeBodyError(err: unknown): string {
	return err instanceof ApiError ? err.message : String(err);
}

export async function batchWarmBodies(
	_sessionId: string,
	ids: number[],
	kinds: BodyKind[]
): Promise<void> {
	if (!getCacheConfig().enabled) return;
	const keys: Array<[number, BodyKind]> = [];
	for (const id of ids) {
		for (const kind of kinds) {
			const k = bodyKey(id, kind);
			if (!bodyMemory.has(k)) {
				keys.push([id, kind]);
			}
		}
	}
	if (keys.length === 0) return;
	const fromIdb = await idbGetBodies(keys);
	for (const [key, payload] of fromIdb) {
		bodyMemory.set(key, payload);
	}
}

export function peekDetail(id: number): RequestDetail | undefined {
	return detailMemory.peek(id);
}

export async function loadDetail(
	id: number,
	fetcher: (id: number) => Promise<RequestDetail>
): Promise<RequestDetail | null> {
	if (!getCacheConfig().enabled) {
		try {
			return await fetcher(id);
		} catch {
			return null;
		}
	}
	const mem = detailMemory.get(id);
	if (mem !== undefined) return mem;

	const existing = detailInFlight.get(id);
	if (existing) return existing;

	const promise = (async (): Promise<RequestDetail | null> => {
		try {
			const fromIdb = await idbReadWithTimeout(idbGetDetail(id));
			if (fromIdb !== null) {
				detailMemory.set(id, fromIdb);
				return fromIdb;
			}

			const detail = await fetcher(id);
			detailMemory.set(id, detail);

			if (!isRequestLive(detail.status)) {
				void idbPutDetail(id, detail.session_id, detail);
			}

			return detail;
		} catch {
			return null;
		} finally {
			detailInFlight.delete(id);
		}
	})();

	detailInFlight.set(id, promise);
	return promise;
}

export async function batchWarmDetails(_sessionId: string, ids: number[]): Promise<void> {
	if (!getCacheConfig().enabled) return;
	const missing = ids.filter((id) => !detailMemory.has(id));
	if (missing.length === 0) return;
	const fromIdb = await idbGetDetails(missing);
	for (const [id, detail] of fromIdb) {
		detailMemory.set(id, detail);
	}
}

export function putDetailMemoryOnly(detail: RequestDetail): void {
	detailMemory.set(detail.id, detail);
}

export async function promoteDetailToIdb(detail: RequestDetail): Promise<void> {
	if (isRequestLive(detail.status)) return;
	await idbPutDetail(detail.id, detail.session_id, detail);
}

export function peekSessionPage(sessionId: string, offset: number): RequestSummary[] | undefined {
	return sessionPageMemory.peek(sessionPageKey(sessionId, offset));
}

export async function loadSessionPage(
	sessionId: string,
	offset: number,
	fetcher: (sessionId: string, offset: number) => Promise<RequestSummary[]>,
	pageSize?: number
): Promise<RequestSummary[]> {
	if (!getCacheConfig().enabled) return fetcher(sessionId, offset);
	const k = sessionPageKey(sessionId, offset);
	const isWholePage = (rows: RequestSummary[]): boolean =>
		pageSize === undefined || rows.length >= pageSize;

	const mem = sessionPageMemory.get(k);
	if (mem !== undefined && isWholePage(mem)) return mem;

	const existing = sessionPageInFlight.get(k);
	if (existing) return existing;

	const promise = (async (): Promise<RequestSummary[]> => {
		try {
			const fromIdb = await idbReadWithTimeout(idbGetSessionPage(sessionId, offset));
			if (fromIdb !== null && isWholePage(fromIdb)) {
				sessionPageMemory.set(k, fromIdb);
				return fromIdb;
			}
			if (fromIdb !== null) void idbDeleteSessionPage(sessionId, offset);
			const rows = await fetcher(sessionId, offset);
			sessionPageMemory.set(k, rows);
			if (isWholePage(rows)) void idbPutSessionPage(sessionId, offset, rows);
			return rows;
		} finally {
			sessionPageInFlight.delete(k);
		}
	})();

	sessionPageInFlight.set(k, promise);
	return promise;
}

export async function batchWarmSessionPages(sessionId: string): Promise<void> {
	if (!getCacheConfig().enabled) return;
	const fromIdb = await idbGetAllSessionPages(sessionId);
	for (const [offset, rows] of fromIdb) {
		const k = sessionPageKey(sessionId, offset);
		if (!sessionPageMemory.has(k)) {
			sessionPageMemory.set(k, rows);
		}
	}
}

export async function invalidateSession(sessionId: string): Promise<void> {
	for (const key of [...sessionPageMemory.keys()]) {
		if (key.startsWith(`${sessionId}:`)) {
			sessionPageMemory.delete(key);
		}
	}
	await idbInvalidateSession(sessionId);
}

export function __resetCacheForTests(): void {
	buildMemoryCaches();
	bodyInFlight.clear();
	detailInFlight.clear();
	sessionPageInFlight.clear();
}

export function __cacheSizesForTests(): {
	bodies: number;
	details: number;
	sessionPages: number;
} {
	return {
		bodies: bodyMemory.size,
		details: detailMemory.size,
		sessionPages: sessionPageMemory.size
	};
}
