import { describe, expect, it } from 'vitest';
import {
	FLEET_A1_REQUEST_ACTIVITY_ID,
	FLEET_A2_ACTOR_ID,
	FLEET_CALLER_TOOL_ACTOR_ID,
	FLEET_COORDINATOR_ID,
	FLEET_PEER_ID,
	fleetDataset
} from './__fixtures__/fleet';
import { connectedSet, REQUEST_PATH_CHIPS, RESPONSE_PATH_CHIPS } from './focus';
import { rolesOf } from './roles';

describe('session-flow focus', () => {
	it('connects an agent to itself, the coordinator, the model, and the chip that targeted it', () => {
		const dataset = fleetDataset();
		const { keys, chipKeys } = connectedSet(dataset, FLEET_A2_ACTOR_ID);

		expect(keys.has(FLEET_A2_ACTOR_ID)).toBe(true);
		expect(keys.has(FLEET_COORDINATOR_ID)).toBe(true);
		expect(keys.has('upstream')).toBe(true);
		expect(keys.has('terminal')).toBe(false);

		for (const chip of [...REQUEST_PATH_CHIPS, ...RESPONSE_PATH_CHIPS])
			expect(chipKeys.has(chip)).toBe(true);
		expect(chipKeys.has('verifier')).toBe(true);
		expect(chipKeys.has('judge')).toBe(false);
	});

	it("connects an agent to the tools it calls, keyed by the caller's own request", () => {
		const dataset = fleetDataset();
		const requestActivity = dataset.activities.get(FLEET_A1_REQUEST_ACTIVITY_ID)!;
		const { keys } = connectedSet(dataset, requestActivity.actor_id);
		expect(keys.has(FLEET_CALLER_TOOL_ACTOR_ID)).toBe(true);
	});

	it('connects the coordinator to the terminal, the peer, and every agent', () => {
		const dataset = fleetDataset();
		const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
		const { keys } = connectedSet(dataset, FLEET_COORDINATOR_ID);

		expect(keys.has('terminal')).toBe(true);
		expect(keys.has('upstream')).toBe(true);
		expect(keys.has(FLEET_PEER_ID)).toBe(true);
		for (const actor of dataset.actors.values())
			if (roles.get(actor.id) === 'agent') expect(keys.has(actor.id)).toBe(true);
	});
});
