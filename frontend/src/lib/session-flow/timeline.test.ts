import { describe, expect, it } from 'vitest';
import {
	aggregateFlowTimeline,
	flowKeyboardSeek,
	flowTimeToX,
	flowXToTime,
	normalizeFlowRange
} from './timeline';
import { FLOW_START_MS, flowActivity, flowDataset, flowTime } from './__fixtures__/sessionFlow';

describe('Session Flow native timeline', () => {
	it('maps the common clock and numeric brush in both directions', () => {
		const range = normalizeFlowRange(2000, 1000);
		expect(flowTimeToX(1500, range, 800)).toBe(400);
		expect(flowXToTime(400, range, 800)).toBe(1500);
		expect(flowXToTime(-100, range, 800)).toBe(1000);
		expect(flowTimeToX(1000, { startMs: 1000, endMs: 1000 }, 800)).toBe(400);
	});
	it('supports keyboard seeking without changing the range', () => {
		const range = { startMs: 0, endMs: 1000 };
		expect(flowKeyboardSeek('ArrowRight', 900, range, 200)).toBe(1000);
		expect(flowKeyboardSeek('ArrowLeft', 100, range, 200)).toBe(0);
		expect(flowKeyboardSeek('Home', 500, range, 100)).toBe(0);
		expect(flowKeyboardSeek('End', 500, range, 100)).toBe(1000);
		expect(flowKeyboardSeek('Escape', 500, range, 100)).toBeNull();
	});
	it('aggregates 10000 events while preserving collapsed and expanded counts', () => {
		const activities = Array.from({ length: 10000 }, (_, i) =>
			flowActivity(`activity-${i}`, {
				actor_id: `worker-${i % 32}`,
				occurred_at: flowTime(i),
				ended_at: flowTime(i + 10)
			})
		);
		const dataset = flowDataset(activities);
		const range = { startMs: FLOW_START_MS, endMs: FLOW_START_MS + 10010 };
		const collapsed = aggregateFlowTimeline(
			dataset.activities.values(),
			range,
			800,
			() => 'workers'
		);
		const expanded = aggregateFlowTimeline(dataset.activities.values(), range, 800);
		expect(collapsed[0]!.total).toBe(10000);
		expect(collapsed[0]!.marks.reduce((sum, mark) => sum + mark.count, 0)).toBe(10000);
		expect(collapsed[0]!.marks.length).toBeLessThanOrEqual(66);
		expect(expanded.reduce((sum, lane) => sum + lane.inWindow, 0)).toBe(10000);
	});
	it('distinguishes unknown bounds, open starts, points and offscreen evidence', () => {
		const dataset = flowDataset([
			flowActivity('open'),
			flowActivity('point', { ended_at: flowTime(0) }),
			flowActivity('unknown', { occurred_at: null }),
			flowActivity('future', { occurred_at: flowTime(5000) })
		]);
		const [lane] = aggregateFlowTimeline(
			dataset.activities.values(),
			{ startMs: FLOW_START_MS, endMs: FLOW_START_MS + 1000 },
			800
		);
		expect([lane!.total, lane!.inWindow, lane!.unknown]).toEqual([4, 2, 1]);
		expect(lane!.marks.map((mark) => mark.kind).sort()).toEqual(['open', 'point']);
	});
	it('rejects nonfinite mapping and seek values', () => {
		expect(() => normalizeFlowRange(NaN, 0)).toThrow(RangeError);
		expect(() => flowTimeToX(0, { startMs: 0, endMs: 1 }, 0)).toThrow(RangeError);
		expect(() => flowKeyboardSeek('Home', 0, { startMs: 0, endMs: 1 }, -1)).toThrow(RangeError);
	});
});
