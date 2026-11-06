// @vitest-environment node

import { describe, expect, it } from 'vitest';
import { match, score } from './fuzzyMatch';

describe('score', () => {
	it('returns score 0 for an empty query', () => {
		const r = score('', 'sessions');
		expect(r).not.toBeNull();
		expect(r?.score).toBe(0);
	});

	it('returns null when haystack is empty', () => {
		expect(score('foo', '')).toBeNull();
	});

	it('rewards exact substring match over character walk', () => {
		const direct = score('session', 'sessions');
		const walk = score('ssn', 'sessions');
		expect(direct).not.toBeNull();
		expect(walk).not.toBeNull();
		expect(direct!.score).toBeGreaterThan(walk!.score);
	});

	it('rewards leading-position substring over mid-position', () => {
		const leading = score('ana', 'analysis page');
		const middle = score('ana', 'open analysis');
		expect(leading).not.toBeNull();
		expect(middle).not.toBeNull();
		expect(leading!.score).toBeGreaterThan(middle!.score);
	});

	it('returns null when haystack lacks one of the query chars', () => {
		expect(score('abc', 'a-only-here')).toBeNull();
	});

	it('rewards consecutive matches', () => {
		const consec = score('comp', 'compose');
		const split = score('cmp', 'cool man pamphlet');
		expect(consec).not.toBeNull();
		expect(split).not.toBeNull();
		expect(consec!.score).toBeGreaterThan(split!.score);
	});

	it('rewards word-boundary matches', () => {
		const onBoundary = score('cf', 'create filter');
		const noBoundary = score('cf', 'crfft');
		expect(onBoundary).not.toBeNull();
		expect(noBoundary).not.toBeNull();
		expect(onBoundary!.score).toBeGreaterThan(noBoundary!.score);
	});
});

describe('match', () => {
	const items = [
		{ id: 1, label: 'Sessions list' },
		{ id: 2, label: 'Compose a new session' },
		{ id: 3, label: 'Analysis findings' },
		{ id: 4, label: 'Pin current session' }
	];

	it('returns matches sorted highest score first', () => {
		const ranked = match('session', items, (i) => i.label);
		expect(ranked.length).toBeGreaterThan(0);
		expect(ranked[0]?.item.id).toBe(1);
		expect(ranked[ranked.length - 1]?.item).toBeDefined();
	});

	it('drops non-matching items', () => {
		const ranked = match('analysis', items, (i) => i.label);
		expect(ranked.find((r) => r.item.id === 1)).toBeUndefined();
		expect(ranked.find((r) => r.item.id === 3)).toBeDefined();
	});

	it('returns all items with score 0 when query is empty', () => {
		const ranked = match('', items, (i) => i.label);
		expect(ranked).toHaveLength(items.length);
		for (const r of ranked) {
			expect(r.score).toBe(0);
		}
	});

	it('preserves the original item reference (no copy)', () => {
		const ranked = match('analysis', items, (i) => i.label);
		const hit = ranked.find((r) => r.item.id === 3);
		expect(hit?.item).toBe(items[2]);
	});
});
