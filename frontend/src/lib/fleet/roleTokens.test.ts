import { describe, expect, it } from 'vitest';
import { fromGraphResponse, type CanvasNode } from './graphNormalize';
import { roleTokens } from './roleTokens';
import type { FleetGraphNode } from './types';

function graphNode(overrides: Partial<FleetGraphNode> = {}): FleetGraphNode {
	return {
		session_id: '7d77c5c5-0000-4000-8000-000000000000',
		title: null,
		agent_type: null,
		role: null,
		model: 'claude-opus-5',
		workspace: null,
		cwd: null,
		worktree: null,
		goal: null,
		next_steps: null,
		state: 'working',
		last_activity_at: '2026-08-20T10:00:00Z',
		request_count: 42,
		...overrides
	};
}

describe('roleTokens', () => {
	it('reports role unknown for a coordinator with dispatches but no declared role', () => {
		const tokens = roleTokens(null);
		expect(tokens.label).toBe('role unknown');
	});

	it('uses the declared role when there is one', () => {
		expect(roleTokens('coordinator').label).toBe('coordinator');
	});

	it('gives the unknown case the muted tokens, never a hashed identity slot', () => {
		const unknown = roleTokens(null);
		const declared = roleTokens('rust-developer');
		expect(unknown.fg).toBe('var(--status-muted-fg)');
		expect(declared.fg).not.toBe(unknown.fg);
	});
});

function normalizeOne(overrides: Partial<FleetGraphNode>): CanvasNode {
	const { nodes } = fromGraphResponse([graphNode(overrides)], []);
	const node = nodes[0];
	if (node === undefined) throw new Error('fromGraphResponse dropped the node');
	return node;
}

describe('fromGraphResponse identity label', () => {
	it('does not name a coordinator after the subagent it dispatched', () => {
		const node = normalizeOne({ agent_type: 'frontend-developer' });
		expect(node.label).not.toBe('frontend-developer');
		expect(node.label).toBe('7d77c5c5');
		expect(node.hasIdentityLabel).toBe(false);
		expect(node.agentType).toBe('frontend-developer');
	});

	it('prefers the wire title, then the session’s own declared goal', () => {
		const titled = normalizeOne({
			title: 'Fix the retry backoff',
			goal: 'ship #102',
			agent_type: 'db-dev'
		});
		expect(titled.label).toBe('Fix the retry backoff');
		expect(titled.hasIdentityLabel).toBe(true);

		const goalOnly = normalizeOne({ goal: 'ship #102', agent_type: 'db-dev' });
		expect(goalOnly.label).toBe('ship #102');
		expect(goalOnly.hasIdentityLabel).toBe(true);
	});
});
