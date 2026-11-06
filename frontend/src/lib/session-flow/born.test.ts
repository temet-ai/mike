import { describe, expect, it } from 'vitest';
import {
	FLEET_A1_HEADER_WORKER_ID,
	FLEET_A1_INSTANCE_WORKER_ID,
	FLEET_COORDINATOR_ID,
	FLEET_PEER_ID,
	fleetDataset
} from './__fixtures__/fleet';
import { FLOW_START_MS } from './__fixtures__/sessionFlow';
import { bornAtMs, compareBornAtMs } from './born';

describe('session-flow born', () => {
	it("takes the min of the dispatch observed time and the worker's own first started activity", () => {
		const dataset = fleetDataset();
		expect(bornAtMs(dataset, FLEET_A1_INSTANCE_WORKER_ID)).toBe(FLOW_START_MS + 0);
		expect(bornAtMs(dataset, FLEET_A1_HEADER_WORKER_ID)).toBe(FLOW_START_MS + 1_000);
	});

	it('is the first activity of any phase for the coordinator and the peer', () => {
		const dataset = fleetDataset();
		expect(bornAtMs(dataset, FLEET_COORDINATOR_ID)).toBe(FLOW_START_MS + 0);
		expect(bornAtMs(dataset, FLEET_PEER_ID)).toBe(FLOW_START_MS + 20_500);
	});

	it('is null for an actor absent from the dataset', () => {
		expect(bornAtMs(fleetDataset(), 'no-such-actor')).toBeNull();
	});

	it('orders ascending by born time, unborn last, with a stable tie-break', () => {
		const dataset = fleetDataset();
		const compare = compareBornAtMs(dataset);
		expect(compare(FLEET_COORDINATOR_ID, FLEET_PEER_ID)).toBeLessThan(0);
		expect(compare('no-such-actor', FLEET_COORDINATOR_ID)).toBeGreaterThan(0);
		expect(compare('no-such-actor', 'no-such-actor')).toBe(0);
	});
});
