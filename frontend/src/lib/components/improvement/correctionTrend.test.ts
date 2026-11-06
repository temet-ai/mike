import { describe, expect, it } from 'vitest';
import { judgeName, toTrendPoints } from '$lib/api/improvement';
import { TREND_POINT_DTOS } from '$lib/test-support/improvementFixtures';
import { lastDayLabel, weekIndexOf, weekLabel, xTickIndices, yTicks } from './correctionTrend';

describe('trend judges', () => {
	it('names judges in plain words and drops labels with no judge', () => {
		expect(judgeName({ label: 'bundle:14', bundle_id: 14, count: 1 })).toBe('configuration 14');
		expect(
			judgeName({ label: 'legacy:moonshotai/kimi-k3:r1-2026-06-22', bundle_id: null, count: 1 })
		).toBe('moonshotai/kimi-k3');
		expect(judgeName({ label: 'legacy:none:none', bundle_id: null, count: 1 })).toBeNull();
	});

	it('flags the weeks whose main judges changed, ignoring a judge under a tenth of the labels', () => {
		const points = toTrendPoints(TREND_POINT_DTOS);
		const changed = points
			.map((p, i) => ({ i, started: p.judgesStarted, stopped: p.judgesStopped }))
			.filter((c) => c.started.length + c.stopped.length > 0);
		expect(changed).toEqual([
			{ i: 4, started: ['google/gemini-3-flash-preview'], stopped: ['moonshotai/kimi-k3'] },
			{ i: 6, started: ['configuration 14'], stopped: [] },
			{ i: 7, started: ['configuration 1'], stopped: ['google/gemini-3-flash-preview'] }
		]);
	});

	it('does not compare across an empty week as a change', () => {
		const points = toTrendPoints([
			{
				week_end: '2026-09-15T00:00:00Z',
				k: 1,
				n: 10,
				judges: [{ label: 'bundle:1', bundle_id: 1, count: 10 }]
			},
			{ week_end: '2026-09-22T00:00:00Z', k: 0, n: 0, judges: [] },
			{
				week_end: '2026-09-29T00:00:00Z',
				k: 1,
				n: 10,
				judges: [{ label: 'bundle:1', bundle_id: 1, count: 10 }]
			}
		]);
		expect(points.every((p) => p.judgesStarted.length + p.judgesStopped.length === 0)).toBe(true);
	});
});

describe('trend axes', () => {
	it('labels a week by the seven days before its midnight end', () => {
		expect(weekLabel('2026-09-29T00:00:00Z')).toBe('22–28 Sep');
		expect(weekLabel('2026-09-03T00:00:00Z')).toBe('27 Aug – 2 Sep');
		expect(lastDayLabel('2026-09-29T00:00:00Z')).toBe('28 Sep');
	});

	it('rounds the percent axis up to clean steps', () => {
		expect(yTicks(25.1)).toEqual([0, 10, 20, 30]);
		expect(yTicks(0)).toEqual([0, 10]);
		expect(yTicks(71)).toEqual([0, 20, 40, 60, 80]);
	});

	it('thins the dates on a narrow chart but keeps the latest', () => {
		expect(xTickIndices(8, 600)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
		expect(xTickIndices(8, 200)).toEqual([1, 3, 5, 7]);
	});

	it('places a decision in the week whose days contain it', () => {
		const weekly = toTrendPoints(TREND_POINT_DTOS);
		expect(weekIndexOf(weekly, '2026-09-20T10:00:00Z')).toBe(6);
		expect(weekIndexOf(weekly, '2026-09-29T10:00:00Z')).toBeNull();
	});
});
