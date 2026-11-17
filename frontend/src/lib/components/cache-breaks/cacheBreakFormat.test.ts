import { describe, expect, it } from 'vitest';
import { CACHE_BREAK_GROUPS, buildCacheBreak } from '$lib/test-support/cacheBreakFixtures';
import {
	causeLabel,
	detailNotes,
	formatGap,
	groupLabel,
	locationLabel,
	orderGroups,
	plural,
	totals
} from './cacheBreakFormat';

describe('causeLabel', () => {
	it('names the mike stage first', () => {
		expect(causeLabel(buildCacheBreak())).toBe('self_identity');
	});

	it('names the client, unknown and expiry causes', () => {
		expect(causeLabel(buildCacheBreak({ cause: 'client', stage: null }))).toBe('Claude Code');
		expect(causeLabel(buildCacheBreak({ cause: 'unknown', stage: null }))).toBe('unknown');
		expect(causeLabel(buildCacheBreak({ cause: 'expired', woken_by: 'mail' }))).toBe(
			'expired, woken by mail'
		);
	});
});

describe('groups', () => {
	it('puts mike first, then client, unknown and expiries, keeping the server order inside each', () => {
		expect(orderGroups(CACHE_BREAK_GROUPS).map((g) => g.cause)).toEqual([
			'mike',
			'client',
			'unknown',
			'expired'
		]);
	});

	it('adds the location for client and unknown groups only', () => {
		const [expired, mike, client] = CACHE_BREAK_GROUPS;
		expect(groupLabel(mike!)).toBe('self_identity');
		expect(groupLabel(client!)).toBe('Claude Code · tools');
		expect(groupLabel(expired!)).toBe('expired, woken by operator');
	});

	it('totals breaks and tokens', () => {
		expect(totals(CACHE_BREAK_GROUPS)).toEqual({ breaks: 6, tokens: 1_835_000 });
	});
});

describe('formatting', () => {
	it('labels locations and falls back to the raw value', () => {
		expect(locationLabel('messages_prefix')).toBe('earlier messages');
		expect(locationLabel(null)).toBe('—');
		expect(locationLabel('future_place')).toBe('future_place');
	});

	it('formats gaps and counts', () => {
		expect(formatGap(42)).toBe('42.0s');
		expect(formatGap(5_400)).toBe('1h 30m');
		expect(plural(1, 'break')).toBe('1 break');
		expect(plural(1200, 'break')).toBe('1,200 breaks');
	});

	it('lists the facts a break carries', () => {
		expect(
			detailNotes({
				tools_added: ['A'],
				tools_removed: ['B', 'C'],
				filter_from: 'x',
				filter_to: 'y',
				applied_stages: ['self_identity'],
				message: 7
			})
		).toEqual([
			'tools added: A',
			'tools removed: B, C',
			'filter x → y',
			'stages applied: self_identity',
			'message 7'
		]);
		expect(detailNotes({})).toEqual([]);
	});
});
