import { describe, expect, it } from 'vitest';
import {
	CONFLICTS,
	CHANGES,
	capabilityFor,
	fixtureFor,
	measures
} from '$lib/test-support/__fixtures__/knowledgeConsolidation';
import {
	MEASURES,
	actionFor,
	conflictedStatementIds,
	formatWhen,
	sectionState,
	settlesLine,
	showsBands
} from './labels';

describe('sectionState', () => {
	it('maps the capability row and the measures to one state', () => {
		const some = measures(3);
		expect(sectionState(null, some)).toBe('unknown');
		expect(sectionState(capabilityFor('off-never'), [])).toBe('off-never');
		expect(sectionState(capabilityFor('off-history'), some)).toBe('off-history');
		expect(sectionState(capabilityFor('first-pass'), [])).toBe('first-pass');
		expect(sectionState(capabilityFor('steady'), some)).toBe('steady');
		expect(sectionState(capabilityFor('degraded'), some)).toBe('degraded');
		expect(sectionState({ ...capabilityFor('degraded')!, state: 'inert' }, some)).toBe('inert');
	});

	it('is empty only when the newest run holds nothing and nothing waits', () => {
		const fx = fixtureFor('empty');
		expect(sectionState(fx.capability, fx.measures)).toBe('empty');
		const waiting = { ...fx.capability!, detail: '3 files waiting to be split' };
		expect(sectionState(waiting, fx.measures)).toBe('steady');
	});

	it('hides bands 2 to 5 for never-run and empty', () => {
		expect(showsBands('off-never')).toBe(false);
		expect(showsBands('empty')).toBe(false);
		for (const state of [
			'off-history',
			'first-pass',
			'steady',
			'degraded',
			'inert',
			'unknown'
		] as const) {
			expect(showsBands(state)).toBe(true);
		}
	});
});

describe('measure definitions', () => {
	it('judges every count and share as lower is better, and the rest as neutral', () => {
		const lower = MEASURES.filter((m) => m.better === 'lower').map((m) => m.key);
		expect(lower).toEqual([
			'statements_current',
			'subjects_shared',
			'conflicts_open',
			'guidance_chars',
			'misgrouped_share',
			'split_subject_share'
		]);
		expect(MEASURES.filter((m) => !m.better).map((m) => m.key)).toEqual([
			'subjects',
			'grouping_distance'
		]);
	});

	it('formats counts, shares and the threshold', () => {
		const by = (key: string) => MEASURES.find((m) => m.key === key)!;
		expect(by('statements_current').format(1912)).toBe('1,912');
		expect(by('misgrouped_share').format(0.042)).toBe('4.2%');
		expect(by('grouping_distance').format(0.7)).toBe('0.70');
	});
});

describe('row actions', () => {
	it('offers Undo, Write to file or Try again by application, and nothing else', () => {
		const byId = (id: number) => CHANGES.find((c) => c.id === id)!;
		expect(actionFor(byId(960))).toMatchObject({ kind: 'undo', label: 'Undo' });
		expect(actionFor(byId(959))).toMatchObject({ kind: 'apply', label: 'Write to file' });
		expect(actionFor(byId(958))).toMatchObject({ kind: 'apply', label: 'Try again' });
		expect(actionFor(byId(957))).toBeNull();
		expect(actionFor(byId(956))).toBeNull();
	});
});

describe('conflicts', () => {
	it('names what settles a lesson pair and a file pair', () => {
		expect(settlesLine(CONFLICTS[0]!)).toMatch(/^Both lessons were stated/);
		expect(settlesLine(CONFLICTS[1]!)).toMatch(/^Both files were written/);
	});

	it('collects both statement ids of every open conflict', () => {
		expect([...conflictedStatementIds(CONFLICTS)].sort()).toEqual([8101, 8102, 8201, 8202]);
	});
});

describe('formatWhen', () => {
	it('gives the clock today and the day otherwise', () => {
		const now = Date.parse('2026-11-20T12:00:00');
		expect(formatWhen('2026-11-20T09:05:00', now)).toBe('09:05');
		expect(formatWhen('2026-11-18T09:05:00', now)).toBe('18 Nov');
		expect(formatWhen(null, now)).toBe('—');
	});
});
