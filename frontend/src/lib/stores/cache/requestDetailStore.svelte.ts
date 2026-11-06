import { getRequest, getAccumulatedText, ApiError } from '$lib/api';
import { isAbortedRequest } from '$lib/status';
import type { ProxyEvent, ProxyEventWithId, RequestDetail, TokenUsage, ToolCall } from '$lib/types';
import { putDetailMemoryOnly, promoteDetailToIdb, onCacheEviction } from './unifiedCache.svelte';

function mergeUsage(current: TokenUsage | null, incoming: TokenUsage | null): TokenUsage | null {
	if (!incoming) return current;
	if (!current) return incoming;
	return {
		input_tokens: incoming.input_tokens || current.input_tokens,
		output_tokens: incoming.output_tokens || current.output_tokens,
		cache_creation_input_tokens:
			incoming.cache_creation_input_tokens ?? current.cache_creation_input_tokens,
		cache_read_input_tokens: incoming.cache_read_input_tokens ?? current.cache_read_input_tokens
	};
}

const PENDING_EVENT_CAP = 1000;

export const DETAIL_CACHE_CAP = 50;

export interface RequestDetailEviction {
	id: number;
	reason: 'cache-cap';
}

type RequestDetailEvictionListener = (event: RequestDetailEviction) => void;

const requestDetailEvictionListeners = new Set<RequestDetailEvictionListener>();

function emitRequestDetailEviction(event: RequestDetailEviction): void {
	for (const listener of requestDetailEvictionListeners) {
		try {
			listener(event);
		} catch (err) {
			console.warn('[requestDetailStore] eviction listener threw', err);
		}
	}
}

export function onRequestDetailEvict(listener: RequestDetailEvictionListener): () => void {
	requestDetailEvictionListeners.add(listener);
	return () => {
		requestDetailEvictionListeners.delete(listener);
	};
}

onCacheEviction((evt) => {
	if (evt.source === 'memory-detail') {
		emitRequestDetailEviction({ id: Number(evt.id), reason: 'cache-cap' });
	}
});

export interface RequestDetailStore {
	readonly detail: RequestDetail | null;
	readonly loading: boolean;
	readonly error: string | null;
	readonly initialized: boolean;
	refresh(): Promise<void>;
	handleEvent(event: ProxyEvent): void;
}

interface MutableState {
	detail: RequestDetail | null;
	loading: boolean;
	error: string | null;
	initialized: boolean;
}

interface RequestDetailEntry {
	readonly id: number;
	readonly state: MutableState;
	pending: ProxyEventWithId[];
	inFlight: Promise<void> | null;
	refresh(): Promise<void>;
	handleEvent(event: ProxyEvent): void;
}

const detailCache = new Map<number, RequestDetailEntry>();

function touch(entry: RequestDetailEntry): void {
	if (detailCache.delete(entry.id)) {
		detailCache.set(entry.id, entry);
	}
}

function evictOldest(): void {
	const first = detailCache.keys().next();
	if (!first.done) {
		const id = first.value;
		detailCache.delete(id);
		emitRequestDetailEviction({ id, reason: 'cache-cap' });
	}
}

export function __resetDetailCacheForTests(): void {
	detailCache.clear();
	requestDetailEvictionListeners.clear();
}

export function __detailCacheSizeForTests(): number {
	return detailCache.size;
}

function findOpenToolCall(calls: ToolCall[]): ToolCall | null {
	for (let i = calls.length - 1; i >= 0; i--) {
		const call = calls[i];
		if (call && !call.complete) return call;
	}
	return null;
}

function createEntry(id: number): RequestDetailEntry {
	const connState = $state<MutableState>({
		detail: null,
		loading: false,
		error: null,
		initialized: false
	});

	const entry: RequestDetailEntry = {
		id,
		state: connState,
		pending: [],
		inFlight: null,
		refresh,
		handleEvent
	};

	function applyEvent(event: ProxyEventWithId): void {
		const detail = connState.detail;
		if (!detail) return;

		switch (event.type) {
			case 'ModelConfirmed':
				detail.model = event.model;
				return;
			case 'TextDelta':
				detail.accumulated_text = (detail.accumulated_text ?? '') + event.text;
				return;
			case 'ThinkingDelta':
				detail.accumulated_thinking = (detail.accumulated_thinking ?? '') + event.thinking;
				return;
			case 'ToolUseStart': {
				const call: ToolCall = {
					tool_id: event.tool_id,
					name: event.tool_name,
					input_json: '',
					complete: false
				};
				detail.tool_calls.push(call);
				return;
			}
			case 'ToolInputDelta': {
				const open = findOpenToolCall(detail.tool_calls);
				if (!open) {
					console.warn('[requestDetailStore] ToolInputDelta with no open tool call, dropping');
					return;
				}
				open.input_json += event.partial_json;
				return;
			}
			case 'ToolUseEnd': {
				const open = findOpenToolCall(detail.tool_calls);
				if (open) open.complete = true;
				return;
			}
			case 'ResponseComplete':
				detail.stop_reason = event.stop_reason;
				if (event.usage !== null) {
					detail.token_usage = mergeUsage(detail.token_usage, event.usage);
				}
				detail.completed_at = new Date().toISOString();
				detail.status = 'Complete';
				if (!isAbortedRequest(detail.status)) {
					void promoteDetailToIdb(detail);
				}
				return;
			case 'ResponseError':
				detail.status = { Error: event.error };
				detail.completed_at = new Date().toISOString();
				if (!isAbortedRequest(detail.status)) {
					void promoteDetailToIdb(detail);
				}
				return;
			case 'UsageReceived':
				detail.token_usage = mergeUsage(detail.token_usage, event.usage);
				return;
		}
	}

	function bufferPending(event: ProxyEventWithId): void {
		if (entry.pending.length >= PENDING_EVENT_CAP) {
			console.warn('[requestDetailStore] pending buffer overflow, discarding oldest');
			entry.pending.shift();
		}
		entry.pending.push(event);
	}

	function drainPending(): void {
		if (entry.pending.length === 0) return;
		const queued = entry.pending;
		entry.pending = [];
		for (const ev of queued) {
			applyEvent(ev);
		}
	}

	function refresh(): Promise<void> {
		if (entry.inFlight) return entry.inFlight;
		const cached = connState.detail;
		const isTerminal =
			cached !== null &&
			(cached.status === 'Complete' ||
				(typeof cached.status === 'object' && cached.status !== null && 'Error' in cached.status));
		const abortedNeedsChunks =
			cached !== null && isAbortedRequest(cached.status) && cached.accumulated_text === undefined;
		if (connState.initialized && isTerminal && !abortedNeedsChunks) {
			drainPending();
			return Promise.resolve();
		}
		connState.loading = true;
		const promise = (async (): Promise<void> => {
			try {
				const fetched = await getRequest(id);
				const needsChunkHydration =
					fetched.accumulated_text === undefined &&
					((fetched.is_streaming && fetched.status !== 'Streaming') ||
						fetched.status === 'Pending' ||
						isAbortedRequest(fetched.status));
				if (needsChunkHydration) {
					try {
						fetched.accumulated_text = await getAccumulatedText(id);
					} catch {}
				}
				connState.detail = fetched;
				connState.error = null;
				connState.initialized = true;

				const terminal =
					fetched.status === 'Complete' ||
					(typeof fetched.status === 'object' && 'Error' in fetched.status);
				const aborted = isAbortedRequest(fetched.status);
				if (terminal && !aborted) {
					void promoteDetailToIdb(fetched);
				} else {
					putDetailMemoryOnly(fetched);
				}

				drainPending();
			} catch (err) {
				connState.error = err instanceof ApiError ? err.message : String(err);
				entry.pending = [];
			} finally {
				connState.loading = false;
				entry.inFlight = null;
			}
		})();
		entry.inFlight = promise;
		return promise;
	}

	function handleEvent(event: ProxyEvent): void {
		if (event.type === 'ResyncRequired') return;
		if (event.type === 'Lagged') {
			refresh();
			return;
		}
		if (event.type === 'RowChanged') return;
		if (event.type === 'MemoryShed') return;
		if (event.type === 'MemoryTick') return;
		if (event.type === 'SteeringFired') return;
		const withId: ProxyEventWithId = event;
		if (withId.id !== id) return;
		if (withId.type === 'DiagnosticReady') {
			void refresh();
			return;
		}
		if (!connState.initialized) {
			bufferPending(withId);
			return;
		}
		applyEvent(withId);
	}

	return entry;
}

export function createRequestDetailStore(id: number): RequestDetailStore {
	let entry = detailCache.get(id);
	if (entry) {
		touch(entry);
	} else {
		if (detailCache.size >= DETAIL_CACHE_CAP) {
			evictOldest();
		}
		entry = createEntry(id);
		detailCache.set(id, entry);
		entry.refresh();
	}

	const cached = entry;
	return {
		get detail() {
			touch(cached);
			return cached.state.detail;
		},
		get loading() {
			touch(cached);
			return cached.state.loading;
		},
		get error() {
			touch(cached);
			return cached.state.error;
		},
		get initialized() {
			touch(cached);
			return cached.state.initialized;
		},
		refresh() {
			touch(cached);
			return cached.refresh();
		},
		handleEvent(event) {
			touch(cached);
			cached.handleEvent(event);
		}
	};
}
