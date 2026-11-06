import { afterEach, describe, expect, it } from 'vitest';
import { ProxyEventStream } from '$lib/ws.svelte';
import type { ConnectionState, ProxyEvent } from '$lib/types';
import type { SessionFlowStore, FlowLiveMark } from './sessionFlowStore.svelte';
import { createSessionFlowLiveAdapter } from './liveAdapter';
import { FLOW_SESSION_ID as SESSION, emptyFlowDataset } from './__fixtures__/sessionFlow';

const OTHER_SESSION = '00000000-0000-4000-8000-000000000099';

function mockFlowStore(activeSessionId: string | null): SessionFlowStore & {
	invalidations: Array<{ reason: string; components?: readonly string[] }>;
	liveMarks: FlowLiveMark[];
	clears: string[];
} {
	const invalidations: Array<{ reason: string; components?: readonly string[] }> = [];
	const liveMarks: FlowLiveMark[] = [];
	const clears: string[] = [];
	return {
		dataset: emptyFlowDataset(),
		coverage: new Map(),
		availability: null,
		activeSessionId,
		loading: false,
		error: null,
		stale: false,
		staleReasons: [],
		overlay: [],
		overlayHistoryUnavailable: false,
		invalidations,
		liveMarks,
		clears,
		load: async () => {},
		loadEarlier: async () => 'inactive',
		invalidate(reason, components) {
			invalidations.push({ reason, components });
		},
		retry: async () => {},
		observeLive(mark) {
			liveMarks.push(mark);
		},
		clearLiveHistory(reason) {
			clears.push(reason);
		},
		deactivate() {},
		dispose() {}
	};
}

function drain(
	stream: ProxyEventStream,
	cursor: { processed: number },
	handle: (event: ProxyEvent) => void
): void {
	const total = stream.totalEvents;
	if (total === cursor.processed) return;
	const start = total < cursor.processed ? 0 : Math.max(cursor.processed, total - 1000);
	for (let i = start; i < total; i++) {
		const event = stream.at(i);
		if (event) handle(event);
	}
	cursor.processed = total;
}

function push(stream: ProxyEventStream, event: ProxyEvent): void {
	(stream as unknown as { __pushForTests(event: ProxyEvent): void }).__pushForTests(event);
}

describe('createSessionFlowLiveAdapter', () => {
	afterEach(() => {});

	it('routes a session-scoped requests row change into a full coalesced invalidation', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'requests',
			id: '1',
			session_id: SESSION,
			op: 'INSERT'
		});
		expect(store.invalidations).toHaveLength(1);
		expect(store.invalidations[0]?.reason).toMatch(/request/i);
		expect(store.invalidations[0]?.components).toBeUndefined();
	});

	it('ignores a requests row change scoped to a different session', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'requests',
			id: '1',
			session_id: OTHER_SESSION,
			op: 'INSERT'
		});
		expect(store.invalidations).toHaveLength(0);
	});

	it('scopes a session-owned table change to only its own Flow component', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'llm_calls',
			id: '1',
			session_id: SESSION,
			op: 'INSERT'
		});
		expect(store.invalidations).toEqual([
			{ reason: 'llm_calls changed', components: ['llm_calls'] }
		]);
	});

	it('routes an outgoing mailbox send via the unconditional mailbox_threads signal, not the recipient-scoped mailbox_messages one', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'mailbox_messages',
			id: 'm1',
			session_id: OTHER_SESSION,
			op: 'INSERT'
		});
		expect(store.invalidations).toHaveLength(0);
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'mailbox_threads',
			id: 't1',
			session_id: null,
			op: 'INSERT'
		});
		expect(store.invalidations).toEqual([
			{ reason: 'mailbox_threads changed', components: ['messages'] }
		]);
	});

	it('routes global consolidation_runs changes unconditionally', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'consolidation_runs',
			id: 'r1',
			session_id: null,
			op: 'UPDATE'
		});
		expect(store.invalidations).toEqual([
			{ reason: 'consolidation_runs changed', components: ['consolidation'] }
		]);
	});

	it('invalidates on a sessions row change only when it names the active session', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'sessions',
			id: OTHER_SESSION,
			session_id: null,
			op: 'UPDATE'
		});
		expect(store.invalidations).toHaveLength(0);
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'sessions',
			id: SESSION,
			session_id: null,
			op: 'UPDATE'
		});
		expect(store.invalidations).toEqual([
			{ reason: 'Session record changed', components: undefined }
		]);
	});

	it('ignores every row change while no session is active', () => {
		const store = mockFlowStore(null);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => SESSION
		});
		adapter.handleEvent({
			type: 'RowChanged',
			table: 'requests',
			id: '1',
			session_id: SESSION,
			op: 'INSERT'
		});
		expect(store.invalidations).toHaveLength(0);
	});

	it('maps ResyncRequired reasons, including P06 listener recovery, into a coalesced invalidation', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({ type: 'ResyncRequired', reason: 'recovery' });
		expect(store.invalidations).toHaveLength(1);
		expect(store.invalidations[0]?.reason).toMatch(/recovered/i);
	});

	it('forwards StreamingStarted/DiagnosticReady only when the request resolves to the active session', () => {
		const store = mockFlowStore(SESSION);
		const owners = new Map<number, string>([
			[1, SESSION],
			[2, OTHER_SESSION]
		]);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: (id) => owners.get(id) ?? null
		});
		adapter.handleEvent({ type: 'StreamingStarted', id: 1, status: 200, ttfb_ms: 12, model: null });
		adapter.handleEvent({ type: 'StreamingStarted', id: 2, status: 200, ttfb_ms: 12, model: null });
		adapter.handleEvent({ type: 'DiagnosticReady', id: 1, count: 3 });
		expect(store.liveMarks.map((m) => [m.requestId, m.family])).toEqual([
			['1', 'StreamingStarted'],
			['1', 'DiagnosticReady']
		]);
	});

	it('never forwards transcript deltas (TextDelta et al.) as Flow evidence', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => SESSION
		});
		adapter.handleEvent({ type: 'TextDelta', id: 1, text: 'hello' });
		adapter.handleEvent({ type: 'ThinkingDelta', id: 1, thinking: 'hmm' });
		expect(store.invalidations).toHaveLength(0);
		expect(store.liveMarks).toHaveLength(0);
	});

	it('handles Lagged idempotently — the ring drain and the separate lagged-counter effect can both signal the same gap', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.handleEvent({ type: 'Lagged', skipped: 4 });
		adapter.handleEvent({ type: 'Lagged', skipped: 4 });
		expect(store.clears).toEqual([
			'WebSocket ring buffer dropped frames',
			'WebSocket ring buffer dropped frames'
		]);
		expect(store.invalidations).toHaveLength(2);
	});

	it('drains >1,000 buffered notifications through the real ring, clamped to its window, and still resolves the tail correctly', () => {
		const stream = new ProxyEventStream();
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		for (let i = 0; i < 1500; i++)
			push(stream, {
				type: 'RowChanged',
				table: 'llm_calls',
				id: String(i),
				session_id: SESSION,
				op: 'INSERT'
			});
		const cursor = { processed: 0 };
		drain(stream, cursor, (e) => adapter.handleEvent(e));
		expect(store.invalidations).toHaveLength(1000);
		expect(cursor.processed).toBe(1500);
	});

	it('reports transport transitions without any accompanying row-change notification, ignoring the first-ever observation', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		const states: ConnectionState[] = ['connecting', 'connected', 'reconnecting', 'connected'];
		for (const s of states) adapter.observeConnectionState(s);
		expect(store.invalidations).toHaveLength(1);
		expect(store.invalidations[0]?.reason).toMatch(/reconnected after reconnecting/i);
	});

	it('stops reacting once disposed', () => {
		const store = mockFlowStore(SESSION);
		const adapter = createSessionFlowLiveAdapter({
			flowStore: store,
			resolveRequestSessionId: () => null
		});
		adapter.dispose();
		adapter.handleEvent({ type: 'ResyncRequired', reason: 'listener_establishment' });
		adapter.observeConnectionState('connected');
		expect(store.invalidations).toHaveLength(0);
	});
});
