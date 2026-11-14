import { describe, expect, it } from 'vitest';
import type { DecisionPointRow, PointScore } from '$lib/api/decisions';
import {
	failOpenPercent,
	formatSpend,
	groupTotals,
	hasHistory,
	matchesFilter,
	rightCell
} from './calibration';

function score(over: Partial<PointScore> = {}, calls = 10): PointScore {
	return {
		counts: {
			point: 'x',
			calls,
			applied_on: 4,
			fail_open: 1,
			schema_errors: 0,
			refused: 0,
			question_refusals: 0,
			cost_usd: 0.0123,
			latency_p50_ms: 400,
			latency_p95_ms: 900,
			labelled: 8,
			labelled_last_7d: 3
		},
		auroc: 0.8,
		held_labelled: 0,
		held_wrong: 0,
		held_precision_lower: null,
		sent_labelled: 0,
		sent_right: 0,
		labels_per_day: 1,
		days_to_min_labels: null,
		applied_labelled: 8,
		applied_right: 6,
		applied_precision_lower: 0.4,
		findings: [],
		...over
	};
}

function row(point: string, s: PointScore | null): DecisionPointRow {
	return { point, label: null, area: 'judges', mode: 'on', mode_source: 'setting', score: s };
}

describe('calibration helpers', () => {
	it('shows applied right and an interval for ordinary points', () => {
		const cell = rightCell(row('turn_judge', score()));
		expect(cell.counts).toBe('6/8');
		expect(cell.interval).toMatch(/^0\.\d\d–0\.\d\d$/);
		expect(cell.holdSide).toBe(false);
	});

	it('shows the held side for the turn-guard pre-check', () => {
		const cell = rightCell(
			row('turn_guard_precheck', score({ held_labelled: 5, held_wrong: 4, applied_labelled: 0 }))
		);
		expect(cell.counts).toBe('4/5');
		expect(cell.holdSide).toBe(true);
	});

	it('shows a dash with no labelled decisions or no score', () => {
		expect(rightCell(row('a', score({ applied_labelled: 0 }))).counts).toBe('—');
		expect(rightCell(row('a', null)).counts).toBe('—');
	});

	it('computes the fail-open share and spend', () => {
		expect(failOpenPercent(row('a', score()))).toBe('10%');
		expect(failOpenPercent(row('a', score({}, 0)))).toBe('0%');
		expect(formatSpend(0.0123)).toBe('$0.0123');
	});

	it('totals calls and spend and reports history', () => {
		const rows = [row('a', score()), row('b', null)];
		expect(groupTotals(rows)).toEqual({ calls: 10, spend: 0.0123 });
		expect(hasHistory(rows)).toBe(true);
		expect(hasHistory([row('b', null)])).toBe(false);
	});

	it('filters by label and point id', () => {
		expect(matchesFilter(row('turn_judge', null), 'JUDGE')).toBe(true);
		expect(matchesFilter(row('turn_judge', null), 'lesson')).toBe(false);
		expect(matchesFilter(row('turn_judge', null), '  ')).toBe(true);
	});
});
