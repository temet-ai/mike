import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectionState, ProxyEvent } from '$lib/types';
import type { MemoryBreakdown } from '$lib/api/memory';
import { createLiveSystemStore } from './liveSystem.svelte';

class FakeStream {
	totalEvents = 0;
	laggedCount = 0;
	state: ConnectionState = 'connected';
	#events: ProxyEvent[] = [];

	push(ev: ProxyEvent): void {
		this.#events.push(ev);
		this.totalEvents = this.#events.length;
	}

	lag(skipped: number): void {
		this.laggedCount += skipped;
	}

	at(i: number): ProxyEvent | undefined {
		return this.#events[i];
	}
}

function makeBreakdown(over: Partial<MemoryBreakdown> = {}): MemoryBreakdown {
	return {
		entry_count: 3,
		request_body_bytes: 1000,
		response_body_bytes: 800,
		filtered_request_body_bytes: 50,
		tool_calls_bytes: 200,
		tool_results_bytes: 150,
		tool_results_in_body_bytes: 75,
		system_prompt_blocks_bytes: 25,
		entries_total_bytes: 2300,
		session_index_keys: 2,
		composable_session_index_keys: 1,
		session_meta_keys: 2,
		in_flight_count: 0,
		process_rss_bytes: 50_000,
		max_entries: 1000,
		max_store_bytes: 10_000,
		over_byte_budget: false,
		top_entries: [],
		...over
	};
}

let nowMs = 1_700_000_000_000;

beforeEach(() => {
	nowMs = 1_700_000_000_000;
	vi.spyOn(Date, 'now').mockImplementation(() => nowMs);
});

describe('cold-start fetch', () => {
	it('maps MemoryBreakdown into the UI MemorySnapshot shape', async () => {
		const ws = new FakeStream();
		const fetchMemory = vi.fn().mockResolvedValue(makeBreakdown());
		const store = createLiveSystemStore({
			proxyEvents: ws as never,
			fetchMemory
		});

		await store.__fetchForTests();

		expect(store.loading).toBe(false);
		expect(store.error).toBeNull();
		expect(store.memory).not.toBeNull();
		expect(store.memory?.entries_total_bytes).toBe(2300);
		expect(store.memory?.max_store_bytes).toBe(10_000);
		expect(store.memory?.rss_bytes).toBe(50_000);
		expect(store.memory?.breakdown.map((b) => b.field)).toEqual([
			'request_body',
			'response_body',
			'filtered_request_body',
			'tool_calls',
			'tool_results',
			'tool_results_in_body',
			'system_prompt_blocks'
		]);
		expect(store.memory?.history).toEqual([2300]);
	});

	it('captures the fetch error and stays not-stale-resolved', async () => {
		const ws = new FakeStream();
		const fetchMemory = vi.fn().mockRejectedValue(new Error('boom'));
		const store = createLiveSystemStore({
			proxyEvents: ws as never,
			fetchMemory
		});

		await store.__fetchForTests();

		expect(store.error).toBe('Error: boom');
		expect(store.memory).toBeNull();
		expect(store.loading).toBe(false);
	});
});

describe('MemoryTick → snapshot + health', () => {
	it('replaces the snapshot, grows the history ring, fills pool/backlog', () => {
		const ws = new FakeStream();
		const store = createLiveSystemStore({ proxyEvents: ws as never });

		ws.push({
			type: 'MemoryTick',
			entry_count: 5,
			request_body_bytes: 1,
			response_body_bytes: 2,
			filtered_request_body_bytes: 3,
			tool_calls_bytes: 4,
			tool_results_bytes: 5,
			tool_results_in_body_bytes: 6,
			system_prompt_blocks_bytes: 7,
			entries_total_bytes: 28,
			session_index_keys: 1,
			composable_session_index_keys: 1,
			session_meta_keys: 1,
			in_flight_count: 2,
			process_rss_bytes: 99_000,
			fastembed_model_bytes: 0,
			max_entries: 1000,
			max_store_bytes: 100,
			over_byte_budget: false,
			broadcast_receivers: 3,
			broadcast_backlog: 7,
			db_pool_size: 5,
			db_pool_idle: 1,
			tick_duration_ms: 2
		});
		store.__drainForTests();

		expect(store.memory?.entries_total_bytes).toBe(28);
		expect(store.memory?.history).toEqual([28]);
		expect(store.stale).toBe(false);
		expect(store.health.in_flight).toBe(2);
		expect(store.health.broadcast_receivers).toBe(3);
		expect(store.health.broadcast_backlog).toBe(7);
		expect(store.health.db_pool_size).toBe(5);
		expect(store.health.db_pool_idle).toBe(1);
	});
});

describe('throughput rolling-window math', () => {
	it('counts requests as req/min and sums tokens/s over the window', () => {
		const ws = new FakeStream();
		const store = createLiveSystemStore({ proxyEvents: ws as never });

		for (const id of [1, 2]) {
			ws.push({
				type: 'StreamingStarted',
				id,
				status: 200,
				ttfb_ms: 50,
				model: 'claude-opus-4-7'
			});
			ws.push({
				type: 'UsageReceived',
				id,
				usage: {
					input_tokens: 1000,
					output_tokens: 0,
					cache_creation_input_tokens: null,
					cache_read_input_tokens: null
				}
			});
			ws.push({
				type: 'ResponseComplete',
				id,
				stop_reason: 'end_turn',
				usage: {
					input_tokens: 0,
					output_tokens: 200,
					cache_creation_input_tokens: null,
					cache_read_input_tokens: null
				}
			});
		}
		store.__drainForTests();

		expect(store.throughput.req_per_min).toBe(4);
		expect(store.throughput.tokens_per_sec).toBeCloseTo(2400 / 60, 5);
		expect(store.throughput.error_rate_pct).toBe(0);
	});

	it('computes error-rate from request vs error events and evicts stale window entries', () => {
		const ws = new FakeStream();
		const store = createLiveSystemStore({ proxyEvents: ws as never });

		ws.push({
			type: 'StreamingStarted',
			id: 1,
			status: 200,
			ttfb_ms: 50,
			model: 'claude-opus-4-7'
		});
		ws.push({ type: 'ResponseError', id: 1, error: 'boom' });
		store.__drainForTests();
		expect(store.throughput.error_rate_pct).toBe(50);

		nowMs += 61_000;
		store.__drainForTests();
		expect(store.throughput.error_rate_pct).toBe(0);
		expect(store.throughput.req_per_min).toBe(0);
	});

	it('ignores terminal frames for requests it never saw start', () => {
		const ws = new FakeStream();
		const store = createLiveSystemStore({ proxyEvents: ws as never });

		ws.push({
			type: 'ResponseComplete',
			id: 99,
			stop_reason: 'end_turn',
			usage: {
				input_tokens: 100,
				output_tokens: 50,
				cache_creation_input_tokens: null,
				cache_read_input_tokens: null
			}
		});
		store.__drainForTests();

		expect(store.throughput.req_per_min).toBe(0);
		expect(store.throughput.tokens_per_sec).toBe(0);
	});
});

describe('WS-down fallback', () => {
	it('polls via REST only while the WS is not connected', async () => {
		vi.useFakeTimers();
		try {
			const ws = new FakeStream();
			ws.state = 'disconnected';
			const fetchMemory = vi.fn().mockResolvedValue(makeBreakdown());
			const store = createLiveSystemStore({
				proxyEvents: ws as never,
				fetchMemory,
				pollMs: 1_000
			});

			const stop = store.start();
			expect(fetchMemory).toHaveBeenCalledTimes(1);
			await vi.advanceTimersByTimeAsync(0);

			await vi.advanceTimersByTimeAsync(1_000);
			expect(fetchMemory).toHaveBeenCalledTimes(2);

			ws.state = 'connected';
			await vi.advanceTimersByTimeAsync(3_000);
			expect(fetchMemory).toHaveBeenCalledTimes(2);

			stop();
		} finally {
			vi.useRealTimers();
		}
	});

	it('maps ws.state to the ws_state UI enum and surfaces stale before first tick', () => {
		const ws = new FakeStream();
		ws.state = 'connected';
		const store = createLiveSystemStore({ proxyEvents: ws as never });

		store.__drainForTests();
		expect(store.health.ws_state).toBe('stale');

		ws.push({
			type: 'MemoryTick',
			entry_count: 1,
			request_body_bytes: 0,
			response_body_bytes: 0,
			filtered_request_body_bytes: 0,
			tool_calls_bytes: 0,
			tool_results_bytes: 0,
			tool_results_in_body_bytes: 0,
			system_prompt_blocks_bytes: 0,
			entries_total_bytes: 0,
			session_index_keys: 0,
			composable_session_index_keys: 0,
			session_meta_keys: 0,
			in_flight_count: 0,
			process_rss_bytes: null,
			fastembed_model_bytes: 0,
			max_entries: 1000,
			max_store_bytes: 0,
			over_byte_budget: false,
			broadcast_receivers: 1,
			broadcast_backlog: 0,
			db_pool_size: null,
			db_pool_idle: null,
			tick_duration_ms: 1
		});
		store.__drainForTests();
		expect(store.health.ws_state).toBe('connected');

		ws.state = 'disconnected';
		store.__drainForTests();
		expect(store.health.ws_state).toBe('disconnected');
	});

	it('advances the lagged cursor without throwing', () => {
		const ws = new FakeStream();
		const store = createLiveSystemStore({ proxyEvents: ws as never });
		ws.lag(3);
		expect(() => store.__drainForTests()).not.toThrow();
	});
});
