import { describe, expect, it } from 'vitest';
import { MAX_EVENTS, ProxyEventStream } from './ws.svelte';
import type { ProxyEvent } from './types';

function makeEvent(idx: number): ProxyEvent {
	return {
		type: 'TextDelta',
		id: idx,
		text: `event-${idx}`
	} as unknown as ProxyEvent;
}

interface StreamWithTestHook extends ProxyEventStream {
	__pushForTests(event: ProxyEvent): void;
}

function pushAll(stream: StreamWithTestHook, count: number): void {
	for (let i = 0; i < count; i++) {
		stream.__pushForTests(makeEvent(i));
	}
}

describe('W1.20-B ProxyEventStream ring buffer', () => {
	it('cap value matches MAX_EVENTS exposed constant', () => {
		expect(MAX_EVENTS).toBe(1000);
	});

	it('totalEvents is monotonic; at(i) reads back what was pushed', () => {
		const stream = new ProxyEventStream() as StreamWithTestHook;
		expect(stream.totalEvents).toBe(0);
		pushAll(stream, 5);
		expect(stream.totalEvents).toBe(5);
		for (let i = 0; i < 5; i++) {
			expect(stream.at(i)).toEqual(makeEvent(i));
		}
		expect(stream.at(-1)).toBeUndefined();
		expect(stream.at(5)).toBeUndefined();
	});

	it('drops the oldest event once MAX_EVENTS is exceeded', () => {
		const stream = new ProxyEventStream() as StreamWithTestHook;
		pushAll(stream, MAX_EVENTS + 1);
		expect(stream.totalEvents).toBe(MAX_EVENTS + 1);
		expect(stream.at(0)).toBeUndefined();
		expect(stream.at(1)).toEqual(makeEvent(1));
		expect(stream.at(MAX_EVENTS)).toEqual(makeEvent(MAX_EVENTS));
	});

	it('after pushing 2*MAX_EVENTS, only the most recent MAX_EVENTS are resident', () => {
		const stream = new ProxyEventStream() as StreamWithTestHook;
		pushAll(stream, 2 * MAX_EVENTS);
		expect(stream.totalEvents).toBe(2 * MAX_EVENTS);
		for (let i = 0; i < MAX_EVENTS; i++) {
			expect(stream.at(i)).toBeUndefined();
		}
		for (let i = MAX_EVENTS; i < 2 * MAX_EVENTS; i++) {
			expect(stream.at(i)).toEqual(makeEvent(i));
		}
	});

	it('clear() resets totalEvents and blanks the ring', () => {
		const stream = new ProxyEventStream() as StreamWithTestHook;
		pushAll(stream, 100);
		expect(stream.totalEvents).toBe(100);
		stream.clear();
		expect(stream.totalEvents).toBe(0);
		for (let i = 0; i < 100; i++) {
			expect(stream.at(i)).toBeUndefined();
		}
		stream.__pushForTests(makeEvent(999));
		expect(stream.totalEvents).toBe(1);
		expect(stream.at(0)).toEqual(makeEvent(999));
	});

	it('Lagged events bump laggedCount but still occupy a ring slot', () => {
		const stream = new ProxyEventStream() as StreamWithTestHook;
		const lagged: ProxyEvent = { type: 'Lagged', skipped: 7 } as unknown as ProxyEvent;
		stream.__pushForTests(lagged);
		expect(stream.laggedCount).toBe(7);
		expect(stream.totalEvents).toBe(1);
		expect(stream.at(0)).toEqual(lagged);
	});
});
