import { describe, expect, it } from 'vitest';
import {
	buildTimeScale,
	buildTimeScaleFromPeriods,
	GAP_MIN_MS,
	GAP_SHOWN_MS,
	LINEAR_TIME_SCALE
} from './timeScale';
import { flowTimeToX, flowXToTime } from './timeline';
import { normalizeFlowDataset } from './projection';

const MIN = 60_000;

describe('time scale — idle stretches are compressed, work is not', () => {
	it('is linear when no gap exceeds the threshold', () => {
		const scale = buildTimeScaleFromPeriods([
			[0, 10 * MIN],
			[11 * MIN, 20 * MIN]
		]);
		expect(scale).toBe(LINEAR_TIME_SCALE);
		expect(scale.warp(15 * MIN)).toBe(15 * MIN);
	});

	it('shows a long gap as GAP_SHOWN_MS and keeps warp continuous and invertible', () => {
		const scale = buildTimeScaleFromPeriods([
			[0, 10 * MIN],
			[3 * 60 * MIN, 3 * 60 * MIN + 10 * MIN]
		]);
		expect(scale.gaps).toHaveLength(1);
		expect(scale.warp(10 * MIN)).toBe(10 * MIN);
		expect(scale.warp(3 * 60 * MIN)).toBe(10 * MIN + GAP_SHOWN_MS);
		expect(scale.warp(3 * 60 * MIN + 10 * MIN)).toBe(20 * MIN + GAP_SHOWN_MS);
		expect(scale.warp(3 * 60 * MIN - 1)).toBeCloseTo(10 * MIN + GAP_SHOWN_MS, 0);
		for (const ms of [0, 5 * MIN, 10 * MIN, 60 * MIN, 3 * 60 * MIN, 3 * 60 * MIN + 7 * MIN])
			expect(scale.unwarp(scale.warp(ms))).toBeCloseTo(ms, 6);
		expect(scale.gaps[0]!.endMs - scale.gaps[0]!.startMs).toBeGreaterThan(GAP_MIN_MS);
	});

	it('positions a track through the same scale in both directions', () => {
		const scale = buildTimeScaleFromPeriods([
			[0, 10 * MIN],
			[3 * 60 * MIN, 3 * 60 * MIN + 10 * MIN]
		]);
		const range = { startMs: 0, endMs: 3 * 60 * MIN + 10 * MIN };
		const width = 1000;
		const secondStart = flowTimeToX(3 * 60 * MIN, range, width, scale);
		expect(secondStart).toBeGreaterThan(480);
		expect(secondStart).toBeLessThan(520);
		for (const ms of [0, 4 * MIN, 3 * 60 * MIN + 2 * MIN])
			expect(flowXToTime(flowTimeToX(ms, range, width, scale), range, width, scale)).toBeCloseTo(
				ms,
				3
			);
	});

	it('builds from a dataset and memoizes on it', () => {
		const dataset = normalizeFlowDataset({
			sessionId: 's',
			actors: [
				{ id: 'a', kind: 'session', label: 'a', parent_actor_id: null, session_id: 's' } as never
			],
			activities: [
				activity('1', '2026-09-15T08:00:00Z', '2026-09-15T08:05:00Z'),
				activity('2', '2026-09-15T11:00:00Z', null)
			]
		});
		const scale = buildTimeScale(dataset);
		expect(scale.gaps).toHaveLength(1);
		expect(buildTimeScale(dataset)).toBe(scale);
	});
});

function activity(id: string, occurredAt: string, endedAt: string | null): never {
	return {
		id,
		actor_id: 'a',
		kind: 'requests',
		phase: 'started',
		occurred_at: occurredAt,
		recorded_at: occurredAt,
		ended_at: endedAt,
		time_semantics: 'capture_interval',
		outcome: null,
		source: { table: 'requests', row_id: id },
		revision: '1'
	} as never;
}
