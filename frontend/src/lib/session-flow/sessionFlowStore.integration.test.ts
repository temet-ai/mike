import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchSessionFlowPage, type FetchSessionFlowPage } from '$lib/api/sessionFlow';
import type { SessionFlowPage } from '$lib/api/generated/SessionFlowPage';
import {
	FLOW_SESSION_ID as session,
	FLOW_START_MS,
	flowActivity,
	flowActor,
	flowCoverage,
	flowTime
} from './__fixtures__/sessionFlow';
import { createSessionFlowStore, type FlowLiveMark } from './sessionFlowStore.svelte';
function deferred<T>() {
	let resolve!: (value: T) => void, reject!: (reason: Error) => void;
	const promise = new Promise<T>((yes, no) => {
		resolve = yes;
		reject = no;
	});
	return { promise, resolve, reject };
}
function page(ids: string[] = ['a'], overrides: Partial<SessionFlowPage> = {}): SessionFlowPage {
	return {
		version: 1,
		session_id: session,
		availability: {
			eligible: { capture_version: 1, admitted_at: flowTime(0), producer: 'claude_code' }
		},
		actors: [flowActor()],
		activities: ids.map((id) => flowActivity(id)),
		relationships: [],
		coverage: flowCoverage(),
		next_cursor: null,
		...overrides
	};
}
const ids = (store: ReturnType<typeof createSessionFlowStore>) => [
	...store.dataset.activities.keys()
];
const continued = (ids: string[] = ['a']) =>
	page(ids, { next_cursor: 'next', coverage: flowCoverage({ exhausted: false, lower_id: '1' }) });
afterEach(() => vi.unstubAllGlobals());

describe('session-flow uncached client', () => {
	it('encodes session and cursor, forwards abort signal and preserves bigint strings', async () => {
		const fetch = vi
			.fn()
			.mockResolvedValue(new Response(JSON.stringify(page(undefined, { session_id: 'a/b' }))));
		vi.stubGlobal('fetch', fetch);
		const controller = new AbortController();
		const result = await fetchSessionFlowPage({
			sessionId: 'a/b',
			component: 'requests',
			cursor: '{"after_id":"9007199254740993"}',
			signal: controller.signal
		});
		const call = fetch.mock.calls[0];
		expect(call?.[0]).toContain('/api/sessions/a%2Fb/flow?');
		expect(new URL(call?.[0] as string, 'http://test').searchParams.get('cursor')).toBe(
			'{"after_id":"9007199254740993"}'
		);
		expect(call?.[1]).toMatchObject({ signal: controller.signal });
		expect(result.activities[0]?.source.row_id).toBe('9007199254740993');
	});
	it('preserves missing and retryable service errors without eligible-empty fallback', async () => {
		for (const status of [404, 503]) {
			vi.stubGlobal(
				'fetch',
				vi.fn().mockResolvedValue(new Response('source unavailable', { status }))
			);
			await expect(
				fetchSessionFlowPage({ sessionId: session, component: 'requests' })
			).rejects.toMatchObject({ status, message: 'source unavailable' });
		}
	});
	it('rejects wrong scope/version and invalid page limits', async () => {
		for (const change of [
			{ version: 2 },
			{ session_id: 'wrong' },
			{ coverage: flowCoverage({ component: 'tools' }) }
		]) {
			vi.stubGlobal(
				'fetch',
				vi.fn().mockResolvedValue(new Response(JSON.stringify(page([], change))))
			);
			await expect(
				fetchSessionFlowPage({ sessionId: session, component: 'requests' })
			).rejects.toMatchObject({ status: 502 });
		}
		await expect(
			fetchSessionFlowPage({ sessionId: session, component: 'requests', limit: 501 })
		).rejects.toThrow(RangeError);
	});
});

describe('session-flow convergent store', () => {
	it('traverses overlapping pages, preserves first lower bound and removes deleted records', async () => {
		let refresh = false;
		const fetchPage = vi.fn<FetchSessionFlowPage>(async ({ cursor }) =>
			refresh
				? page(['b'])
				: cursor
					? page(['a', 'b'], { coverage: flowCoverage({ lower_id: '2' }) })
					: continued()
		);
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		await store.load(session);
		expect(ids(store)).toEqual(['a', 'b']);
		expect(store.coverage.get('requests')?.lower_id).toBe('1');
		expect(fetchPage.mock.calls[1]?.[0].cursor).toBe('next');
		refresh = true;
		store.invalidate('delete');
		await vi.waitFor(() => expect(ids(store)).toEqual(['b']));
		expect(store.stale).toBe(false);
		store.dispose();
	});
	it('coalesces over 1000 invalidations during initial fetch without losing terminal closure', async () => {
		const first = deferred<SessionFlowPage>();
		const closed = page();
		closed.activities[0]!.ended_at = flowTime(2000);
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockReturnValueOnce(first.promise)
			.mockResolvedValue(closed);
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		const loading = store.load(session);
		for (let i = 0; i < 1200; i++) store.invalidate('row changed');
		first.resolve(page());
		await loading;
		expect(fetchPage).toHaveBeenCalledTimes(2);
		expect(store.dataset.activities.get('a')?.endedAtMs).toBe(FLOW_START_MS + 2000);
		expect(store.stale).toBe(false);
		store.dispose();
	});
	it('restarts the full traversal when writes arrive during a continuation', async () => {
		const second = deferred<SessionFlowPage>();
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockResolvedValueOnce(continued())
			.mockReturnValueOnce(second.promise)
			.mockResolvedValue(page(['fresh']));
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		const loading = store.load(session);
		await vi.waitFor(() => expect(fetchPage).toHaveBeenCalledTimes(2));
		store.invalidate('late write');
		second.resolve(page(['b']));
		await loading;
		expect(fetchPage.mock.calls[2]?.[0].cursor).toBeUndefined();
		expect(ids(store)).toEqual(['fresh']);
		store.dispose();
	});
	it('automatically drains final dirtiness beyond the per-batch follow-up budget', async () => {
		const first = deferred<SessionFlowPage>(),
			second = deferred<SessionFlowPage>();
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise)
			.mockResolvedValue(page(['settled']));
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		const loading = store.load(session);
		store.invalidate('first change');
		first.resolve(page());
		await vi.waitFor(() => expect(fetchPage).toHaveBeenCalledTimes(2));
		store.invalidate('second change');
		second.resolve(page());
		await loading;
		await vi.waitFor(() => expect(ids(store)).toEqual(['settled']));
		expect(fetchPage).toHaveBeenCalledTimes(3);
		expect(store.stale).toBe(false);
		store.dispose();
	});
	it('deactivation cancels a queued dirty-batch continuation', async () => {
		const queued: Array<() => void> = [];
		vi.stubGlobal('queueMicrotask', (callback: () => void) => {
			queued.push(callback);
		});
		const first = deferred<SessionFlowPage>(),
			second = deferred<SessionFlowPage>();
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise)
			.mockResolvedValue(page(['late']));
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		const loading = store.load(session);
		store.invalidate('first change');
		first.resolve(page());
		await vi.waitFor(() => expect(fetchPage).toHaveBeenCalledTimes(2));
		store.invalidate('second change');
		second.resolve(page());
		await loading;
		expect(queued).toHaveLength(1);
		store.deactivate();
		for (const callback of queued) callback();
		await Promise.resolve();
		expect(fetchPage).toHaveBeenCalledTimes(2);
		expect(store.activeSessionId).toBeNull();
		expect(ids(store)).toEqual([]);
		store.dispose();
	});
	it('ignores obsolete resolves, errors and finally across A to B to A', async () => {
		const first = deferred<SessionFlowPage>(),
			second = deferred<SessionFlowPage>(),
			third = deferred<SessionFlowPage>();
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise)
			.mockReturnValueOnce(third.promise);
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		const a = store.load(session),
			b = store.load('b'),
			again = store.load(session);
		expect(fetchPage.mock.calls[0]?.[0].signal?.aborted).toBe(true);
		first.resolve(page(['obsolete']));
		second.reject(new Error('obsolete failure'));
		await Promise.all([a, b]);
		expect(store.loading).toBe(true);
		expect(store.error).toBeNull();
		expect(ids(store)).toEqual([]);
		third.resolve(page(['current']));
		await again;
		expect(ids(store)).toEqual(['current']);
		store.dispose();
	});
	it('retains authoritative data on failed partial refresh and retries cleanly', async () => {
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockResolvedValueOnce(page(['old']))
			.mockResolvedValueOnce(continued(['new']))
			.mockRejectedValueOnce(new Error('offline'))
			.mockResolvedValue(page(['new']));
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		await store.load(session);
		await store.retry();
		expect(ids(store)).toEqual(['old']);
		expect(store.error?.message).toBe('offline');
		expect(store.stale).toBe(true);
		await store.retry();
		expect(ids(store)).toEqual(['new']);
		expect(store.error).toBeNull();
		store.dispose();
	});
	it('rejects repeated cursors and wrong scope without complete coverage', async () => {
		const fetchPage = vi.fn<FetchSessionFlowPage>().mockResolvedValue(continued());
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		await store.load(session);
		expect(fetchPage).toHaveBeenCalledTimes(2);
		expect(store.error?.message).toContain('cursor');
		expect(ids(store)).toEqual([]);
		fetchPage.mockResolvedValue(page([], { session_id: 'wrong' }));
		await store.retry();
		expect(store.error?.message).toContain('scope');
		store.dispose();
	});
	it('caps retained evidence, exposes unloaded coverage and avoids refetching a capped prefix', async () => {
		const fetchPage = vi.fn<FetchSessionFlowPage>(async ({ cursor }) =>
			cursor ? page(['b']) : continued()
		);
		const store = createSessionFlowStore({
			fetchPage,
			components: ['requests'],
			limits: { activities: 1 }
		});
		await store.load(session);
		expect(ids(store)).toEqual(['a']);
		expect(store.coverage.get('requests')?.exhausted).toBe(false);
		expect(store.staleReasons.join(' ')).toContain('unloaded');
		expect(await store.loadEarlier('requests')).toBe('bounded');
		expect(fetchPage).toHaveBeenCalledTimes(2);
		store.dispose();
	});
	it('keeps component replacement isolated and exhausted loadEarlier makes no request', async () => {
		let version = 0;
		const fetchPage = vi.fn<FetchSessionFlowPage>(async ({ component }) =>
			page([`${component}-${version}`], { coverage: flowCoverage({ component }) })
		);
		const store = createSessionFlowStore({ fetchPage, components: ['requests', 'tools'] });
		await store.load(session);
		expect(await store.loadEarlier('requests')).toBe('exhausted');
		expect(fetchPage).toHaveBeenCalledTimes(2);
		version = 1;
		store.invalidate('tools changed', ['tools']);
		await vi.waitFor(() => expect(ids(store)).toEqual(['requests-0', 'tools-1']));
		store.dispose();
	});
	it('rejects older source-supported revisions and replaces revisionless evidence', async () => {
		const newer = page(),
			older = page();
		newer.activities[0]!.source_revision = '10';
		older.activities[0]!.source_revision = '9';
		const fetchPage = vi
			.fn<FetchSessionFlowPage>()
			.mockResolvedValueOnce(newer)
			.mockResolvedValueOnce(older)
			.mockResolvedValue(page(['replacement']));
		const store = createSessionFlowStore({
			fetchPage,
			components: ['requests'],
			compareRevision: (_table, incoming, previous) => Number(BigInt(incoming) - BigInt(previous))
		});
		await store.load(session);
		await store.retry();
		expect(store.dataset.activities.get('a')?.source_revision).toBe('10');
		await store.retry();
		expect(ids(store)).toEqual(['replacement']);
		store.dispose();
	});
	it('distinguishes unavailable, unsupported and eligible empty sessions', async () => {
		for (const availability of [
			'no_admitted_start',
			{ unsupported_version: { capture_version: 2 } },
			{ eligible: { capture_version: 1, admitted_at: flowTime(0), producer: 'claude_code' } }
		] as const) {
			const store = createSessionFlowStore({
				fetchPage: async () => page([], { availability }),
				components: ['requests']
			});
			await store.load(session);
			expect(store.availability).toEqual(availability);
			expect(store.error).toBeNull();
			expect(ids(store)).toEqual([]);
			store.dispose();
		}
	});
	it('deactivates and reloads while terminal disposal prevents restart', async () => {
		const fetchPage = vi.fn<FetchSessionFlowPage>().mockResolvedValue(page());
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		await store.load(session);
		store.deactivate();
		expect(store.activeSessionId).toBeNull();
		expect(ids(store)).toEqual([]);
		await store.load(session);
		expect(ids(store)).toEqual(['a']);
		store.dispose();
		await store.load(session);
		store.invalidate('late');
		expect(fetchPage).toHaveBeenCalledTimes(2);
	});
	it('bounds/deduplicates receipt marks separately and runtime-validates the allowlist', async () => {
		const store = createSessionFlowStore({
			fetchPage: async () => page([]),
			components: ['requests'],
			limits: { overlay: 2 }
		});
		await store.load(session);
		store.observeLive({
			sessionId: 'other',
			requestId: 'wrong',
			family: 'StreamingStarted',
			receivedAtMs: 1
		});
		store.observeLive({
			sessionId: session,
			requestId: 'bad',
			family: 'TextDelta',
			receivedAtMs: 1
		} as unknown as FlowLiveMark);
		for (const requestId of ['a', 'a', 'b', 'c'])
			store.observeLive({
				sessionId: session,
				requestId,
				family: 'DiagnosticReady',
				receivedAtMs: 2,
				count: 0
			});
		expect(store.overlay.map((mark) => mark.requestId)).toEqual(['b', 'c']);
		expect(store.overlay[0]?.count).toBe(0);
		expect(store.overlayHistoryUnavailable).toBe(true);
		expect(ids(store)).toEqual([]);
		store.clearLiveHistory('reconnect');
		expect(store.overlay).toEqual([]);
		expect(store.overlayHistoryUnavailable).toBe(true);
		store.dispose();
	});
	it('refreshes old loaded captures when closure lies outside the latest page', async () => {
		let close = false;
		const fetchPage: FetchSessionFlowPage = async ({ cursor }) => {
			if (cursor) return page(['latest']);
			const initial = continued(['old']);
			if (close) initial.activities[0]!.ended_at = flowTime(4000);
			return initial;
		};
		const store = createSessionFlowStore({ fetchPage, components: ['requests'] });
		await store.load(session);
		close = true;
		await store.retry();
		expect(store.dataset.activities.get('old')?.endedAtMs).toBe(FLOW_START_MS + 4000);
		expect(ids(store)).toEqual(['old', 'latest']);
		store.dispose();
	});
});
