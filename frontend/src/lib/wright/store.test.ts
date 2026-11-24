// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createLocalStorageMock } from '$lib/test-support/localStorage-mock';

const { storage, localStorageMock } = createLocalStorageMock();

Object.defineProperty(globalThis, 'localStorage', {
	value: localStorageMock,
	writable: true
});

import {
	causeFixture,
	installWrightFetch,
	normalScenario,
	type TriageCall,
	type WrightScenario
} from '$lib/components/__fixtures__/wright';
import { PAIR_STORAGE_KEY } from './pairing';
import { createWrightStore } from './store.svelte';

const ids = (store: ReturnType<typeof createWrightStore>): number[] =>
	store.causes.rows.map((c) => c.cause_id);

/** The scenario's causes in server order: 10, 18, 21, 22, 23 (priority 90 down to 10). */
async function loaded(over: Partial<WrightScenario> = {}) {
	const scenario = { ...normalScenario(), ...over };
	const { triage } = installWrightFetch(scenario);
	const store = createWrightStore();
	await store.loadAll();
	return { store, triage };
}

beforeEach(() => {
	storage.clear();
	storage.set(PAIR_STORAGE_KEY, 'story-token');
});

describe('operator actions on the triage queue', () => {
	it('pins a cause with the operator token and shows it first at once', async () => {
		const { store, triage } = await loaded();
		expect(ids(store)).toEqual([10, 18, 21, 22, 23]);

		expect(await store.pinCause(23, true)).toBe(true);

		expect(ids(store)).toEqual([23, 10, 18, 21, 22]);
		expect(store.causes.rows[0]!.pinned).toBe(true);
		const call = triage.find((c: TriageCall) => c.method === 'PATCH')!;
		expect(call).toMatchObject({
			path: '/api/wright/causes/23',
			body: { pinned: true },
			authorization: 'Bearer story-token'
		});
	});

	it('moves a cause to the value it is sent and puts it there at once', async () => {
		const { store, triage } = await loaded();
		// Between 21 (60) and 18 (80).
		expect(await store.moveCause(21, 70)).toBe(true);
		expect(ids(store)).toEqual([10, 18, 21, 22, 23]);
		// Past the top.
		expect(await store.moveCause(22, 91)).toBe(true);
		expect(ids(store)).toEqual([22, 10, 18, 21, 23]);
		expect(store.causes.rows[0]).toMatchObject({ operator_priority: 91, rank_priority: 91 });
		expect(triage.at(-1)!.body).toEqual({ operator_priority: 91 });

		// Reset: null clears the value and the formula orders it again.
		expect(await store.moveCause(22, null)).toBe(true);
		expect(ids(store)).toEqual([10, 18, 21, 22, 23]);
		expect(triage.at(-1)!.body).toEqual({ operator_priority: null });
	});

	it('keeps rows where they are on a passive refresh and only repaints them', async () => {
		const { store } = await loaded();
		// Another browser pins cause 23: this page still shows it last, but as pinned.
		await fetch('/api/wright/causes/23', {
			method: 'PATCH',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ pinned: true })
		});
		await store.refresh();
		expect(ids(store)).toEqual([10, 18, 21, 22, 23]);
		expect(store.causes.rows.at(-1)!.pinned).toBe(true);
	});

	it('holds a cause that is new to the page until it is asked for', async () => {
		const scenario = normalScenario();
		const extra = causeFixture({ cause_id: 99, priority: 100, check_name: 'log_errors' });
		installWrightFetch(scenario);
		const store = createWrightStore();
		await store.loadAll();
		installWrightFetch({ ...scenario, causes: [extra, ...(scenario.causes ?? [])] });
		await store.refresh();
		expect(ids(store)).toEqual([10, 18, 21, 22, 23]);
		expect(store.causes.held.map((c) => c.cause_id)).toEqual([99]);
		store.showHeldCauses();
		expect(ids(store)).toEqual([99, 10, 18, 21, 22, 23]);
	});

	it('merges causes into the one that keeps them and separates an operator merge again', async () => {
		const { store, triage } = await loaded();
		expect(await store.mergeCauses(18, [22, 23])).toBe(true);
		expect(ids(store)).toEqual([10, 18, 21]);
		const root = store.causes.rows.find((c) => c.cause_id === 18)!;
		expect(root.absorbed.map((a) => a.cause_id)).toEqual([23, 22]);
		expect(root.problems).toBe(9);
		expect(triage.find((c) => c.path.endsWith('/merge'))).toMatchObject({
			path: '/api/wright/causes/18/merge',
			body: { cause_ids: [22, 23] },
			authorization: 'Bearer story-token'
		});

		const mergeId = root.absorbed.find((a) => a.cause_id === 23)!.merge_id;
		expect(await store.separateMerge(mergeId)).toBe(true);
		expect(ids(store)).toEqual([10, 18, 21, 23]);
		expect(store.causes.rows.find((c) => c.cause_id === 18)!.absorbed).toHaveLength(1);
	});

	it('creates a group by name, puts the causes in it and shows its name on them', async () => {
		const { store, triage } = await loaded();
		expect(await store.groupCauses([21, 22], { name: 'Mail and tools' })).toBe(true);
		expect(triage.map((c) => [c.method, c.path])).toEqual([
			['POST', '/api/wright/cause-groups'],
			['PATCH', '/api/wright/causes/21'],
			['PATCH', '/api/wright/causes/22']
		]);
		const grouped = store.causes.rows.filter((c) => c.group_id !== null);
		expect(grouped.map((c) => c.group_name)).toEqual(['Mail and tools', 'Mail and tools']);
		expect(store.groups).toEqual([{ id: 50, name: 'Mail and tools', causes: 2 }]);

		// An existing group by id; a second group of the same name is refused with the reason.
		expect(await store.groupCauses([23], { id: 50 })).toBe(true);
		expect(await store.groupCauses([10], { name: 'Mail and tools' })).toBe(false);
		expect(store.triageError).toBe('A group with that name already exists.');
		expect(store.causes.rows.find((c) => c.cause_id === 10)!.group_id).toBeNull();
	});

	it('renames and deletes a group; its causes lose the group', async () => {
		const { store } = await loaded({ groups: [{ id: 3, name: 'Old', causes: 0 }] });
		await store.groupCauses([21], { id: 3 });
		expect(await store.renameGroup(3, 'New')).toBe(true);
		expect(store.causes.rows.find((c) => c.cause_id === 21)!.group_name).toBe('New');
		expect(await store.deleteGroup(3)).toBe(true);
		expect(store.groups).toEqual([]);
		expect(store.causes.rows.find((c) => c.cause_id === 21)!.group_id).toBeNull();
	});

	it('drops a reload that was slow and has been overtaken by a later action', async () => {
		const { store } = await loaded();
		const real = globalThis.fetch;
		let release: () => void = () => {};
		let held = false;
		// The first read of the queue after an action answers late, with the state it saw.
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const res = await real(input, init);
			const isRead = String(input).startsWith('/api/wright/causes?');
			if (isRead && !held) {
				held = true;
				await new Promise<void>((resolve) => (release = resolve));
			}
			return res;
		}) as typeof fetch;

		await store.pinCause(23, true);
		// The slow read of the queue started by the pin is still on its way: move cause 21 on top.
		await store.moveCause(21, 95);
		expect(ids(store)).toEqual([23, 21, 10, 18, 22]);
		await vi.waitFor(() => expect(held).toBe(true));
		release();
		await new Promise((resolve) => setTimeout(resolve, 20));
		// The late answer still has 21 where the formula puts it; it must not undo the move.
		expect(ids(store)).toEqual([23, 21, 10, 18, 22]);
	});

	it('reads a refusal from the server and leaves the queue as it was', async () => {
		const { store } = await loaded();
		// Separate on an unknown merge answers 404.
		expect(await store.separateMerge(999)).toBe(false);
		expect(store.triageError).toBe(
			'That cause is gone, or this part of Wright is not in the running mike yet.'
		);
		expect(ids(store)).toEqual([10, 18, 21, 22, 23]);
		// The next action clears the message.
		await store.pinCause(10, true);
		expect(store.triageError).toBeNull();
	});
});

describe('the open problems of a cause', () => {
	it('loads them when asked and says so when the route is not deployed', async () => {
		const { store } = await loaded();
		await store.loadCauseProblems(10);
		expect(store.causeProblems[10]!.status).toBe('ready');
		expect(store.causeProblems[10]!.rows.map((p) => p.id)).toEqual([707, 703]);

		await store.loadCauseProblems(404);
		expect(store.causeProblems[404]!.status).toBe('missing');
	});
});
