import { getMemoryBreakdown, MemoryApiError, type MemoryBreakdown } from '$lib/api/memory';
import { proxyEvents as defaultProxyEvents } from '$lib/ws.svelte';
import type { ProxyEvent } from '$lib/types';

export interface FieldBreakdown {
	field: string;
	bytes: number;
}

export interface MemorySnapshot {
	entries_total_bytes: number;
	max_store_bytes: number;
	rss_bytes: number;
	breakdown: FieldBreakdown[];
	history: number[];
}

export interface ThroughputSnapshot {
	req_per_min: number;
	tokens_per_sec: number;
	error_rate_pct: number;
	req_history: number[];
	tokens_history: number[];
}

export type WsState = 'connected' | 'connecting' | 'disconnected' | 'stale';

export interface HealthSnapshot {
	in_flight: number;
	broadcast_receivers: number;
	broadcast_backlog: number;
	db_pool_size: number | null;
	db_pool_idle: number | null;
	ws_state: WsState;
}

export const HISTORY_RING_CAP = 120;

export const THROUGHPUT_WINDOW_MS = 60_000;

export const FALLBACK_POLL_MS = 3_000;

interface PendingRequest {
	id: number;
	input_tokens: number | null;
	output_tokens: number | null;
	startedAtMs: number;
}

interface ThroughputEvent {
	at: number;
	kind: 'request' | 'error';
	tokens: number;
}

export interface LiveSystemStore {
	readonly memory: MemorySnapshot | null;
	readonly throughput: ThroughputSnapshot;
	readonly health: HealthSnapshot;
	readonly loading: boolean;
	readonly error: string | null;
	readonly stale: boolean;
	start(): () => void;
	__drainForTests(): void;
	__fetchForTests(): Promise<void>;
}

interface Deps {
	proxyEvents?: typeof defaultProxyEvents;
	fetchMemory?: typeof getMemoryBreakdown;
	pollMs?: number;
}

export function createLiveSystemStore(deps: Deps = {}): LiveSystemStore {
	const ws = deps.proxyEvents ?? defaultProxyEvents;
	const fetchMemory = deps.fetchMemory ?? getMemoryBreakdown;
	const pollMs = deps.pollMs ?? FALLBACK_POLL_MS;

	const s = $state<{
		memory: MemorySnapshot | null;
		throughput: ThroughputSnapshot;
		health: HealthSnapshot;
		loading: boolean;
		error: string | null;
		stale: boolean;
	}>({
		memory: null,
		throughput: {
			req_per_min: 0,
			tokens_per_sec: 0,
			error_rate_pct: 0,
			req_history: [],
			tokens_history: []
		},
		health: {
			in_flight: 0,
			broadcast_receivers: 0,
			broadcast_backlog: 0,
			db_pool_size: null,
			db_pool_idle: null,
			ws_state: 'connecting'
		},
		loading: true,
		error: null,
		stale: true
	});

	const pending = new Map<number, PendingRequest>();
	const throughputEvents: ThroughputEvent[] = [];
	const memHistory: number[] = [];
	const reqRateHistory: number[] = [];
	const tokRateHistory: number[] = [];

	let processedEvents = 0;
	let processedLagged = 0;
	let pollTimer: ReturnType<typeof setInterval> | null = null;
	let stopEffect: (() => void) | null = null;
	let started = false;
	let inFlight: Promise<void> | null = null;

	function breakdownRows(t: {
		request_body_bytes: number;
		response_body_bytes: number;
		filtered_request_body_bytes: number;
		tool_calls_bytes: number;
		tool_results_bytes: number;
		tool_results_in_body_bytes: number;
		system_prompt_blocks_bytes: number;
	}): FieldBreakdown[] {
		return [
			{ field: 'request_body', bytes: t.request_body_bytes },
			{ field: 'response_body', bytes: t.response_body_bytes },
			{ field: 'filtered_request_body', bytes: t.filtered_request_body_bytes },
			{ field: 'tool_calls', bytes: t.tool_calls_bytes },
			{ field: 'tool_results', bytes: t.tool_results_bytes },
			{ field: 'tool_results_in_body', bytes: t.tool_results_in_body_bytes },
			{ field: 'system_prompt_blocks', bytes: t.system_prompt_blocks_bytes }
		];
	}

	function applyMemoryBreakdown(b: MemoryBreakdown): void {
		if (memHistory.length === 0) {
			memHistory.push(b.entries_total_bytes);
		}
		s.memory = {
			entries_total_bytes: b.entries_total_bytes,
			max_store_bytes: b.max_store_bytes,
			rss_bytes: b.process_rss_bytes ?? 0,
			breakdown: breakdownRows(b),
			history: [...memHistory]
		};
	}

	function applyMemoryTick(t: Extract<ProxyEvent, { type: 'MemoryTick' }>): void {
		pushRing(memHistory, t.entries_total_bytes, HISTORY_RING_CAP);
		s.memory = {
			entries_total_bytes: t.entries_total_bytes,
			max_store_bytes: t.max_store_bytes,
			rss_bytes: t.process_rss_bytes ?? 0,
			breakdown: breakdownRows(t),
			history: [...memHistory]
		};
		s.health = {
			...s.health,
			in_flight: t.in_flight_count,
			broadcast_receivers: t.broadcast_receivers,
			broadcast_backlog: t.broadcast_backlog,
			db_pool_size: t.db_pool_size,
			db_pool_idle: t.db_pool_idle
		};
		s.stale = false;
		s.loading = false;
	}

	function pushRing(ring: number[], v: number, cap: number): void {
		ring.push(v);
		if (ring.length > cap) ring.splice(0, ring.length - cap);
	}

	function ensurePending(id: number, nowMs: number): PendingRequest {
		let p = pending.get(id);
		if (!p) {
			p = {
				id,
				input_tokens: null,
				output_tokens: null,
				startedAtMs: nowMs
			};
			pending.set(id, p);
		}
		return p;
	}

	function handleEvent(ev: ProxyEvent, nowMs: number): void {
		switch (ev.type) {
			case 'MemoryTick':
				applyMemoryTick(ev);
				return;

			case 'StreamingStarted': {
				ensurePending(ev.id, nowMs);
				throughputEvents.push({ at: nowMs, kind: 'request', tokens: 0 });
				return;
			}

			case 'UsageReceived': {
				const p = pending.get(ev.id);
				if (!p) return;
				p.input_tokens = ev.usage.input_tokens || p.input_tokens;
				p.output_tokens = ev.usage.output_tokens || p.output_tokens;
				return;
			}

			case 'ResponseComplete': {
				const p = pending.get(ev.id);
				if (!p) return;
				if (ev.usage) {
					p.input_tokens = ev.usage.input_tokens || p.input_tokens;
					p.output_tokens = ev.usage.output_tokens || p.output_tokens;
				}
				const tokens = (p.input_tokens ?? 0) + (p.output_tokens ?? 0);
				throughputEvents.push({ at: nowMs, kind: 'request', tokens });
				return;
			}

			case 'ResponseError': {
				const p = pending.get(ev.id);
				if (!p) return;
				throughputEvents.push({ at: nowMs, kind: 'error', tokens: 0 });
				return;
			}
		}
	}

	function recomputeThroughput(nowMs: number): void {
		const cutoff = nowMs - THROUGHPUT_WINDOW_MS;
		let drop = 0;
		while (drop < throughputEvents.length) {
			const e = throughputEvents[drop];
			if (e === undefined || e.at >= cutoff) break;
			drop++;
		}
		if (drop > 0) throughputEvents.splice(0, drop);

		let reqs = 0;
		let errors = 0;
		let tokens = 0;
		for (const e of throughputEvents) {
			if (e.kind === 'request') {
				reqs++;
				tokens += e.tokens;
			} else {
				errors++;
			}
		}
		const windowSecs = THROUGHPUT_WINDOW_MS / 1000;
		const total = reqs + errors;
		const reqPerMin = reqs;
		const tokensPerSec = tokens / windowSecs;
		const errorRate = total > 0 ? (errors / total) * 100 : 0;

		pushRing(reqRateHistory, reqPerMin, HISTORY_RING_CAP);
		pushRing(tokRateHistory, tokensPerSec, HISTORY_RING_CAP);

		s.throughput = {
			req_per_min: reqPerMin,
			tokens_per_sec: tokensPerSec,
			error_rate_pct: errorRate,
			req_history: [...reqRateHistory],
			tokens_history: [...tokRateHistory]
		};
	}

	function mapWsState(): WsState {
		switch (ws.state) {
			case 'connected':
				return 'connected';
			case 'connecting':
			case 'reconnecting':
				return 'connecting';
			case 'disconnected':
				return 'disconnected';
			default:
				return 'connecting';
		}
	}

	function refreshHealthWsState(): void {
		const mapped = mapWsState();
		const ws_state: WsState = mapped === 'connected' && s.stale ? 'stale' : mapped;
		if (s.health.ws_state !== ws_state) {
			s.health = { ...s.health, ws_state };
		}
	}

	async function fetchOnce(): Promise<void> {
		if (inFlight) return inFlight;
		const p = (async (): Promise<void> => {
			try {
				const b = await fetchMemory();
				applyMemoryBreakdown(b);
				s.error = null;
			} catch (err) {
				s.error = err instanceof MemoryApiError ? err.message : String(err);
			} finally {
				s.loading = false;
				inFlight = null;
			}
		})();
		inFlight = p;
		return p;
	}

	function armFallbackPoll(): void {
		if (pollTimer !== null) return;
		pollTimer = setInterval(() => {
			if (ws.state !== 'connected') {
				void fetchOnce();
			}
		}, pollMs);
	}

	function disarmFallbackPoll(): void {
		if (pollTimer !== null) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function drain(): void {
		const nowMs = Date.now();
		const total = ws.totalEvents;
		if (total !== processedEvents) {
			const start = total < processedEvents ? 0 : Math.max(processedEvents, total - 1000);
			for (let i = start; i < total; i++) {
				const ev = ws.at(i);
				if (!ev) continue;
				handleEvent(ev, nowMs);
			}
			processedEvents = total;
		}

		const lagged = ws.laggedCount;
		if (lagged !== processedLagged) {
			processedLagged = lagged;
		}

		recomputeThroughput(nowMs);
		refreshHealthWsState();
	}

	function start(): () => void {
		if (started) {
			return () => {};
		}
		started = true;

		void fetchOnce();
		armFallbackPoll();

		const cleanupRoot = $effect.root(() => {
			$effect(() => {
				void ws.totalEvents;
				void ws.laggedCount;
				void ws.state;
				drain();
			});
		});

		const heartbeat = setInterval(() => {
			drain();
		}, 1_000);

		stopEffect = () => {
			cleanupRoot();
			clearInterval(heartbeat);
		};

		return () => {
			disarmFallbackPoll();
			if (stopEffect) {
				stopEffect();
				stopEffect = null;
			}
			started = false;
		};
	}

	return {
		get memory() {
			return s.memory;
		},
		get throughput() {
			return s.throughput;
		},
		get health() {
			return s.health;
		},
		get loading() {
			return s.loading;
		},
		get error() {
			return s.error;
		},
		get stale() {
			return s.stale;
		},
		start,
		__drainForTests: drain,
		__fetchForTests: fetchOnce
	};
}
