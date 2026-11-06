import type { BodyKind } from './cacheConfig';

export type EvictionSource =
	| 'memory-body'
	| 'memory-detail'
	| 'memory-session-page'
	| 'memory-api'
	| 'idb-body'
	| 'idb-detail'
	| 'idb-session-page'
	| 'idb-api'
	| 'idb-unavailable';

export interface MemoryEvictionEvent {
	source: EvictionSource;
	id: number | string;
	kind?: BodyKind;
	bytes: number;
	reason: 'entry-cap' | 'byte-cap';
}

type MemoryEvictionListener = (event: MemoryEvictionEvent) => void;

export class BoundedLruCache<K, V> {
	private readonly map = new Map<K, V>();
	private readonly byteMap = new Map<K, number>();
	private totalBytes = 0;

	constructor(
		private readonly entryCap: number,
		private readonly byteCap: number,
		private readonly byteOf: (v: V) => number = () => 0,
		private readonly onEvict?: (
			key: K,
			value: V,
			bytes: number,
			reason: 'entry-cap' | 'byte-cap'
		) => void
	) {}

	get size(): number {
		return this.map.size;
	}

	get bytes(): number {
		return this.totalBytes;
	}

	peek(key: K): V | undefined {
		return this.map.get(key);
	}

	get(key: K): V | undefined {
		const val = this.map.get(key);
		if (val !== undefined) {
			const bytes = this.byteMap.get(key) ?? 0;
			this.map.delete(key);
			this.byteMap.delete(key);
			this.map.set(key, val);
			this.byteMap.set(key, bytes);
		}
		return val;
	}

	set(key: K, value: V): void {
		if (this.map.has(key)) {
			const oldBytes = this.byteMap.get(key) ?? 0;
			this.totalBytes -= oldBytes;
			this.map.delete(key);
			this.byteMap.delete(key);
		}
		const bytes = this.byteOf(value);
		this.map.set(key, value);
		this.byteMap.set(key, bytes);
		this.totalBytes += bytes;
		this.evict();
	}

	delete(key: K): boolean {
		if (!this.map.has(key)) return false;
		const bytes = this.byteMap.get(key) ?? 0;
		this.totalBytes -= bytes;
		this.map.delete(key);
		this.byteMap.delete(key);
		return true;
	}

	clear(): void {
		this.map.clear();
		this.byteMap.clear();
		this.totalBytes = 0;
	}

	keys(): IterableIterator<K> {
		return this.map.keys();
	}

	values(): IterableIterator<V> {
		return this.map.values();
	}

	entries(): IterableIterator<[K, V]> {
		return this.map.entries();
	}

	has(key: K): boolean {
		return this.map.has(key);
	}

	private evict(): void {
		while (
			(this.entryCap > 0 && this.map.size > this.entryCap) ||
			(this.byteCap > 0 && this.totalBytes > this.byteCap)
		) {
			const headIter = this.map.keys().next();
			if (headIter.done) break;
			const key = headIter.value;
			const val = this.map.get(key)!;
			const bytes = this.byteMap.get(key) ?? 0;
			const reason: 'entry-cap' | 'byte-cap' =
				this.entryCap > 0 && this.map.size > this.entryCap ? 'entry-cap' : 'byte-cap';
			this.totalBytes -= bytes;
			this.map.delete(key);
			this.byteMap.delete(key);
			this.onEvict?.(key, val, bytes, reason);
		}
	}
}

const memoryEvictionListeners = new Set<MemoryEvictionListener>();

export function emitMemoryEviction(event: MemoryEvictionEvent): void {
	for (const listener of memoryEvictionListeners) {
		try {
			listener(event);
		} catch (err) {
			console.warn('[cacheMemory] eviction listener threw', err);
		}
	}
}

export function onMemoryEviction(listener: MemoryEvictionListener): () => void {
	memoryEvictionListeners.add(listener);
	return () => memoryEvictionListeners.delete(listener);
}

export function __resetMemoryEvictionListenersForTests(): void {
	memoryEvictionListeners.clear();
}
