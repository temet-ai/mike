import { describe, expect, it } from 'vitest';
import { seatDecisionFigures, wholePercents } from './seatDecisions';
import type { FleetRecord } from './types';

describe('wholePercents', () => {
	it('always sums to 100 using the largest remainder', () => {
		expect(wholePercents([1, 1, 1])).toEqual([34, 33, 33]);
		expect(wholePercents([620, 210, 170]).reduce((a, b) => a + b, 0)).toBe(100);
		expect(wholePercents([620, 210, 170])).toEqual([62, 21, 17]);
	});

	it('is all zero without tokens', () => {
		expect(wholePercents([0, 0, 0])).toEqual([0, 0, 0]);
	});
});

describe('seatDecisionFigures', () => {
	const record: FleetRecord = {
		live: true,
		effort_split: {
			by_seat: {
				P1: {
					tokens: 1000,
					labels: {
						work: { requests: 4, tokens: 620, share_percent: 62 },
						machinery: { requests: 2, tokens: 210, share_percent: 21 }
					}
				}
			}
		},
		mail_wake_counts: { by_seat: { P1: { held: 7, sent: 12 } } }
	};

	it('reads one seat out of the live or stored record', () => {
		expect(seatDecisionFigures(record, 'P1')).toEqual({
			effort: { work: 620, machinery: 210, waiting: 0 },
			mail: { held: 7, woken: 12 }
		});
	});

	it('gives zeros for a seat the record has not seen and null without a seat', () => {
		expect(seatDecisionFigures(record, 'P9')?.mail).toEqual({ held: 0, woken: 0 });
		expect(seatDecisionFigures(record, null)).toBeNull();
		expect(seatDecisionFigures(null, 'P1')).toBeNull();
	});
});
