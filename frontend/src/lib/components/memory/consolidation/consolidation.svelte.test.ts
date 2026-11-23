import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	CHANGES,
	CONFLICTS,
	stubConsolidationFetch
} from '$lib/test-support/__fixtures__/knowledgeConsolidation';
import { ConsolidationStore } from './consolidation.svelte';

let store: ConsolidationStore;
const realFetch = globalThis.fetch;

beforeEach(() => {
	vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
	store = new ConsolidationStore();
});

afterEach(() => {
	store.dispose();
	vi.useRealTimers();
	globalThis.fetch = realFetch;
});

describe('ConsolidationStore', () => {
	it('loads the capability, measures, conflicts, changes and subjects for the project', async () => {
		stubConsolidationFetch('steady');
		await store.start('3');
		expect(store.capability?.state).toBe('active');
		expect(store.measures).toHaveLength(30);
		expect(store.conflicts.map((c) => c.id)).toEqual([910, 911]);
		expect(store.changes).toHaveLength(CHANGES.filter((c) => c.verdict !== 'kept').length);
		expect(store.subjects).toHaveLength(5);
		expect(store.changesNext).toBeNull();
	});

	it('updates held rows in place on a poll and only counts rows it does not hold', async () => {
		const fx = stubConsolidationFetch('steady');
		await store.start('3');
		const order = store.changes.map((c) => c.id);

		fx.changes[0]!.application = 'undone';
		fx.changes = [
			{ ...fx.changes[0]!, id: 999, created_at: '2026-11-20T17:30:00Z' },
			...fx.changes
		];
		await store.poll();

		expect(store.changes.map((c) => c.id)).toEqual(order);
		expect(store.changes.find((c) => c.id === 960)?.application).toBe('undone');
		expect(store.newChanges).toBe(1);

		await store.showNew('changes');
		expect(store.changes[0]!.id).toBe(999);
		expect(store.newChanges).toBe(0);
	});

	it('keeps a conflict that a poll no longer returns and marks it settled', async () => {
		const fx = stubConsolidationFetch('steady');
		await store.start('3');
		fx.conflicts = fx.conflicts.filter((c) => c.id !== CONFLICTS[0]!.id);
		await store.poll();
		expect(store.conflicts.map((c) => c.id)).toEqual([910, 911]);
		expect(store.settled[910]).toBeNull();
		expect(910 in store.settled && 911 in store.settled).toBe(false);
	});

	it('undoes a change in place without moving it', async () => {
		stubConsolidationFetch('steady');
		await store.start('3');
		const order = store.changes.map((c) => c.id);
		await store.undo(store.changes[0]!);
		expect(store.changes.map((c) => c.id)).toEqual(order);
		expect(store.changes[0]!.application).toBe('undone');
		expect(store.inFlight).toBeNull();
	});

	it('records the kept carrier when a conflict is settled', async () => {
		stubConsolidationFetch('steady');
		await store.start('3');
		await store.keep(store.conflicts[0]!, 8101);
		expect(store.settled[910]).toBe('Lesson 2733');
	});

	it('shows the server message under the row when an action is refused', async () => {
		stubConsolidationFetch('steady');
		await store.start('3');
		const row = store.changes[0]!;
		const original = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			if (init?.method === 'POST') {
				return new Response(JSON.stringify({ error: 'the file changed since' }), { status: 409 });
			}
			return original(input, init);
		}) as typeof fetch;
		await store.undo(row);
		expect(store.rowErrors[row.id]).toBe('Not undone. the file changed since');
		await store.apply(row);
		expect(store.rowErrors[row.id]).toBe('Not written. the file changed since');
	});

	it('stops polling bands while the worker is off but keeps the stored rows', async () => {
		stubConsolidationFetch('off-history');
		await store.start('3');
		const calls = vi.fn();
		const fetchBefore = globalThis.fetch;
		globalThis.fetch = ((...args: Parameters<typeof fetch>) => {
			calls(String(args[0]));
			return fetchBefore(...args);
		}) as typeof fetch;
		await store.poll();
		expect(calls.mock.calls.map(([url]) => url)).toEqual(['/api/capabilities']);
		expect(store.measures.length).toBeGreaterThan(0);
	});

	it('notes a failed refresh without dropping rows', async () => {
		stubConsolidationFetch('steady');
		await store.start('3');
		globalThis.fetch = (async () => new Response('{}', { status: 500 })) as typeof fetch;
		await store.poll();
		expect(store.refresh.changes.failedAt).not.toBeNull();
		expect(store.changes.length).toBeGreaterThan(0);
	});
});
