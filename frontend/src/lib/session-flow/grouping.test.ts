import { describe, expect, it } from 'vitest';
import {
	FLEET_A1_DISPATCH_ID,
	FLEET_A1_HEADER_WORKER_ID,
	FLEET_A1_INSTANCE_WORKER_ID,
	FLEET_PENDING_DISPATCH_IDS,
	fleetDataset
} from './__fixtures__/fleet';
import { flowActor, flowActivity, FLOW_SESSION_ID } from './__fixtures__/sessionFlow';
import { normalizeFlowDataset } from './projection';
import { bornAtMs } from './born';
import { buildAgentIdentities, buildGroups, toolFamilyOf } from './grouping';

describe('session-flow grouping', () => {
	it('classifies local-tool names into shell/files/browser families', () => {
		expect(toolFamilyOf('Bash')).toBe('shell');
		expect(toolFamilyOf('Read')).toBe('files');
		expect(toolFamilyOf('Edit')).toBe('files');
		expect(toolFamilyOf('Browser')).toBe('browser');
		expect(toolFamilyOf('SomethingElse')).toBe('other');
	});

	it('groups sidecars, tool families and MCP servers with per-member counts', () => {
		const groups = buildGroups(fleetDataset());
		const byKey = new Map(groups.map((group) => [group.key, group]));

		expect(byKey.get('sidecars')?.memberIds).toHaveLength(3);
		expect(byKey.get('tools:shell')?.memberIds).toHaveLength(3);
		expect(byKey.get('tools:files')?.memberIds).toHaveLength(2);
		expect(byKey.get('tools:browser')?.memberIds).toHaveLength(1);
		expect(byKey.get('mike-mcp')?.memberIds).toHaveLength(1);
		expect(byKey.get('other-mcp:github')?.memberIds).toHaveLength(1);

		const sidecarGroup = byKey.get('sidecars')!;
		const totalSidecarActivity = sidecarGroup.memberIds.reduce(
			(sum, id) => sum + (sidecarGroup.counts.get(id) ?? 0),
			0
		);
		expect(totalSidecarActivity).toBe(62);

		expect(byKey.has('agent')).toBe(false);
		expect(byKey.has('peer')).toBe(false);
	});

	it('collapses the dispatch, instance worker and header worker into one agent identity', () => {
		const identities = buildAgentIdentities(fleetDataset());
		const merged = identities.find(
			(identity) => identity.dispatchActorId === FLEET_A1_DISPATCH_ID
		)!;
		expect(merged.pending).toBe(false);
		expect(merged.actorIds).toEqual(
			[FLEET_A1_DISPATCH_ID, FLEET_A1_HEADER_WORKER_ID, FLEET_A1_INSTANCE_WORKER_ID].sort()
		);
		expect([...merged.workerActorIds].sort()).toEqual(
			[FLEET_A1_HEADER_WORKER_ID, FLEET_A1_INSTANCE_WORKER_ID].sort()
		);
	});

	it('leaves an unresolved dispatch alone, labelled PENDING IDENTITY', () => {
		const identities = buildAgentIdentities(fleetDataset());
		for (const dispatchId of FLEET_PENDING_DISPATCH_IDS) {
			const pending = identities.find((identity) => identity.dispatchActorId === dispatchId)!;
			expect(pending).toBeDefined();
			expect(pending.pending).toBe(true);
			expect(pending.label).toBe('PENDING IDENTITY');
			expect(pending.actorIds).toEqual([dispatchId]);
		}
	});

	it('produces exactly 24 agent identities, 2 of them pending, ordered by born time', () => {
		const dataset = fleetDataset();
		const identities = buildAgentIdentities(dataset);
		expect(identities).toHaveLength(24);
		expect(identities.filter((identity) => identity.pending)).toHaveLength(2);

		const bornTimes = identities.map((identity) =>
			bornAtMs(dataset, identity.dispatchActorId ?? identity.workerActorIds[0]!)!
		);
		for (let i = 1; i < bornTimes.length; i += 1)
			expect(bornTimes[i]).toBeGreaterThanOrEqual(bornTimes[i - 1]!);
	});

	it('merges the producer-id worker (no parent, holds the requests) into the instance worker identity', () => {
		const dispatchId = 'unresolved_dispatch:1';
		const instanceWorkerId = 'worker:s1:instance:1';
		const producerWorkerId = 'worker:s1:agent-producer-7';
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [
				flowActor(dispatchId, { kind: 'unresolved_dispatch', producer_id: null }),
				flowActor(instanceWorkerId, {
					producer_id: 'agent-producer-7',
					parent_actor_id: dispatchId
				}),
				flowActor(producerWorkerId, { producer_id: 'agent-producer-7', parent_actor_id: null })
			],
			activities: [
				flowActivity('dispatches:1:started', {
					actor_id: dispatchId,
					kind: 'dispatches',
					source: {
						table: 'dispatches',
						row_id: '1',
						session_id: FLOW_SESSION_ID,
						request_id: null,
						detail_href: null
					}
				}),
				flowActivity('requests:421524:started', {
					actor_id: producerWorkerId,
					kind: 'requests',
					source: {
						table: 'requests',
						row_id: '421524',
						session_id: FLOW_SESSION_ID,
						request_id: '421524',
						detail_href: '/api/requests/421524'
					}
				})
			]
		});

		const identities = buildAgentIdentities(dataset);
		const identity = identities.find((entry) => entry.dispatchActorId === dispatchId)!;
		expect(identity).toBeDefined();
		expect([...identity.workerActorIds].sort()).toEqual(
			[instanceWorkerId, producerWorkerId].sort()
		);
	});
});
