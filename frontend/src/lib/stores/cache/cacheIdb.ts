import { openDB, type IDBPDatabase } from 'idb';
import type { BodyPayload, RequestDetail, RequestSummary } from '$lib/types';
import type { BodyKind } from './cacheConfig';
import { getCacheConfig } from './cacheConfig';
import { emitMemoryEviction } from './cacheMemory';

const STORE_BODIES = 'bodies' as const;
const STORE_DETAILS = 'details' as const;
const STORE_SESSION_PAGES = 'sessionPages' as const;
const STORE_API = 'apiResponses' as const;

export interface IdbEnvelope<T> {
	schemaVersion: number;
	sessionId: string | null;
	bytes: number;
	lastAccess: number;
	storedAt: number;
	payload: T;
}

export type BodyKey = [number, BodyKind];

export type SessionPageKey = [string, number];

let dbPromise: Promise<IDBPDatabase | null> | null = null;

async function openCacheDb(): Promise<IDBPDatabase | null> {
	if (typeof indexedDB === 'undefined') return null;

	if (dbPromise) return dbPromise;

	const cfg = getCacheConfig();
	dbPromise = openDB(cfg.idbDatabaseName, cfg.idbVersion, {
		upgrade(db, _oldVersion, _newVersion, tx) {
			for (const name of [...db.objectStoreNames]) {
				db.deleteObjectStore(name);
			}

			const bodiesStore = db.createObjectStore(STORE_BODIES, {
				keyPath: ['requestId', 'kind']
			});
			bodiesStore.createIndex('lastAccess', 'lastAccess');
			bodiesStore.createIndex('sessionId', 'sessionId');

			const detailsStore = db.createObjectStore(STORE_DETAILS, {
				keyPath: 'requestId'
			});
			detailsStore.createIndex('lastAccess', 'lastAccess');
			detailsStore.createIndex('sessionId', 'sessionId');

			const pagesStore = db.createObjectStore(STORE_SESSION_PAGES, {
				keyPath: ['sessionId', 'offset']
			});
			pagesStore.createIndex('sessionId', 'sessionId');
			pagesStore.createIndex('lastAccess', 'lastAccess');

			const apiStore = db.createObjectStore(STORE_API, {
				keyPath: 'path'
			});
			apiStore.createIndex('lastAccess', 'lastAccess');

			void tx;
		},
		blocked() {
			console.warn('[cacheIdb] IDB version upgrade blocked by another tab');
		},
		blocking() {
			dbPromise?.then((d) => d?.close());
			dbPromise = null;
		},
		terminated() {
			dbPromise = null;
		}
	}).catch((err: unknown) => {
		console.warn('[cacheIdb] IDB open failed, degrading to memory-only:', err);
		emitMemoryEviction({ source: 'idb-unavailable', id: 0, bytes: 0, reason: 'entry-cap' });
		dbPromise = null;
		return null;
	});

	return dbPromise;
}

export function __resetIdbHandleForTests(): void {
	dbPromise = null;
}

export async function __deleteIdbForTests(): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		const tx = db.transaction(
			[STORE_BODIES, STORE_DETAILS, STORE_SESSION_PAGES, STORE_API],
			'readwrite'
		);
		await Promise.all([
			tx.objectStore(STORE_BODIES).clear(),
			tx.objectStore(STORE_DETAILS).clear(),
			tx.objectStore(STORE_SESSION_PAGES).clear(),
			tx.objectStore(STORE_API).clear()
		]);
		await tx.done;
	} catch {}
}

function isCurrentSchema<T>(envelope: IdbEnvelope<T>): boolean {
	return envelope.schemaVersion === getCacheConfig().schemaVersion;
}

const IDB_READ_TIMEOUT_MS = 1500;

export function idbReadWithTimeout<T>(idbPromise: Promise<T | null>): Promise<T | null> {
	return new Promise<T | null>((resolve) => {
		const timer = setTimeout(() => resolve(null), IDB_READ_TIMEOUT_MS);
		idbPromise.then(
			(val) => {
				clearTimeout(timer);
				resolve(val);
			},
			() => {
				clearTimeout(timer);
				resolve(null);
			}
		);
	});
}

function estimateBytes(value: unknown): number {
	try {
		return JSON.stringify(value)?.length ?? 0;
	} catch {
		return 0;
	}
}

interface StoredBody extends IdbEnvelope<BodyPayload> {
	requestId: number;
	kind: BodyKind;
}

export async function idbGetBody(id: number, kind: BodyKind): Promise<BodyPayload | null> {
	if (typeof indexedDB === 'undefined') return null;
	try {
		const db = await openCacheDb();
		if (!db) return null;
		const record = await db.get(STORE_BODIES, [id, kind] as BodyKey);
		if (!record) return null;
		if (!isCurrentSchema(record)) {
			void db.delete(STORE_BODIES, [id, kind] as BodyKey).catch(() => {});
			return null;
		}
		const updated: StoredBody = { ...record, lastAccess: Date.now() };
		void db.put(STORE_BODIES, updated).catch(() => {});
		return record.payload;
	} catch (err) {
		console.warn('[cacheIdb] idbGetBody error:', err);
		return null;
	}
}

export async function idbPutBody(
	id: number,
	kind: BodyKind,
	sessionId: string | null,
	payload: BodyPayload
): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		const cfg = getCacheConfig();
		const now = Date.now();
		const record: StoredBody = {
			requestId: id,
			kind,
			schemaVersion: cfg.schemaVersion,
			sessionId,
			bytes: payload.byte_size,
			lastAccess: now,
			storedAt: now,
			payload
		};
		await db.put(STORE_BODIES, record);
		await evictBodiesIfNeeded(db);
	} catch (err) {
		console.warn('[cacheIdb] idbPutBody error:', err);
	}
}

export async function idbGetBodies(
	keys: Array<[number, BodyKind]>
): Promise<Map<string, BodyPayload>> {
	const result = new Map<string, BodyPayload>();
	if (typeof indexedDB === 'undefined' || keys.length === 0) return result;
	try {
		const db = await openCacheDb();
		if (!db) return result;
		const tx = db.transaction(STORE_BODIES, 'readonly');
		const store = tx.objectStore(STORE_BODIES);
		const gets = keys.map(([id, kind]) =>
			store.get([id, kind] as BodyKey).then((record) => {
				if (!record || !isCurrentSchema(record)) return;
				result.set(`${id}:${kind}`, record.payload);
			})
		);
		await Promise.all(gets);
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] idbGetBodies error:', err);
	}
	return result;
}

async function evictBodiesIfNeeded(db: IDBPDatabase): Promise<void> {
	const cfg = getCacheConfig();
	try {
		let totalBytes = 0;
		{
			const tx = db.transaction(STORE_BODIES, 'readonly');
			let cursor = await tx.objectStore(STORE_BODIES).index('lastAccess').openCursor();
			while (cursor) {
				const rec = cursor.value as StoredBody;
				totalBytes += rec.bytes;
				cursor = await cursor.continue();
			}
			await tx.done;
		}

		if (totalBytes <= cfg.idbBodyBudgetBytes) return;

		const tx = db.transaction(STORE_BODIES, 'readwrite');
		let cursor = await tx.objectStore(STORE_BODIES).index('lastAccess').openCursor();
		while (cursor && totalBytes > cfg.idbBodyBudgetBytes) {
			const rec = cursor.value as StoredBody;
			totalBytes -= rec.bytes;
			await cursor.delete();
			emitMemoryEviction({
				source: 'idb-body',
				id: rec.requestId,
				kind: rec.kind,
				bytes: rec.bytes,
				reason: 'byte-cap'
			});
			cursor = await cursor.continue();
		}
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] evictBodiesIfNeeded error:', err);
	}
}

interface StoredDetail extends IdbEnvelope<RequestDetail> {
	requestId: number;
}

export async function idbGetDetail(id: number): Promise<RequestDetail | null> {
	if (typeof indexedDB === 'undefined') return null;
	try {
		const db = await openCacheDb();
		if (!db) return null;
		const record = await db.get(STORE_DETAILS, id);
		if (!record) return null;
		if (!isCurrentSchema(record)) {
			void db.delete(STORE_DETAILS, id).catch(() => {});
			return null;
		}
		const updated: StoredDetail = { ...record, lastAccess: Date.now() };
		void db.put(STORE_DETAILS, updated).catch(() => {});
		return record.payload;
	} catch (err) {
		console.warn('[cacheIdb] idbGetDetail error:', err);
		return null;
	}
}

export async function idbPutDetail(
	id: number,
	sessionId: string | null,
	payload: RequestDetail
): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		const cfg = getCacheConfig();
		const now = Date.now();
		const record: StoredDetail = {
			requestId: id,
			schemaVersion: cfg.schemaVersion,
			sessionId,
			bytes: estimateBytes(payload),
			lastAccess: now,
			storedAt: now,
			payload
		};
		await db.put(STORE_DETAILS, record);
		await evictDetailsIfNeeded(db);
	} catch (err) {
		console.warn('[cacheIdb] idbPutDetail error:', err);
	}
}

export async function idbGetDetails(ids: number[]): Promise<Map<number, RequestDetail>> {
	const result = new Map<number, RequestDetail>();
	if (typeof indexedDB === 'undefined' || ids.length === 0) return result;
	try {
		const db = await openCacheDb();
		if (!db) return result;
		const tx = db.transaction(STORE_DETAILS, 'readonly');
		const store = tx.objectStore(STORE_DETAILS);
		await Promise.all(
			ids.map((id) =>
				store.get(id).then((record) => {
					if (!record || !isCurrentSchema(record)) return;
					result.set(id, record.payload);
				})
			)
		);
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] idbGetDetails error:', err);
	}
	return result;
}

async function evictDetailsIfNeeded(db: IDBPDatabase): Promise<void> {
	const cfg = getCacheConfig();
	try {
		const count = await db.count(STORE_DETAILS);
		if (count <= cfg.idbDetailCap) return;
		const toDelete = count - cfg.idbDetailCap;
		const tx = db.transaction(STORE_DETAILS, 'readwrite');
		let cursor = await tx.objectStore(STORE_DETAILS).index('lastAccess').openCursor();
		let deleted = 0;
		while (cursor && deleted < toDelete) {
			const rec = cursor.value as StoredDetail;
			await cursor.delete();
			emitMemoryEviction({
				source: 'idb-detail',
				id: rec.requestId,
				bytes: rec.bytes,
				reason: 'entry-cap'
			});
			deleted++;
			cursor = await cursor.continue();
		}
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] evictDetailsIfNeeded error:', err);
	}
}

interface StoredPage extends IdbEnvelope<RequestSummary[]> {
	sessionId: string;
	offset: number;
}

export async function idbGetSessionPage(
	sessionId: string,
	offset: number
): Promise<RequestSummary[] | null> {
	if (typeof indexedDB === 'undefined') return null;
	try {
		const db = await openCacheDb();
		if (!db) return null;
		const record = await db.get(STORE_SESSION_PAGES, [sessionId, offset] as SessionPageKey);
		if (!record) return null;
		if (!isCurrentSchema(record)) {
			void db.delete(STORE_SESSION_PAGES, [sessionId, offset] as SessionPageKey).catch(() => {});
			return null;
		}
		const updated: StoredPage = { ...record, lastAccess: Date.now() };
		void db.put(STORE_SESSION_PAGES, updated).catch(() => {});
		return record.payload;
	} catch (err) {
		console.warn('[cacheIdb] idbGetSessionPage error:', err);
		return null;
	}
}

export async function idbPutSessionPage(
	sessionId: string,
	offset: number,
	payload: RequestSummary[]
): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		const cfg = getCacheConfig();
		const now = Date.now();
		const record: StoredPage = {
			sessionId,
			offset,
			schemaVersion: cfg.schemaVersion,
			bytes: estimateBytes(payload),
			lastAccess: now,
			storedAt: now,
			payload
		};
		await db.put(STORE_SESSION_PAGES, record);
		await evictSessionPagesIfNeeded(db);
	} catch (err) {
		console.warn('[cacheIdb] idbPutSessionPage error:', err);
	}
}

export async function idbDeleteSessionPage(sessionId: string, offset: number): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		await db.delete(STORE_SESSION_PAGES, [sessionId, offset] as SessionPageKey);
	} catch (err) {
		console.warn('[cacheIdb] idbDeleteSessionPage error:', err);
	}
}

export async function idbGetAllSessionPages(
	sessionId: string
): Promise<Map<number, RequestSummary[]>> {
	const result = new Map<number, RequestSummary[]>();
	if (typeof indexedDB === 'undefined') return result;
	try {
		const db = await openCacheDb();
		if (!db) return result;
		const tx = db.transaction(STORE_SESSION_PAGES, 'readonly');
		const index = tx.objectStore(STORE_SESSION_PAGES).index('sessionId');
		let cursor = await index.openCursor(IDBKeyRange.only(sessionId));
		while (cursor) {
			const rec = cursor.value as StoredPage;
			if (isCurrentSchema(rec)) {
				result.set(rec.offset, rec.payload);
			}
			cursor = await cursor.continue();
		}
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] idbGetAllSessionPages error:', err);
	}
	return result;
}

export async function idbInvalidateSession(sessionId: string): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;

		{
			const tx = db.transaction(STORE_BODIES, 'readwrite');
			const index = tx.objectStore(STORE_BODIES).index('sessionId');
			let cursor = await index.openCursor(IDBKeyRange.only(sessionId));
			while (cursor) {
				await cursor.delete();
				cursor = await cursor.continue();
			}
			await tx.done;
		}

		{
			const tx = db.transaction(STORE_DETAILS, 'readwrite');
			const index = tx.objectStore(STORE_DETAILS).index('sessionId');
			let cursor = await index.openCursor(IDBKeyRange.only(sessionId));
			while (cursor) {
				await cursor.delete();
				cursor = await cursor.continue();
			}
			await tx.done;
		}

		{
			const tx = db.transaction(STORE_SESSION_PAGES, 'readwrite');
			const index = tx.objectStore(STORE_SESSION_PAGES).index('sessionId');
			let cursor = await index.openCursor(IDBKeyRange.only(sessionId));
			while (cursor) {
				await cursor.delete();
				cursor = await cursor.continue();
			}
			await tx.done;
		}
	} catch (err) {
		console.warn('[cacheIdb] idbInvalidateSession error:', err);
	}
}

async function evictSessionPagesIfNeeded(db: IDBPDatabase): Promise<void> {
	const cfg = getCacheConfig();
	try {
		const count = await db.count(STORE_SESSION_PAGES);
		if (count <= cfg.idbSessionPageCap) return;
		const toDelete = count - cfg.idbSessionPageCap;
		const tx = db.transaction(STORE_SESSION_PAGES, 'readwrite');
		let cursor = await tx.objectStore(STORE_SESSION_PAGES).index('lastAccess').openCursor();
		let deleted = 0;
		while (cursor && deleted < toDelete) {
			const rec = cursor.value as StoredPage;
			await cursor.delete();
			emitMemoryEviction({
				source: 'idb-session-page',
				id: rec.sessionId,
				bytes: rec.bytes,
				reason: 'entry-cap'
			});
			deleted++;
			cursor = await cursor.continue();
		}
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] evictSessionPagesIfNeeded error:', err);
	}
}

interface StoredApiResponse extends IdbEnvelope<unknown> {
	path: string;
}

export async function idbGetApiResponse(path: string): Promise<unknown | null> {
	if (typeof indexedDB === 'undefined') return null;
	try {
		const db = await openCacheDb();
		if (!db) return null;
		const record = (await db.get(STORE_API, path)) as StoredApiResponse | undefined;
		if (!record) return null;
		if (!isCurrentSchema(record)) {
			void db.delete(STORE_API, path).catch(() => {});
			return null;
		}
		const updated: StoredApiResponse = { ...record, lastAccess: Date.now() };
		void db.put(STORE_API, updated).catch(() => {});
		return record.payload;
	} catch (err) {
		console.warn('[cacheIdb] idbGetApiResponse error:', err);
		return null;
	}
}

export async function idbPutApiResponse(path: string, payload: unknown): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		const cfg = getCacheConfig();
		const now = Date.now();
		const record: StoredApiResponse = {
			path,
			schemaVersion: cfg.schemaVersion,
			sessionId: null,
			bytes: estimateBytes(payload),
			lastAccess: now,
			storedAt: now,
			payload
		};
		await db.put(STORE_API, record);
		await evictApiResponsesIfNeeded(db);
	} catch (err) {
		console.warn('[cacheIdb] idbPutApiResponse error:', err);
	}
}

export async function idbInvalidateApiResponses(prefix: string): Promise<void> {
	if (typeof indexedDB === 'undefined') return;
	try {
		const db = await openCacheDb();
		if (!db) return;
		const tx = db.transaction(STORE_API, 'readwrite');
		let cursor = await tx.objectStore(STORE_API).openCursor();
		while (cursor) {
			const rec = cursor.value as StoredApiResponse;
			if (prefix === '' || rec.path.startsWith(prefix)) {
				await cursor.delete();
			}
			cursor = await cursor.continue();
		}
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] idbInvalidateApiResponses error:', err);
	}
}

async function evictApiResponsesIfNeeded(db: IDBPDatabase): Promise<void> {
	const cfg = getCacheConfig();
	try {
		let totalBytes = 0;
		{
			const tx = db.transaction(STORE_API, 'readonly');
			let cursor = await tx.objectStore(STORE_API).index('lastAccess').openCursor();
			while (cursor) {
				totalBytes += (cursor.value as StoredApiResponse).bytes;
				cursor = await cursor.continue();
			}
			await tx.done;
		}
		if (totalBytes <= cfg.apiResponseBudgetBytes) return;
		const tx = db.transaction(STORE_API, 'readwrite');
		let cursor = await tx.objectStore(STORE_API).index('lastAccess').openCursor();
		while (cursor && totalBytes > cfg.apiResponseBudgetBytes) {
			const rec = cursor.value as StoredApiResponse;
			totalBytes -= rec.bytes;
			await cursor.delete();
			emitMemoryEviction({ source: 'idb-api', id: rec.path, bytes: rec.bytes, reason: 'byte-cap' });
			cursor = await cursor.continue();
		}
		await tx.done;
	} catch (err) {
		console.warn('[cacheIdb] evictApiResponsesIfNeeded error:', err);
	}
}

export async function __idbCountsForTests(): Promise<{
	bodies: number;
	details: number;
	sessionPages: number;
	apiResponses: number;
}> {
	const zero = { bodies: 0, details: 0, sessionPages: 0, apiResponses: 0 };
	if (typeof indexedDB === 'undefined') return zero;
	try {
		const db = await openCacheDb();
		if (!db) return zero;
		const [bodies, details, sessionPages, apiResponses] = await Promise.all([
			db.count(STORE_BODIES),
			db.count(STORE_DETAILS),
			db.count(STORE_SESSION_PAGES),
			db.count(STORE_API)
		]);
		return { bodies, details, sessionPages, apiResponses };
	} catch {
		return zero;
	}
}
