import { describe, expect, it } from 'vitest';
import { deriveSessionActivity, sessionLastSeenMs } from './sessionActivity';
import { buildSessionSummary } from './components/__fixtures__/session';
import type { ProxyEvent } from './types';

const NOW = Date.parse('2026-04-17T12:10:00Z');
const minsAgo = (m: number) => new Date(NOW - m * 60_000).toISOString();
const liveDelta: ProxyEvent = { type: 'TextDelta', id: 1, text: 'hi' };

describe('sessionLastSeenMs', () => {
	it('parses an ISO last_seen to epoch ms', () => {
		const s = buildSessionSummary({ last_seen: '2026-04-17T12:00:00Z' });
		expect(sessionLastSeenMs(s)).toBe(Date.parse('2026-04-17T12:00:00Z'));
	});

	it('returns null for an absent/unparseable last_seen', () => {
		expect(
			sessionLastSeenMs(buildSessionSummary({ last_seen: null as unknown as string }))
		).toBeNull();
		expect(sessionLastSeenMs(buildSessionSummary({ last_seen: 'not-a-date' }))).toBeNull();
	});
});

describe('deriveSessionActivity', () => {
	it('Complete + recent → not live, but active (within the window)', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Complete', last_seen: minsAgo(1) }),
			null,
			NOW
		);
		expect(a.isLive).toBe(false);
		expect(a.isActive).toBe(true);
	});

	it('Complete + old → neither live nor active', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Complete', last_seen: minsAgo(40) }),
			null,
			NOW
		);
		expect(a.isLive).toBe(false);
		expect(a.isActive).toBe(false);
	});

	it('Streaming + fresh → live AND active', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Streaming', last_seen: minsAgo(0.5) }),
			null,
			NOW
		);
		expect(a.isLive).toBe(true);
		expect(a.isActive).toBe(true);
	});

	it('Pending + fresh → live AND active', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Pending', last_seen: minsAgo(1) }),
			null,
			NOW
		);
		expect(a.isLive).toBe(true);
		expect(a.isActive).toBe(true);
	});

	it('STALE Streaming (last_seen 10 min, no WS) → NOT live but still active — the stale-spinner bug', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Streaming', last_seen: minsAgo(10) }),
			null,
			NOW
		);
		expect(a.isLive).toBe(false);
		expect(a.isActive).toBe(true);
	});

	it('very stale Streaming (last_seen 40 min) → neither live nor active — drops out entirely', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Streaming', last_seen: minsAgo(40) }),
			null,
			NOW
		);
		expect(a.isLive).toBe(false);
		expect(a.isActive).toBe(false);
	});

	it('live WS frames override staleness → live even with an old last_seen', () => {
		const a = deriveSessionActivity(
			buildSessionSummary({ last_status: 'Streaming', last_seen: minsAgo(30) }),
			liveDelta,
			NOW
		);
		expect(a.isLive).toBe(true);
		expect(a.isActive).toBe(true);
	});
});
