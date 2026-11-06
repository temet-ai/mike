import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestDetail, ProxyEvent, TokenUsage } from '$lib/types';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		getRequest: vi.fn(),
		getRequestSummary: vi.fn(),
		getRequestBody: vi.fn(),
		getResponseBody: vi.fn(),
		getAccumulatedText: vi.fn()
	};
});

import { ApiError, getRequest, getAccumulatedText } from '$lib/api';
import {
	DETAIL_CACHE_CAP,
	__detailCacheSizeForTests,
	__resetDetailCacheForTests,
	createRequestDetailStore,
	onRequestDetailEvict,
	type RequestDetailEviction
} from './requestDetailStore.svelte';
import { __resetCacheForTests } from './unifiedCache.svelte';
import { __resetIdbHandleForTests, __deleteIdbForTests, __idbCountsForTests } from './cacheIdb';
import { __resetMemoryEvictionListenersForTests } from './cacheMemory';

const mockGetRequest = vi.mocked(getRequest);
const mockGetAccumulatedText = vi.mocked(getAccumulatedText);

const FROZEN_NOW = new Date('2026-04-13T12:00:00Z');

function makeDetail(overrides: Partial<RequestDetail> = {}): RequestDetail {
	return {
		id: 42,
		upstream_request_id: null,
		upstream_message_id: null,
		session_id: 'sess-1',
		project: null,
		human_prompt: '',
		turn_type: 'user_prompt',
		method: 'POST',
		path: '/v1/messages',
		request_headers: [],
		request_body_size: 256,
		model: null,
		is_streaming: true,
		message_count: 1,
		is_tool_loop: false,
		last_user_message: '',
		tool_names: [],
		params: {
			max_tokens: null,
			temperature: null,
			thinking_enabled: false,
			thinking_budget: null,
			thinking_mode: null
		},
		status: 'Pending',
		response_status: null,
		response_headers: null,
		response_body_size: 0,
		accumulated_text: '',
		accumulated_thinking: '',
		tool_calls: [],
		token_usage: null,
		stop_reason: null,
		created_at: '2026-04-13T11:59:00.000Z',
		completed_at: null,
		...overrides
	};
}

function makeUsage(overrides: Partial<TokenUsage> = {}): TokenUsage {
	return {
		input_tokens: 10,
		output_tokens: 20,
		cache_creation_input_tokens: null,
		cache_read_input_tokens: null,
		...overrides
	};
}

let warnSpy: ReturnType<typeof vi.spyOn>;

beforeEach(async () => {
	await __deleteIdbForTests();
	vi.useFakeTimers();
	vi.setSystemTime(FROZEN_NOW);
	mockGetRequest.mockReset();
	mockGetAccumulatedText.mockReset();
	warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
	__resetDetailCacheForTests();
	__resetCacheForTests();
	__resetMemoryEvictionListenersForTests();
});

afterEach(() => {
	warnSpy.mockRestore();
	vi.useRealTimers();
});

describe('createRequestDetailStore', () => {
	it('populates detail on successful initial refresh', async () => {
		const base = makeDetail();
		mockGetRequest.mockResolvedValueOnce(base);

		const store = createRequestDetailStore(42);
		expect(store.loading).toBe(true);
		expect(store.initialized).toBe(false);

		await vi.waitFor(() => {
			expect(store.initialized).toBe(true);
		});

		expect(mockGetRequest).toHaveBeenCalledTimes(1);
		expect(mockGetRequest).toHaveBeenCalledWith(42);
		expect(store.detail).toEqual(base);
		expect(store.loading).toBe(false);
		expect(store.error).toBe(null);
	});

	it('aborted request hydrates partial stream chunks instead of failing blank', async () => {
		mockGetRequest.mockResolvedValueOnce(
			makeDetail({ status: { Error: 'stream aborted' }, accumulated_text: undefined })
		);
		mockGetAccumulatedText.mockResolvedValueOnce('partial bytes that arrived');

		const store = createRequestDetailStore(42);
		await vi.waitFor(() => {
			expect(store.initialized).toBe(true);
		});

		expect(mockGetAccumulatedText).toHaveBeenCalledTimes(1);
		expect(mockGetAccumulatedText).toHaveBeenCalledWith(42);
		expect(store.detail?.accumulated_text).toBe('partial bytes that arrived');
	});

	it('clean Complete with present accumulated_text does NOT refetch chunks', async () => {
		mockGetRequest.mockResolvedValueOnce(
			makeDetail({ status: 'Complete', is_streaming: false, accumulated_text: 'full body' })
		);

		const store = createRequestDetailStore(42);
		await vi.waitFor(() => {
			expect(store.initialized).toBe(true);
		});

		expect(mockGetAccumulatedText).not.toHaveBeenCalled();
		expect(store.detail?.accumulated_text).toBe('full body');
	});

	it('records error and leaves detail null when initial refresh fails', async () => {
		const apiErr = new ApiError('boom', 500, '/api/requests/42');
		mockGetRequest.mockRejectedValueOnce(apiErr);

		const store = createRequestDetailStore(42);
		await vi.waitFor(() => {
			expect(store.loading).toBe(false);
		});

		expect(store.detail).toBe(null);
		expect(store.initialized).toBe(false);
		expect(store.error).toBe('boom');
	});

	it('dedupes overlapping refresh() calls into a single fetch', async () => {
		let resolveFirst: (value: RequestDetail) => void = () => {};
		const firstCall = new Promise<RequestDetail>((resolve) => {
			resolveFirst = resolve;
		});
		mockGetRequest.mockReturnValueOnce(firstCall);

		const store = createRequestDetailStore(42);
		const second = store.refresh();
		const third = store.refresh();
		expect(second).toBe(third);
		expect(mockGetRequest).toHaveBeenCalledTimes(1);

		resolveFirst(makeDetail());
		await second;

		expect(store.detail).not.toBe(null);
		expect(mockGetRequest).toHaveBeenCalledTimes(1);
	});

	it('ModelConfirmed updates detail.model', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail({ model: null }));
		const store = createRequestDetailStore(42);
		await store.refresh();

		const event: ProxyEvent = { type: 'ModelConfirmed', id: 42, model: 'claude-sonnet-4-5' };
		store.handleEvent(event);

		expect(store.detail?.model).toBe('claude-sonnet-4-5');
	});

	it('TextDelta events append in order', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'TextDelta', id: 42, text: 'Hello' });
		store.handleEvent({ type: 'TextDelta', id: 42, text: ', ' });
		store.handleEvent({ type: 'TextDelta', id: 42, text: 'world!' });

		expect(store.detail?.accumulated_text).toBe('Hello, world!');
	});

	it('ThinkingDelta events append in order', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'ThinkingDelta', id: 42, thinking: 'let me ' });
		store.handleEvent({ type: 'ThinkingDelta', id: 42, thinking: 'think…' });

		expect(store.detail?.accumulated_thinking).toBe('let me think…');
	});

	it('tool use lifecycle rolls up into a complete ToolCall', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'ToolUseStart', id: 42, tool_id: 'toolu_01', tool_name: 'search' });
		store.handleEvent({ type: 'ToolInputDelta', id: 42, partial_json: '{"query":"' });
		store.handleEvent({ type: 'ToolInputDelta', id: 42, partial_json: 'svelte"}' });
		store.handleEvent({ type: 'ToolUseEnd', id: 42 });

		expect(store.detail?.tool_calls).toEqual([
			{ tool_id: 'toolu_01', name: 'search', input_json: '{"query":"svelte"}', complete: true }
		]);
	});

	it('ToolInputDelta with no open tool call is dropped silently with a warn', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'ToolInputDelta', id: 42, partial_json: '{"orphan":true}' });

		expect(store.detail?.tool_calls).toEqual([]);
		expect(warnSpy).toHaveBeenCalled();
		const firstArg = warnSpy.mock.calls[0]?.[0];
		expect(typeof firstArg).toBe('string');
		expect(String(firstArg).startsWith('[requestDetail')).toBe(true);
	});

	it('ResponseComplete sets stop_reason, usage, completed_at, and status', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await store.refresh();

		const usage = makeUsage({ input_tokens: 77, output_tokens: 88 });
		store.handleEvent({ type: 'ResponseComplete', id: 42, stop_reason: 'end_turn', usage });

		expect(store.detail?.stop_reason).toBe('end_turn');
		expect(store.detail?.token_usage).toEqual(usage);
		expect(store.detail?.completed_at).toBe('2026-04-13T12:00:00.000Z');
		expect(store.detail?.status).toBe('Complete');
	});

	it('ResponseError sets status and completed_at', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'ResponseError', id: 42, error: 'boom' });

		expect(store.detail?.status).toEqual({ Error: 'boom' });
		expect(store.detail?.completed_at).toBe('2026-04-13T12:00:00.000Z');
	});

	it('UsageReceived then ResponseComplete merges both slices', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail({ token_usage: null }));
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({
			type: 'UsageReceived',
			id: 42,
			usage: {
				input_tokens: 10,
				output_tokens: 0,
				cache_creation_input_tokens: 1745,
				cache_read_input_tokens: 391269
			}
		});
		store.handleEvent({
			type: 'ResponseComplete',
			id: 42,
			stop_reason: 'end_turn',
			usage: {
				input_tokens: 0,
				output_tokens: 246,
				cache_creation_input_tokens: null,
				cache_read_input_tokens: null
			}
		});

		expect(store.detail?.token_usage).toEqual({
			input_tokens: 10,
			output_tokens: 246,
			cache_creation_input_tokens: 1745,
			cache_read_input_tokens: 391269
		});
	});

	it('ResponseComplete with usage=null does not clear existing token_usage', async () => {
		const prior = {
			input_tokens: 10,
			output_tokens: 246,
			cache_creation_input_tokens: 1745,
			cache_read_input_tokens: 391269
		};
		mockGetRequest.mockResolvedValueOnce(makeDetail({ token_usage: prior }));
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'ResponseComplete', id: 42, stop_reason: 'end_turn', usage: null });

		expect(store.detail?.token_usage).toEqual(prior);
		expect(store.detail?.status).toBe('Complete');
	});

	it('Lagged triggers exactly one getRequest even on two back-to-back frames', async () => {
		let resolveInitial: (value: RequestDetail) => void = () => {};
		const first = new Promise<RequestDetail>((resolve) => {
			resolveInitial = resolve;
		});
		mockGetRequest.mockReturnValueOnce(first);

		const store = createRequestDetailStore(42);
		store.handleEvent({ type: 'Lagged', skipped: 3 });
		store.handleEvent({ type: 'Lagged', skipped: 5 });
		expect(mockGetRequest).toHaveBeenCalledTimes(1);

		resolveInitial(makeDetail());
		await vi.waitFor(() => expect(store.initialized).toBe(true));
		expect(mockGetRequest).toHaveBeenCalledTimes(1);
	});

	it('events for a different id are ignored', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail({ model: null }));
		const store = createRequestDetailStore(42);
		await store.refresh();

		store.handleEvent({ type: 'ModelConfirmed', id: 9999, model: 'should-not-stick' });
		store.handleEvent({ type: 'TextDelta', id: 9999, text: 'nope' });

		expect(store.detail?.model).toBe(null);
		expect(store.detail?.accumulated_text).toBe('');
	});

	it('buffers events that arrive before the initial refresh resolves', async () => {
		let resolveInitial: (value: RequestDetail) => void = () => {};
		const first = new Promise<RequestDetail>((resolve) => {
			resolveInitial = resolve;
		});
		mockGetRequest.mockReturnValueOnce(first);

		const store = createRequestDetailStore(42);
		store.handleEvent({ type: 'TextDelta', id: 42, text: 'early-' });
		store.handleEvent({ type: 'TextDelta', id: 42, text: 'bird' });
		store.handleEvent({ type: 'TextDelta', id: 9999, text: 'nope' });

		resolveInitial(makeDetail({ accumulated_text: '' }));
		await vi.waitFor(() => expect(store.initialized).toBe(true));

		expect(store.detail?.accumulated_text).toBe('early-bird');
	});

	it('a single store drains 100 streaming events with exactly one getRequest call', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());
		const store = createRequestDetailStore(42);
		await vi.waitFor(() => expect(store.initialized).toBe(true));

		for (let i = 0; i < 100; i++) {
			store.handleEvent({ type: 'TextDelta', id: 42, text: 'x' });
			store.handleEvent({
				type: 'UsageReceived',
				id: 42,
				usage: makeUsage({ input_tokens: i, output_tokens: i })
			});
		}

		expect(mockGetRequest).toHaveBeenCalledTimes(1);
		expect(store.detail?.accumulated_text).toBe('x'.repeat(100));
	});

	it('discards the pending buffer when the initial refresh fails', async () => {
		let rejectInitial: (reason: Error) => void = () => {};
		const first = new Promise<RequestDetail>((_, reject) => {
			rejectInitial = reject;
		});
		mockGetRequest.mockReturnValueOnce(first);

		const store = createRequestDetailStore(42);
		store.handleEvent({ type: 'TextDelta', id: 42, text: 'early' });

		rejectInitial(new ApiError('boom', 500, '/api/requests/42'));
		await vi.waitFor(() => expect(store.loading).toBe(false));

		expect(store.detail).toBe(null);
		expect(store.error).toBe('boom');

		store.handleEvent({ type: 'TextDelta', id: 42, text: 'late' });
		expect(store.detail).toBe(null);
	});
});

describe('detail cache', () => {
	it('two calls to createRequestDetailStore for the same id share one fetch', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail());

		const first = createRequestDetailStore(42);
		const second = createRequestDetailStore(42);

		await vi.waitFor(() => expect(first.initialized).toBe(true));

		expect(mockGetRequest).toHaveBeenCalledTimes(1);
		expect(first.detail).not.toBe(null);
		expect(second.detail).toBe(first.detail);
		expect(second.initialized).toBe(true);
	});

	it('cache hit reflects WS event merges made via the first facade', async () => {
		mockGetRequest.mockResolvedValueOnce(makeDetail({ accumulated_text: '' }));

		const first = createRequestDetailStore(42);
		await vi.waitFor(() => expect(first.initialized).toBe(true));

		first.handleEvent({ type: 'TextDelta', id: 42, text: 'streamed' });

		const second = createRequestDetailStore(42);
		expect(second.detail?.accumulated_text).toBe('streamed');
		expect(mockGetRequest).toHaveBeenCalledTimes(1);
	});

	it('distinct ids each get their own entry and their own fetch', async () => {
		mockGetRequest.mockImplementation(async (id: number) => makeDetail({ id }));

		const storeA = createRequestDetailStore(1);
		const storeB = createRequestDetailStore(2);

		await vi.waitFor(() => expect(storeA.initialized).toBe(true));
		await vi.waitFor(() => expect(storeB.initialized).toBe(true));

		expect(mockGetRequest).toHaveBeenCalledTimes(2);
		expect(storeA.detail?.id).toBe(1);
		expect(storeB.detail?.id).toBe(2);
	});

	it('evicts the least-recently-used entry when DETAIL_CACHE_CAP is reached', async () => {
		mockGetRequest.mockImplementation(async (id: number) => makeDetail({ id }));

		for (let i = 0; i < DETAIL_CACHE_CAP; i++) {
			createRequestDetailStore(1000 + i);
		}
		await vi.waitFor(() => expect(__detailCacheSizeForTests()).toBe(DETAIL_CACHE_CAP));

		createRequestDetailStore(9999);
		await vi.waitFor(() => expect(__detailCacheSizeForTests()).toBe(DETAIL_CACHE_CAP));

		const callsBefore = mockGetRequest.mock.calls.length;
		createRequestDetailStore(1000);
		await vi.waitFor(() => expect(mockGetRequest.mock.calls.length).toBe(callsBefore + 1));
	});

	it('emits a RequestDetailEviction event when LRU sheds the oldest entry', async () => {
		const evictions: RequestDetailEviction[] = [];
		const unsub = onRequestDetailEvict((e) => evictions.push(e));
		try {
			mockGetRequest.mockImplementation(async (id: number) => makeDetail({ id }));
			for (let i = 0; i < DETAIL_CACHE_CAP; i++) {
				createRequestDetailStore(2000 + i);
			}
			await vi.waitFor(() => expect(__detailCacheSizeForTests()).toBe(DETAIL_CACHE_CAP));
			expect(evictions).toHaveLength(0);

			createRequestDetailStore(9999);
			await vi.waitFor(() => expect(__detailCacheSizeForTests()).toBe(DETAIL_CACHE_CAP));
			expect(evictions).toHaveLength(1);
			expect(evictions[0]).toEqual({ id: 2000, reason: 'cache-cap' });
		} finally {
			unsub();
		}
	});
});

describe('IDB terminal gating', () => {
	it('Complete detail is promoted to IDB after refresh', async () => {
		vi.useRealTimers();
		mockGetRequest.mockResolvedValueOnce(
			makeDetail({ id: 42, status: 'Complete', is_streaming: true, accumulated_text: undefined })
		);
		mockGetAccumulatedText.mockResolvedValueOnce('');

		const store = createRequestDetailStore(42);
		await vi.waitFor(() => expect(store.initialized).toBe(true));
		await new Promise((r) => setTimeout(r, 50));
		const counts = await __idbCountsForTests();
		expect(counts.details).toBe(1);
	});

	it('Streaming / Pending detail is NOT written to IDB', async () => {
		vi.useRealTimers();
		mockGetRequest.mockResolvedValueOnce(
			makeDetail({ id: 43, status: 'Streaming', is_streaming: true })
		);

		const store = createRequestDetailStore(43);
		await vi.waitFor(() => expect(store.initialized).toBe(true));
		await new Promise((r) => setTimeout(r, 50));
		const counts = await __idbCountsForTests();
		expect(counts.details).toBe(0);
	});

	it('ResponseComplete event promotes to IDB', async () => {
		vi.useRealTimers();
		mockGetRequest.mockResolvedValueOnce(makeDetail({ id: 44, status: 'Streaming' }));
		const store = createRequestDetailStore(44);
		await vi.waitFor(() => expect(store.initialized).toBe(true));

		store.handleEvent({
			type: 'ResponseComplete',
			id: 44,
			stop_reason: 'end_turn',
			usage: null
		});
		await new Promise((r) => setTimeout(r, 50));
		const counts = await __idbCountsForTests();
		expect(counts.details).toBe(1);
	});
});

describe('graceful degradation when IDB open throws', () => {
	it('does not throw into the caller when indexedDB is broken', async () => {
		vi.useRealTimers();
		const orig = globalThis.indexedDB.open.bind(globalThis.indexedDB);
		vi.spyOn(globalThis.indexedDB, 'open').mockImplementation(() => {
			throw new Error('simulated IDB failure');
		});
		__resetIdbHandleForTests();
		try {
			mockGetRequest.mockResolvedValue(
				makeDetail({ id: 42, status: 'Complete', is_streaming: false, accumulated_text: 'text' })
			);
			const store = createRequestDetailStore(42);
			await vi.waitFor(() => expect(store.initialized).toBe(true));
			expect(store.detail?.id).toBe(42);
		} finally {
			vi.spyOn(globalThis.indexedDB, 'open').mockImplementation(orig);
			__resetIdbHandleForTests();
		}
	});
});
