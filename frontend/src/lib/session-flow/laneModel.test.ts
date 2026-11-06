import { describe, expect, it } from 'vitest';
import {
	FLEET_A6_ERROR_REQUEST_ID,
	FLEET_LATE_SIDECAR_REQUEST_ID,
	fleetDataset
} from './__fixtures__/fleet';
import { buildLaneModel, LATE_DURATION_MS } from './laneModel';

const WIDTH = 1000;

describe('session-flow laneModel', () => {
	it('yields the five role lanes in order', () => {
		const lanes = buildLaneModel(fleetDataset(), WIDTH);
		expect(lanes.map((lane) => lane.id)).toEqual(['parent', 'agents', 'peer', 'sidecars', 'mike']);
		expect(lanes.map((lane) => lane.label)).toEqual([
			'Parent',
			'Agents',
			'Peer',
			'Sidecars',
			'Mike'
		]);
	});

	it('gives the coordinator, agent and sidecar request intervals, and Mike ticks', () => {
		const lanes = buildLaneModel(fleetDataset(), WIDTH);
		const byId = new Map(lanes.map((lane) => [lane.id, lane]));

		expect(byId.get('parent')!.intervals).toHaveLength(3);
		expect(byId.get('parent')!.ticks).toHaveLength(0);

		expect(byId.get('agents')!.intervals).toHaveLength(22);
		expect(byId.get('agents')!.ticks).toHaveLength(24);

		expect(byId.get('peer')!.intervals).toHaveLength(0);
		expect(byId.get('peer')!.ticks).toHaveLength(10);

		expect(byId.get('sidecars')!.intervals).toHaveLength(62);
		expect(byId.get('sidecars')!.ticks).toHaveLength(0);

		expect(byId.get('mike')!.intervals).toHaveLength(0);
		expect(byId.get('mike')!.ticks).toHaveLength(28);
	});

	it('flags the errored agent request and the late sidecar closure', () => {
		const lanes = buildLaneModel(fleetDataset(), WIDTH);
		const agentInterval = lanes
			.find((lane) => lane.id === 'agents')!
			.intervals.find((interval) => interval.id === FLEET_A6_ERROR_REQUEST_ID)!;
		expect(agentInterval.error).toBe(true);
		expect(agentInterval.late).toBe(false);

		const sidecarInterval = lanes
			.find((lane) => lane.id === 'sidecars')!
			.intervals.find((interval) => interval.id === FLEET_LATE_SIDECAR_REQUEST_ID)!;
		expect(sidecarInterval.late).toBe(true);
		expect(sidecarInterval.error).toBe(false);
		expect(sidecarInterval.endMs! - sidecarInterval.startMs).toBeGreaterThan(LATE_DURATION_MS);

		const otherSidecars = lanes
			.find((lane) => lane.id === 'sidecars')!
			.intervals.filter((interval) => interval.id !== FLEET_LATE_SIDECAR_REQUEST_ID);
		expect(otherSidecars.every((interval) => !interval.late)).toBe(true);
	});

	it('positions every interval and tick within [0, width] and sorts ascending by start', () => {
		const lanes = buildLaneModel(fleetDataset(), WIDTH);
		for (const lane of lanes) {
			for (const interval of lane.intervals) {
				expect(interval.x).toBeGreaterThanOrEqual(0);
				expect(interval.x).toBeLessThanOrEqual(WIDTH);
			}
			for (let i = 1; i < lane.intervals.length; i += 1)
				expect(lane.intervals[i]!.startMs).toBeGreaterThanOrEqual(lane.intervals[i - 1]!.startMs);
			for (let i = 1; i < lane.ticks.length; i += 1)
				expect(lane.ticks[i]!.atMs).toBeGreaterThanOrEqual(lane.ticks[i - 1]!.atMs);
		}
	});
});
