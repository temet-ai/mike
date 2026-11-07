import { describe, expect, it } from 'vitest';
import { formatShare, queryString, rejudgeSummary } from './evalMetrics';
import type { RejudgeRunDto } from './evalMetrics';

describe('evalMetrics', () => {
	it('builds a sorted query string and skips undefined values', () => {
		expect(
			queryString({
				role: 'turn_judge',
				family: 'accuracy',
				label_tier: undefined,
				latest: true,
				window_days: 0
			})
		).toBe('?family=accuracy&latest=true&role=turn_judge&window_days=0');
		expect(queryString({})).toBe('');
	});

	it('formats a share to one decimal place and a missing value as a dash', () => {
		expect(formatShare(0.0682)).toBe('6.8%');
		expect(formatShare(null)).toBe('—');
	});
});

describe('rejudgeSummary', () => {
	it('says when no run exists and summarises the latest one', () => {
		expect(rejudgeSummary(undefined)).toBe('No history re-judge has been started.');
		const run = {
			id: 3,
			status: 'running',
			judged: 120,
			eligible_total: 3900,
			cost_usd_so_far: 0.52,
			projected_finish_date: '2026-10-03T09:00:00Z'
		} as RejudgeRunDto;
		expect(rejudgeSummary(run)).toBe(
			'Run 3 running: 120 of 3900 judged, $0.52 so far, finishing about 2026-10-03'
		);
	});
});
