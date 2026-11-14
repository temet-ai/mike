import { describe, expect, it } from 'vitest';
import { hasSeatRecord, seatDecisionFigures, wholePercents } from './seatDecisions';
import type { FleetRecord, FleetSeatDecisionRow } from './types';

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

function decision(
	id: number,
	point: string,
	applied: string | null,
	over: Partial<FleetSeatDecisionRow> = {}
): FleetSeatDecisionRow {
	return {
		id,
		created_at: `2026-11-14T10:0${id}:00Z`,
		seat: 'P1',
		point,
		applied,
		fail_open: false,
		verdict: { verdict: { score: 0.4 }, error: null },
		...over
	};
}

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
		mail_wake_counts: { by_seat: { P1: { held: 7, sent: 12 } } },
		seat_decisions: {
			by_seat: {
				P1: {
					fleet_stop_check: [
						decision(2, 'fleet_stop_check', 'pushed_back'),
						decision(1, 'fleet_stop_check', 'let_through'),
						decision(3, 'fleet_stop_check', null, { fail_open: true })
					],
					fleet_commit_message: [decision(4, 'fleet_commit_message', 'warned')]
				}
			},
			check_triage: [
				{
					id: 9,
					finished_at: '2026-11-14T10:09:00Z',
					exit_code: 1,
					askers: ['P1', 'P2'],
					triage: 'fix the code'
				},
				{
					id: 8,
					finished_at: '2026-11-14T10:08:00Z',
					exit_code: 1,
					askers: ['P2'],
					triage: 'retry'
				}
			]
		}
	};

	it('reads one seat out of the live or stored record', () => {
		const figures = seatDecisionFigures(record, 'P1');
		expect(figures?.effort).toEqual({ work: 620, machinery: 210, waiting: 0 });
		expect(figures?.mail).toEqual({ held: 7, woken: 12 });
	});

	it('words stop checks and orders them newest first', () => {
		const figures = seatDecisionFigures(record, 'P1');
		expect(figures?.stopChecks.map((s) => [s.id, s.result])).toEqual([
			[3, 'fell back'],
			[2, 'pushed back'],
			[1, 'let through']
		]);
		expect(figures?.stopChecks[1]?.score).toBe(0.4);
	});

	it('keeps only the triage lines the seat asked for', () => {
		expect(seatDecisionFigures(record, 'P1')?.triage.map((t) => t.line)).toEqual(['fix the code']);
		expect(seatDecisionFigures(record, 'P2')?.triage.map((t) => t.line)).toEqual([
			'fix the code',
			'retry'
		]);
	});

	it('reports commit-message warnings', () => {
		expect(seatDecisionFigures(record, 'P1')?.commitMessages[0]?.result).toBe('warned');
	});

	it('gives zeros and empty lists for a seat the record has not seen', () => {
		const figures = seatDecisionFigures(record, 'P9');
		expect(figures?.mail).toEqual({ held: 0, woken: 0 });
		expect(figures?.stopChecks).toEqual([]);
		expect(seatDecisionFigures(record, null)).toBeNull();
		expect(seatDecisionFigures(null, 'P1')).toBeNull();
	});

	it('knows whether the record mentions the seat at all', () => {
		expect(hasSeatRecord(record, 'P1')).toBe(true);
		expect(hasSeatRecord(record, 'P2')).toBe(true);
		expect(hasSeatRecord(record, 'P9')).toBe(false);
		expect(hasSeatRecord(null, 'P1')).toBe(false);
	});
});
