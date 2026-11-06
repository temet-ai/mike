import { describe, expect, it } from 'vitest';
import {
	FLOW_SESSION_ID,
	FLOW_START_MS as start,
	flowActivity,
	flowActor,
	flowCoverage,
	flowTime
} from './__fixtures__/sessionFlow';
import { normalizeFlowDataset } from './projection';
import { LINEAR_TIME_SCALE } from './timeScale';
import {
	buildTransportSteps,
	nextTransportStepMs,
	previousTransportStepMs,
	TRANSPORT_QUANTUM_MS
} from './transport';
import type { FlowBounds, FlowDataset } from './types';

const COORDINATOR_ID = 'coordinator';
const TURN_GAP_MS = 10_000;
const TURN_COUNT = 12;

function coordinatorTurnAtMs(index: number): number {
	return start + index * TURN_GAP_MS;
}

function markerFreeDataset(): FlowDataset {
	const activities = [];
	for (let i = 0; i < TURN_COUNT; i++) {
		const offset = i * TURN_GAP_MS;
		activities.push(
			flowActivity(`turn-${i}`, {
				actor_id: COORDINATOR_ID,
				occurred_at: flowTime(offset),
				recorded_at: flowTime(offset),
				ended_at: flowTime(offset + 2000)
			})
		);
	}
	activities.push(
		flowActivity('turn-0-duplicate', {
			actor_id: COORDINATOR_ID,
			occurred_at: flowTime(0),
			recorded_at: flowTime(0),
			ended_at: flowTime(1000)
		})
	);
	activities.push(
		flowActivity('worker-request', {
			actor_id: 'worker-a',
			occurred_at: flowTime(TURN_GAP_MS / 2),
			recorded_at: flowTime(TURN_GAP_MS / 2),
			ended_at: flowTime(TURN_GAP_MS / 2 + 500)
		})
	);
	activities.push(
		flowActivity('sidecar-request', {
			actor_id: 'sidecar-a',
			occurred_at: flowTime(TURN_GAP_MS * 1.5),
			recorded_at: flowTime(TURN_GAP_MS * 1.5),
			ended_at: flowTime(TURN_GAP_MS * 1.5 + 500)
		})
	);
	activities.push(
		flowActivity('mike-request', {
			actor_id: 'mike-a',
			occurred_at: flowTime(TURN_GAP_MS * 2.5),
			recorded_at: flowTime(TURN_GAP_MS * 2.5),
			ended_at: flowTime(TURN_GAP_MS * 2.5 + 500)
		})
	);
	activities.push(
		flowActivity('coordinator-observation', {
			actor_id: COORDINATOR_ID,
			time_semantics: 'recorded_observation',
			occurred_at: flowTime(TURN_GAP_MS * 3.5),
			recorded_at: flowTime(TURN_GAP_MS * 3.5),
			ended_at: null
		})
	);
	return normalizeFlowDataset({
		sessionId: FLOW_SESSION_ID,
		actors: [
			flowActor(COORDINATOR_ID, { kind: 'session', session_id: FLOW_SESSION_ID, label: 'Session' }),
			flowActor('worker-a', { kind: 'worker' }),
			flowActor('sidecar-a', { kind: 'sidecar' }),
			flowActor('mike-a', { kind: 'mike', label: 'Filters' })
		],
		activities,
		coverage: [flowCoverage()]
	});
}

describe('buildTransportSteps', () => {
	it('returns exactly the coordinator turn starts, ascending, ignoring other roles and de-duplicating', () => {
		const steps = buildTransportSteps(markerFreeDataset());
		expect(steps.length).toBe(TURN_COUNT);
		expect([...steps]).toEqual(
			Array.from({ length: TURN_COUNT }, (_, i) => coordinatorTurnAtMs(i))
		);
		for (let i = 1; i < steps.length; i++) expect(steps[i]).toBeGreaterThan(steps[i - 1]!);
	});
});

describe('fast-forward / rewind walk the coordinator turn chain', () => {
	it('ten fast-forwards from the loaded start visit ten distinct turns in order, never the loaded end', () => {
		const dataset = markerFreeDataset(),
			steps = buildTransportSteps(dataset),
			bounds = dataset.bounds!;
		expect(bounds.startMs).toBe(steps[0]);

		const visited: number[] = [];
		let atMs: number | null = bounds.startMs;
		for (let i = 0; i < 10; i++) {
			atMs = nextTransportStepMs(steps, atMs, bounds, LINEAR_TIME_SCALE, 1);
			expect(atMs).not.toBeNull();
			visited.push(atMs!);
		}

		expect(new Set(visited).size).toBe(10);
		for (let i = 1; i < visited.length; i++) expect(visited[i]).toBeGreaterThan(visited[i - 1]!);
		expect(visited).toEqual(steps.slice(1, 11));
		for (const atStep of visited) expect(atStep).not.toBe(bounds.endMs);
	});

	it('rewind walks the same chain back down to the loaded start', () => {
		const dataset = markerFreeDataset(),
			steps = buildTransportSteps(dataset),
			bounds = dataset.bounds!;

		const visited: number[] = [];
		let atMs: number | null = steps[10]!;
		for (let i = 0; i < 10; i++) {
			atMs = previousTransportStepMs(steps, atMs, bounds, LINEAR_TIME_SCALE, 1);
			expect(atMs).not.toBeNull();
			visited.push(atMs!);
		}

		const firstTen = Array.from({ length: 10 }, (_, i) => steps[i]!);
		expect(visited).toEqual([...firstTen].reverse());
		expect(visited[visited.length - 1]).toBe(bounds.startMs);
	});
});

describe('quantum fallback past either end of the turn chain', () => {
	it('fast-forward past the last step advances by the quantum in warped time, scaled by speed', () => {
		const bounds: FlowBounds = { startMs: 0, endMs: 1_000_000 };
		expect(nextTransportStepMs([], 0, bounds, LINEAR_TIME_SCALE, 1)).toBe(TRANSPORT_QUANTUM_MS);
		expect(nextTransportStepMs([], 0, bounds, LINEAR_TIME_SCALE, 10)).toBe(
			TRANSPORT_QUANTUM_MS * 10
		);
	});

	it('fast-forward fallback clamps to bounds.endMs rather than overshooting', () => {
		const bounds: FlowBounds = { startMs: 0, endMs: 200_000 };
		expect(nextTransportStepMs([], 190_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(bounds.endMs);
	});

	it('rewind past the first step retreats by the quantum in warped time, scaled by speed', () => {
		const bounds: FlowBounds = { startMs: 0, endMs: 1_000_000 };
		expect(previousTransportStepMs([], 500_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(
			500_000 - TRANSPORT_QUANTUM_MS
		);
		expect(previousTransportStepMs([], 500_000, bounds, LINEAR_TIME_SCALE, 10)).toBe(
			500_000 - TRANSPORT_QUANTUM_MS * 10
		);
	});

	it('rewind fallback clamps to bounds.startMs rather than undershooting', () => {
		const bounds: FlowBounds = { startMs: 100_000, endMs: 1_000_000 };
		expect(previousTransportStepMs([], 110_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(bounds.startMs);
	});

	it('already at or past bounds.endMs, fast-forward does not overshoot', () => {
		const bounds: FlowBounds = { startMs: 0, endMs: 100_000 };
		expect(nextTransportStepMs([50_000], 100_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(bounds.endMs);
		expect(nextTransportStepMs([50_000], 150_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(bounds.endMs);
	});

	it('already at or before bounds.startMs, rewind does not undershoot', () => {
		const bounds: FlowBounds = { startMs: 100_000, endMs: 200_000 };
		expect(previousTransportStepMs([150_000], 100_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(
			bounds.startMs
		);
		expect(previousTransportStepMs([150_000], 50_000, bounds, LINEAR_TIME_SCALE, 1)).toBe(
			bounds.startMs
		);
	});
});

describe('a brushed bounds narrower than the dataset excludes steps outside it', () => {
	it('fast-forward and rewind only see turns inside the brush', () => {
		const dataset = markerFreeDataset(),
			steps = buildTransportSteps(dataset);
		const brush: FlowBounds = {
			startMs: coordinatorTurnAtMs(2) - 1,
			endMs: coordinatorTurnAtMs(9) + 1
		};

		expect(nextTransportStepMs(steps, brush.startMs, brush, LINEAR_TIME_SCALE, 1)).toBe(
			coordinatorTurnAtMs(2)
		);
		expect(previousTransportStepMs(steps, brush.endMs, brush, LINEAR_TIME_SCALE, 1)).toBe(
			coordinatorTurnAtMs(9)
		);
		expect(nextTransportStepMs(steps, coordinatorTurnAtMs(9), brush, LINEAR_TIME_SCALE, 1)).toBe(
			brush.endMs
		);
		expect(
			previousTransportStepMs(steps, coordinatorTurnAtMs(2), brush, LINEAR_TIME_SCALE, 1)
		).toBe(brush.startMs);
	});
});

describe('unset playhead and unset bounds', () => {
	it('atMs === null goes to the bounds edge in the direction of travel', () => {
		const bounds: FlowBounds = { startMs: 1000, endMs: 9000 };
		expect(nextTransportStepMs([5000], null, bounds, LINEAR_TIME_SCALE, 1)).toBe(bounds.endMs);
		expect(previousTransportStepMs([5000], null, bounds, LINEAR_TIME_SCALE, 1)).toBe(
			bounds.startMs
		);
	});

	it('bounds === null yields no destination at all', () => {
		expect(nextTransportStepMs([5000], 4000, null, LINEAR_TIME_SCALE, 1)).toBeNull();
		expect(previousTransportStepMs([5000], 4000, null, LINEAR_TIME_SCALE, 1)).toBeNull();
		expect(nextTransportStepMs([], null, null, LINEAR_TIME_SCALE, 1)).toBeNull();
		expect(previousTransportStepMs([], null, null, LINEAR_TIME_SCALE, 1)).toBeNull();
	});
});
