import { describe, expect, it } from 'vitest';
import {
	FLOW_START_MS as start,
	flowActivity,
	flowActor,
	flowCoverage,
	flowDataset,
	flowTime
} from './__fixtures__/sessionFlow';
import { flowTimestamp, normalizeFlowDataset, preferFlowActivity, sceneAt } from './projection';

describe('session-flow projection', () => {
	it('produces the same scene for direct, incremental and backward seeks', () => {
		const dataset = flowDataset();
		const direct = sceneAt(dataset, start + 1000);
		for (const offset of [0, 500, 1000, 4000, 2500]) sceneAt(dataset, start + offset);
		expect(sceneAt(dataset, start + 1000)).toEqual(direct);
		expect(direct.openCaptureIds).toEqual(new Set(['a', 'b']));
	});
	it('gates completion phases, interval ends, outcomes and terminal metadata', () => {
		const initial = flowActivity('a', {
			ended_at: flowTime(2000),
			lifecycle: 'completed',
			outcome: 'error'
		});
		initial.metadata.status_code = 500;
		const dataset = flowDataset([
			initial,
			flowActivity('done', {
				occurred_at: flowTime(2000),
				time_semantics: 'recorded_observation',
				lifecycle: 'completed',
				outcome: 'error'
			})
		]);
		const before = sceneAt(dataset, start + 1999);
		expect(before.activities.map(({ id }) => id)).toEqual(['a']);
		expect(before.activities[0]).toMatchObject({
			ended_at: null,
			endedAtMs: null,
			outcome: null,
			lifecycle: 'started',
			metadata: { status_code: null }
		});
		expect(before.outcomes.size).toBe(0);
		expect(sceneAt(dataset, start + 2000).openCaptureIds.size).toBe(0);
		expect(sceneAt(dataset, start + 2000).outcomes.size).toBe(2);
	});
	it('does not treat a current terminal state without transition as historical completion', () => {
		const scene = sceneAt(
			flowDataset([flowActivity('a', { lifecycle: 'completed', outcome: 'success' })]),
			start + 10000
		);
		expect(scene.outcomes.size).toBe(0);
		expect(scene.openCaptureIds.has('a')).toBe(true);
	});
	it('preserves unknown occurrence while placing recorded output evidence', () => {
		const dataset = flowDataset([
			flowActivity('output', {
				occurred_at: null,
				recorded_at: flowTime(2000),
				time_semantics: 'stored_output'
			}),
			flowActivity('unknown', { occurred_at: null })
		]);
		expect(sceneAt(dataset, start + 1999).activities).toEqual([]);
		expect(sceneAt(dataset, start + 2000).activities[0]).toMatchObject({
			id: 'output',
			occurredAtMs: null,
			atMs: start + 2000,
			timeIssues: ['unknown_occurrence']
		});
		expect(sceneAt(dataset, start).unknownActivityIds).toEqual(['unknown']);
	});
	it('retains invalid and reversed intervals as anomalous evidence', () => {
		const dataset = flowDataset([
			flowActivity('bad', { occurred_at: 'bad' }),
			flowActivity('reversed', { ended_at: flowTime(-1) }),
			flowActivity('invalid-end', { ended_at: 'bad' })
		]);
		expect(dataset.activities.get('bad')?.timeIssues).toContain('invalid_occurrence');
		expect(sceneAt(dataset, start).openCaptureIds.size).toBe(0);
		expect(sceneAt(dataset, start).activities).toHaveLength(2);
	});
	it('does not reveal future actors from topology or future relationships', () => {
		const dataset = normalizeFlowDataset({
			sessionId: 's',
			actors: [flowActor(), flowActor('future')],
			activities: [
				flowActivity(),
				flowActivity('later', { actor_id: 'future', occurred_at: flowTime(2000) })
			],
			relationships: [
				{
					id: 'rel',
					from_id: 'worker-a',
					to_id: 'future',
					kind: 'message_recipient',
					phase: 'sent',
					source: flowActivity().source,
					occurred_at: flowTime(2000),
					recorded_at: flowTime(2000)
				}
			]
		});
		expect(sceneAt(dataset, start).actors.map(({ id }) => id)).toEqual(['worker-a']);
		expect(sceneAt(dataset, start).relationships).toEqual([]);
		expect(sceneAt(dataset, start + 2000).actors).toHaveLength(2);
	});
	it('preserves concurrent captures and report-followed-by-work without termination inference', () => {
		const dataset = flowDataset([
			flowActivity('a'),
			flowActivity('b'),
			flowActivity('report', {
				time_semantics: 'recorded_observation',
				occurred_at: flowTime(1000),
				lifecycle: 'delivered'
			}),
			flowActivity('followup', { occurred_at: flowTime(2000) })
		]);
		expect(sceneAt(dataset, start + 3000).openCaptureIds).toEqual(new Set(['a', 'b', 'followup']));
	});
	it('late closure changes active counts only from the supported end onward', () => {
		const before = flowDataset([flowActivity('a')]);
		const after = flowDataset([flowActivity('a', { ended_at: flowTime(2000) })]);
		expect(sceneAt(before, start + 1000)).toEqual(sceneAt(after, start + 1000));
		expect(sceneAt(after, start + 2000).openCaptureIds.size).toBe(0);
	});
	it('keeps large IDs and source phases distinct while deduplicating IDs', () => {
		const a = flowActivity(),
			b = flowActivity('requests:9007199254740993:completed', {
				time_semantics: 'recorded_observation'
			});
		const dataset = flowDataset([a, b, a]);
		expect(dataset.activities.size).toBe(2);
		expect(dataset.activities.get(a.id)?.source.row_id).toBe('9007199254740993');
	});
	it('compares revisions only through source-supported ordering and keeps equal revisions idempotent', () => {
		const newer = flowActivity('a', { source_revision: '10' }),
			older = flowActivity('a', { source_revision: '9' });
		const compare = (_table: string, incoming: string, previous: string) =>
			Number(BigInt(incoming) - BigInt(previous));
		expect(preferFlowActivity(newer, older, compare)).toBe(newer);
		expect(preferFlowActivity(newer, { ...newer, ended_at: flowTime(2) }, compare)).toBe(newer);
		expect(preferFlowActivity(newer, older)).toBe(older);
	});
	it('reports loaded bounds and preserves partial/unknown coverage independently', () => {
		const dataset = normalizeFlowDataset({
			sessionId: 's',
			activities: [flowActivity('a', { occurred_at: flowTime(1000) })],
			coverage: [flowCoverage({ exhausted: false, retention: 'partial' })]
		});
		expect(dataset.bounds).toEqual({ startMs: start + 1000, endMs: start + 1000 });
		expect(dataset.observationFrontierMs).toBe(start + 4000);
		expect(sceneAt(dataset, start + 1000).coverage.get('requests')).toMatchObject({
			exhausted: false,
			retention: 'partial',
			capture_complete: null
		});
		expect(flowDataset([]).bounds).toBeNull();
	});
	it('rejects non-finite playheads and timestamps without zones', () => {
		expect(() => sceneAt(flowDataset(), NaN)).toThrow(RangeError);
		expect(flowTimestamp('2026-09-10T12:00:00')).toBeNull();
	});
});
