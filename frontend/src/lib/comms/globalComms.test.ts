import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return { ApiError };
});

vi.mock('./api', () => ({
	getTags: vi.fn(),
	getThread: vi.fn(),
	isCommsUnavailable: vi.fn(() => false),
	listThreads: vi.fn(),
	resolveThread: vi.fn()
}));

import { getTags, getThread, listThreads } from './api';
import { createGlobalCommsStore } from './globalComms.svelte';
import type { ProxyEvent, RowChangedTable } from '$lib/types';

const mockListThreads = vi.mocked(listThreads);
const mockGetThread = vi.mocked(getThread);
const mockGetTags = vi.mocked(getTags);

function rowChanged(table: RowChangedTable): ProxyEvent {
	return { type: 'RowChanged', table, id: '1', session_id: null, op: 'INSERT' };
}

const settle = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
	mockListThreads.mockReset();
	mockListThreads.mockResolvedValue({ threads: [] } as never);
	mockGetThread.mockReset();
	mockGetTags.mockReset();
	mockGetTags.mockResolvedValue({ tags: [] } as never);
});

describe('thread list reloads on mailbox changes', () => {
	it('asks for nothing while no view shows the list', async () => {
		const store = createGlobalCommsStore();
		store.handleEvent(rowChanged('mailbox_messages'));
		store.handleEvent(rowChanged('mailbox_threads'));
		store.handleEvent({ type: 'Lagged' } as ProxyEvent);
		await settle();
		expect(mockListThreads).not.toHaveBeenCalled();
		expect(mockGetThread).not.toHaveBeenCalled();
	});

	it('reloads while a view shows the list, and stops when that view leaves', async () => {
		const store = createGlobalCommsStore();
		const release = store.retain();
		store.handleEvent(rowChanged('mailbox_messages'));
		await settle();
		expect(mockListThreads).toHaveBeenCalledTimes(1);

		release();
		store.handleEvent(rowChanged('mailbox_messages'));
		await settle();
		expect(mockListThreads).toHaveBeenCalledTimes(1);
	});

	it('keeps reloading for a second view after the first one leaves', async () => {
		const store = createGlobalCommsStore();
		const first = store.retain();
		const second = store.retain();
		first();
		first();
		store.handleEvent(rowChanged('mailbox_threads'));
		await settle();
		expect(mockListThreads).toHaveBeenCalledTimes(1);

		second();
		store.handleEvent(rowChanged('mailbox_threads'));
		await settle();
		expect(mockListThreads).toHaveBeenCalledTimes(1);
	});

	it('ignores changes to other tables even while a view shows the list', async () => {
		const store = createGlobalCommsStore();
		store.retain();
		store.handleEvent(rowChanged('requests'));
		await settle();
		expect(mockListThreads).not.toHaveBeenCalled();
	});

	it('passes every event to its subscribers whether or not a view shows the list', () => {
		const store = createGlobalCommsStore();
		const seen: ProxyEvent[] = [];
		store.subscribe((event) => seen.push(event));
		const event = rowChanged('mailbox_messages');
		store.handleEvent(event);
		expect(seen).toEqual([event]);
	});
});
