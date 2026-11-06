import { describe, expect, it } from 'vitest';
import {
	FLEET_COORDINATOR_ID,
	FLEET_MAILBOX_ID,
	FLEET_PEER_ID,
	FLEET_TOOL_ACTOR_IDS,
	FLEET_UPSTREAM_ID,
	fleetDataset
} from './__fixtures__/fleet';
import { identityOf, nodeKindOf, roleOf, rolesOf } from './roles';

describe('session-flow roles', () => {
	it('assigns every fleet actor exactly one role', () => {
		const dataset = fleetDataset();
		const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
		expect(roles.size).toBe(dataset.actors.size);
		for (const actor of dataset.actors.values()) expect(roles.has(actor.id)).toBe(true);
	});

	it('classifies the coordinator, agents, peer and sidecars', () => {
		const dataset = fleetDataset();
		expect(roleOf(dataset.actors.get(FLEET_COORDINATOR_ID)!, dataset.sessionId)).toBe(
			'coordinator'
		);
		expect(roleOf(dataset.actors.get(FLEET_PEER_ID)!, dataset.sessionId)).toBe('peer');
		for (const actor of dataset.actors.values()) {
			if (actor.kind === 'worker' || actor.kind === 'unresolved_dispatch')
				expect(roleOf(actor, dataset.sessionId)).toBe('agent');
			if (actor.kind === 'sidecar') expect(roleOf(actor, dataset.sessionId)).toBe('sidecars');
		}
	});

	it('classifies local tools, Mike MCP and other MCP servers by name', () => {
		const dataset = fleetDataset();
		expect(roleOf(dataset.actors.get(FLEET_TOOL_ACTOR_IDS.shell[0])!, dataset.sessionId)).toBe(
			'local-tools'
		);
		expect(roleOf(dataset.actors.get(FLEET_TOOL_ACTOR_IDS.mikeMcp)!, dataset.sessionId)).toBe(
			'mike-mcp'
		);
		expect(roleOf(dataset.actors.get(FLEET_TOOL_ACTOR_IDS.githubMcp)!, dataset.sessionId)).toBe(
			'other-mcp:github'
		);
	});

	it('classifies the mailbox actor and upstream', () => {
		const dataset = fleetDataset();
		expect(roleOf(dataset.actors.get(FLEET_MAILBOX_ID)!, dataset.sessionId)).toBe(
			'service:mailbox'
		);
		expect(roleOf(dataset.actors.get(FLEET_UPSTREAM_ID)!, dataset.sessionId)).toBe('upstream');
	});

	it('maps every role to exactly one node kind and identity family', () => {
		expect(nodeKindOf('coordinator')).toBe('session');
		expect(nodeKindOf('agent')).toBe('agent');
		expect(nodeKindOf('sidecars')).toBe('group');
		expect(nodeKindOf('local-tools')).toBe('tool');
		expect(nodeKindOf('mike-mcp')).toBe('tool');
		expect(nodeKindOf('other-mcp:github')).toBe('tool');
		expect(nodeKindOf('service:mailbox')).toBe('service');
		expect(nodeKindOf('upstream')).toBe('outside');
		expect(nodeKindOf('terminal')).toBe('terminal');

		expect(identityOf('coordinator')).toBe('session');
		expect(identityOf('agent')).toBe('agent');
		expect(identityOf('sidecars')).toBe('agent');
		expect(identityOf('local-tools')).toBe('tool');
		expect(identityOf('service:mailbox')).toBe('mike');
		expect(identityOf('upstream')).toBe('outside');
		expect(identityOf('terminal')).toBe('terminal');
	});
});
