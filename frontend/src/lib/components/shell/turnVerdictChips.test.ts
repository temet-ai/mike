import { describe, expect, it } from 'vitest';
import { verdictSource, sourceTip, reactionTip, verdictHistoryHref } from './turnVerdictChips';

describe('verdictSource', () => {
	it('names the projection source', () => {
		expect(verdictSource({ outcome: 'corrected', outcome_source: 'judge' })).toBe('judge');
		expect(verdictSource({ outcome: 'accepted', outcome_source: 'operator' })).toBe('operator');
		expect(verdictSource({ outcome: 'unknown', outcome_source: 'no_verdict' })).toBe('no verdict');
		expect(verdictSource({ outcome: 'abandoned', outcome_source: 'frustration_close' })).toBe(
			'frustration close'
		);
	});

	it('tells a legacy label from a turn still waiting', () => {
		expect(verdictSource({ outcome: 'abandoned', outcome_source: null, legacy: true })).toBe(
			'legacy'
		);
		expect(verdictSource({ outcome: 'unknown', outcome_source: null, pending: true })).toBe(
			'pending'
		);
		expect(verdictSource({ outcome: 'shipped' })).toBe('legacy');
		expect(verdictSource({ outcome: 'unknown' })).toBe('pending');
	});

	it('has a tip for every source and falls back to the code for an unknown axis value', () => {
		expect(sourceTip('operator')).toContain('gold label');
		expect(reactionTip('corrected')).toContain('wrong');
		expect(reactionTip('future')).toBe('future');
		expect(verdictHistoryHref(42)).toBe('/api/operator-turns/42/verdicts');
	});
});
