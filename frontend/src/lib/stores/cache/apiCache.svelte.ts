import { getCacheConfig } from './cacheConfig';
import { BoundedLruCache, emitMemoryEviction } from './cacheMemory';
import {
	idbGetApiResponse,
	idbPutApiResponse,
	idbInvalidateApiResponses,
	idbReadWithTimeout
} from './cacheIdb';

export type CachePolicy = 'immutable' | 'swr' | 'passthrough';

let memory: BoundedLruCache<string, unknown> | null = null;

function mem(): BoundedLruCache<string, unknown> {
	if (!memory) {
		const cfg = getCacheConfig();
		memory = new BoundedLruCache<string, unknown>(
			cfg.apiResponseMemoryEntryCap,
			0,
			() => 0,
			(key) => emitMemoryEviction({ source: 'memory-api', id: key, bytes: 0, reason: 'entry-cap' })
		);
	}
	return memory;
}

const inFlight = new Map<string, Promise<unknown>>();
const revalidating = new Set<string>();
const revalidatedAt = new Map<string, number>();

const RE_FRESH_STRUCTURAL =
	/^\/api\/(settings|steerers|filter\/rules)$|^\/api\/sessions\/[^/]+\/(analyzers|filter-rules|enrolments)$/;
const RE_FRESH_COUNTERS = /^\/api\/(stats|health)$|^\/api\/sessions(\?|$)/;

const FRESH_STRUCTURAL_MS = 300_000;
const FRESH_COUNTERS_MS = 15_000;
const FRESH_SESSION_MS = 30_000;

export function freshnessWindowMs(path: string): number {
	const p = path.split('?')[0] ?? path;
	if (RE_FRESH_STRUCTURAL.test(p)) return FRESH_STRUCTURAL_MS;
	if (RE_FRESH_COUNTERS.test(path)) return FRESH_COUNTERS_MS;
	return FRESH_SESSION_MS;
}

function isFresh(path: string, now: number): boolean {
	const last = revalidatedAt.get(path);
	return last !== undefined && now - last < freshnessWindowMs(path);
}

type UpdateListener = (value: unknown) => void;
const updateListeners = new Map<string, Set<UpdateListener>>();

interface PrefixEntry {
	prefix: string;
	listener: UpdateListener;
}
const prefixListeners: PrefixEntry[] = [];

export function onApiCacheUpdate(
	key: string,
	listener: UpdateListener,
	opts?: { prefix?: boolean }
): () => void {
	if (opts?.prefix) {
		const entry: PrefixEntry = { prefix: key, listener };
		prefixListeners.push(entry);
		return () => {
			const idx = prefixListeners.indexOf(entry);
			if (idx !== -1) prefixListeners.splice(idx, 1);
		};
	}

	let set = updateListeners.get(key);
	if (!set) {
		set = new Set();
		updateListeners.set(key, set);
	}
	set.add(listener);
	return () => {
		const s = updateListeners.get(key);
		if (!s) return;
		s.delete(listener);
		if (s.size === 0) updateListeners.delete(key);
	};
}

function emitUpdate(path: string, value: unknown): void {
	const set = updateListeners.get(path);
	if (set) {
		for (const listener of set) {
			try {
				listener(value);
			} catch (err) {
				console.warn('[apiCache] update listener threw', err);
			}
		}
	}
	for (const entry of prefixListeners) {
		if (path.startsWith(entry.prefix)) {
			try {
				entry.listener(value);
			} catch (err) {
				console.warn('[apiCache] update listener threw', err);
			}
		}
	}
}

function jsonEqual(a: unknown, b: unknown): boolean {
	try {
		return JSON.stringify(a) === JSON.stringify(b);
	} catch {
		return false;
	}
}

const RE_BODY = /^\/api\/requests\/\d+\/body\/(request|request_sent|response)$/;
const RE_DETAIL = /^\/api\/requests\/\d+$/;
const RE_IMMUTABLE = /^\/api\/requests\/\d+\/stream_chunks$/;
const RE_IMMUTABLE_DELTAS = /^\/api\/sessions\/[^/]+\/requests\/\d+\/deltas$/;

export function policyForPath(path: string): CachePolicy {
	const p = path.split('?')[0] ?? path;
	if (RE_BODY.test(p) || RE_DETAIL.test(p)) return 'passthrough';
	if (RE_IMMUTABLE.test(p) || RE_IMMUTABLE_DELTAS.test(p)) return 'immutable';
	return 'swr';
}

export async function cachedGet<T>(
	path: string,
	fetcher: () => Promise<T>,
	policy?: CachePolicy
): Promise<T> {
	const pol = policy ?? policyForPath(path);
	if (pol === 'passthrough') return fetcher();
	if (!getCacheConfig().enabled) return fetcher();

	const cache = mem();

	const fromMem = cache.get(path);
	if (fromMem !== undefined) {
		if (pol === 'swr' && !isFresh(path, Date.now())) void revalidate(path, fetcher);
		return fromMem as T;
	}

	const existing = inFlight.get(path);
	if (existing) return existing as Promise<T>;

	const promise = (async (): Promise<T> => {
		try {
			const fromIdb = await idbReadWithTimeout(idbGetApiResponse(path));
			if (fromIdb !== null && fromIdb !== undefined) {
				cache.set(path, fromIdb);
				if (pol === 'swr' && !isFresh(path, Date.now())) void revalidate(path, fetcher);
				return fromIdb as T;
			}
			const value = await fetcher();
			cache.set(path, value);
			revalidatedAt.set(path, Date.now());
			void idbPutApiResponse(path, value);
			return value;
		} finally {
			inFlight.delete(path);
		}
	})();
	inFlight.set(path, promise);
	return promise;
}

async function revalidate(path: string, fetcher: () => Promise<unknown>): Promise<void> {
	if (revalidating.has(path)) return;
	revalidating.add(path);
	try {
		const fresh = await fetcher();
		const cache = mem();
		const prev = cache.peek(path);
		cache.set(path, fresh);
		void idbPutApiResponse(path, fresh);
		if (!jsonEqual(prev, fresh)) emitUpdate(path, fresh);
	} catch {
	} finally {
		revalidating.delete(path);
		revalidatedAt.set(path, Date.now());
	}
}

export async function invalidateApiCache(prefix: string): Promise<void> {
	const cache = mem();
	for (const key of [...cache.keys()]) {
		if (prefix === '' || key.startsWith(prefix)) cache.delete(key);
	}
	for (const key of [...revalidatedAt.keys()]) {
		if (prefix === '' || key.startsWith(prefix)) revalidatedAt.delete(key);
	}
	await idbInvalidateApiResponses(prefix);
}

export function __resetApiCacheForTests(): void {
	memory?.clear();
	inFlight.clear();
	revalidating.clear();
	revalidatedAt.clear();
	updateListeners.clear();
	prefixListeners.length = 0;
}

export function __apiCacheMemorySizeForTests(): number {
	return memory?.size ?? 0;
}

export function __expireApiCacheFreshnessForTests(): void {
	revalidatedAt.clear();
}
