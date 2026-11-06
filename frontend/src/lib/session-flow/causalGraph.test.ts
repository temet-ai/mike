import { describe, expect, it } from 'vitest';
import type { SessionFlowRelationship } from '$lib/api/generated/SessionFlowRelationship';
import type { SessionFlowRelationshipKind } from '$lib/api/generated/SessionFlowRelationshipKind';
import {
	CAUSAL_RELATIONSHIP_KINDS,
	DISPLAY_RELATIONSHIP_KINDS,
	buildCausalGraph,
	causalAncestors,
	causalDependents,
	causalExchanges,
	isCausalRelationshipKind
} from './causalGraph';
import { normalizeFlowDataset } from './projection';
import {
	FLOW_START_MS as start,
	flowActivity,
	flowActor,
	flowTime
} from './__fixtures__/sessionFlow';
import type { FlowDataset } from './types';

const ALL_RELATIONSHIP_KINDS: readonly SessionFlowRelationshipKind[] = [
	'dispatch_tool',
	'result_carrier',
	'message_sender',
	'message_recipient',
	'message_reply',
	'explicit_parent',
	'request_tool',
	'activity_request'
];

function relationship(overrides: Partial<SessionFlowRelationship> = {}): SessionFlowRelationship {
	return {
		id: 'rel',
		from_id: 'a',
		to_id: 'b',
		kind: 'dispatch_tool',
		source: flowActivity().source,
		phase: 'observed',
		occurred_at: flowTime(1000),
		recorded_at: flowTime(1000),
		...overrides
	};
}

function dataset(
	relationships: readonly SessionFlowRelationship[],
	actorIds: readonly string[] = ['a', 'b', 'c', 'd']
): FlowDataset {
	return normalizeFlowDataset({
		sessionId: 's',
		actors: actorIds.map((id) => flowActor(id, { label: id })),
		activities: [],
		relationships
	});
}

describe('session-flow causal relationship partition', () => {
	it('exhaustively and disjointly partitions every relationship kind into causal or display', () => {
		for (const kind of ALL_RELATIONSHIP_KINDS) {
			expect(CAUSAL_RELATIONSHIP_KINDS.has(kind) !== DISPLAY_RELATIONSHIP_KINDS.has(kind)).toBe(
				true
			);
			expect(isCausalRelationshipKind(kind)).toBe(CAUSAL_RELATIONSHIP_KINDS.has(kind));
		}
		expect(CAUSAL_RELATIONSHIP_KINDS.size + DISPLAY_RELATIONSHIP_KINDS.size).toBe(
			ALL_RELATIONSHIP_KINDS.length
		);
	});
	it('excludes explicit_parent (display/grouping) from causal traversal even though it names endpoints', () => {
		const ds = normalizeFlowDataset({
			sessionId: 's',
			actors: [
				flowActor('a', { label: 'a' }),
				flowActor('b', { label: 'b', parent_actor_id: 'a' })
			],
			activities: [],
			relationships: [relationship({ kind: 'explicit_parent' })]
		});
		const graph = buildCausalGraph(ds, start + 1000);
		expect(graph.edges).toEqual([]);
		expect(graph.displayEdges).toHaveLength(1);
		expect(causalDependents(ds, graph, 'a').actorIds).toEqual([]);
		expect(causalAncestors(ds, graph, 'b').actorIds).toEqual([]);
	});
});

describe('session-flow causal graph playhead gating', () => {
	it('withholds a future relationship until its own phase time, then admits it', () => {
		const ds = dataset([
			relationship({ occurred_at: flowTime(2000), recorded_at: flowTime(2000) })
		]);
		const early = buildCausalGraph(ds, start + 1000);
		expect(early.edges).toEqual([]);
		expect(early.futureEdgeIds).toEqual(['rel']);
		const late = buildCausalGraph(ds, start + 2000);
		expect(late.edges).toHaveLength(1);
		expect(late.futureEdgeIds).toEqual([]);
	});
	it('retains a relationship with no resolvable time as unknown-time evidence, never as a visible edge', () => {
		const ds = dataset([relationship({ occurred_at: 'not-a-timestamp', recorded_at: 'also-bad' })]);
		const graph = buildCausalGraph(ds, start + 5000);
		expect(graph.edges).toEqual([]);
		expect(graph.unknownTimeEdgeIds).toEqual(['rel']);
	});
	it('rejects a non-finite playhead', () => {
		expect(() => buildCausalGraph(dataset([]), NaN)).toThrow(RangeError);
	});
});

describe('session-flow causal traversal direction', () => {
	it('walks ancestors backward and dependents forward from a single directed edge', () => {
		const ds = dataset([relationship()]);
		const graph = buildCausalGraph(ds, start + 1000);
		expect(causalDependents(ds, graph, 'a').actorIds).toEqual(['b']);
		expect(causalAncestors(ds, graph, 'a').actorIds).toEqual([]);
		expect(causalAncestors(ds, graph, 'b').actorIds).toEqual(['a']);
		expect(causalDependents(ds, graph, 'b').actorIds).toEqual([]);
	});
	it('explains each included edge with kind, source and observation time', () => {
		const ds = dataset([relationship({ kind: 'result_carrier', phase: 'delivered' })]);
		const graph = buildCausalGraph(ds, start + 1000);
		const result = causalDependents(ds, graph, 'a');
		expect(result.edges).toEqual([
			{
				edge: {
					relationshipId: 'rel',
					kind: 'result_carrier',
					fromId: 'a',
					toId: 'b',
					source: relationship().source,
					phase: 'delivered',
					atMs: start + 1000
				},
				actorId: 'b',
				actorResolved: true,
				direction: 'outgoing',
				depth: 1
			}
		]);
	});
	it('traverses parallel unrelated work without pulling in disconnected actors', () => {
		const ds = dataset([relationship({ id: 'ab', from_id: 'a', to_id: 'b' })]);
		const graph = buildCausalGraph(ds, start + 1000);
		expect(causalDependents(ds, graph, 'c').actorIds).toEqual([]);
		expect(causalAncestors(ds, graph, 'd').actorIds).toEqual([]);
	});
});

describe('session-flow causal traversal cycles and self-links', () => {
	it('handles a cycle without infinite looping, reporting every edge exactly once as evidence', () => {
		const ds = dataset([
			relationship({ id: 'ab', from_id: 'a', to_id: 'b', occurred_at: flowTime(1000) }),
			relationship({ id: 'ba', from_id: 'b', to_id: 'a', occurred_at: flowTime(2000) })
		]);
		const graph = buildCausalGraph(ds, start + 2000);
		const result = causalDependents(ds, graph, 'a');
		expect(result.actorIds).toEqual(['b']);
		expect(result.edges.map((entry) => entry.edge.relationshipId)).toEqual(['ab', 'ba']);
		expect(result.truncated).toBe(false);
	});
	it('records a self-link as evidence without treating the actor as its own neighbor', () => {
		const ds = dataset([relationship({ from_id: 'a', to_id: 'a', kind: 'activity_request' })]);
		const graph = buildCausalGraph(ds, start + 1000);
		const dependents = causalDependents(ds, graph, 'a');
		expect(dependents.actorIds).toEqual([]);
		expect(dependents.edges).toHaveLength(1);
		expect(dependents.edges[0]).toMatchObject({ actorId: 'a', actorResolved: true });
	});
});

describe('session-flow causal traversal missing and deleted endpoints', () => {
	it('reports a null endpoint (e.g. an unaddressed broadcast) as unresolved without expanding it', () => {
		const ds = dataset([relationship({ to_id: null, kind: 'message_sender' })]);
		const graph = buildCausalGraph(ds, start + 1000);
		const result = causalDependents(ds, graph, 'a');
		expect(result.actorIds).toEqual([]);
		expect(result.edges).toEqual([
			expect.objectContaining({ actorId: null, actorResolved: false, direction: 'outgoing' })
		]);
	});
	it('reports an endpoint naming a deleted/missing actor as unresolved without expanding it', () => {
		const ds = dataset([relationship({ to_id: 'ghost' })], ['a', 'b']);
		const graph = buildCausalGraph(ds, start + 1000);
		const result = causalDependents(ds, graph, 'a');
		expect(result.actorIds).toEqual([]);
		expect(result.edges).toEqual([
			expect.objectContaining({ actorId: 'ghost', actorResolved: false })
		]);
	});
});

describe('session-flow causal traversal ordering and bounds', () => {
	it('orders same-time phases by a stable relationship-id tiebreak', () => {
		const ds = dataset([
			relationship({ id: 'z', from_id: 'a', to_id: 'c', occurred_at: flowTime(1000) }),
			relationship({ id: 'm', from_id: 'a', to_id: 'b', occurred_at: flowTime(1000) })
		]);
		const graph = buildCausalGraph(ds, start + 1000);
		expect(graph.edges.map((edge) => edge.relationshipId)).toEqual(['m', 'z']);
	});
	it('truncates a long causal chain at maxActors and reports the limit honestly', () => {
		const ds = dataset(
			[
				relationship({ id: 'ab', from_id: 'a', to_id: 'b', occurred_at: flowTime(1000) }),
				relationship({ id: 'bc', from_id: 'b', to_id: 'c', occurred_at: flowTime(2000) }),
				relationship({ id: 'cd', from_id: 'c', to_id: 'd', occurred_at: flowTime(3000) })
			],
			['a', 'b', 'c', 'd']
		);
		const graph = buildCausalGraph(ds, start + 3000);
		const result = causalDependents(ds, graph, 'a', { maxActors: 1 });
		expect(result.actorIds).toEqual(['b']);
		expect(result.truncated).toBe(true);
		const untruncated = causalDependents(ds, graph, 'a', { maxActors: 10 });
		expect(untruncated.actorIds).toEqual(['b', 'c', 'd']);
		expect(untruncated.truncated).toBe(false);
	});
});

describe('session-flow causal exchanges (bounded peer scope)', () => {
	it('reports only direct peers, both directions, and does not traverse transitively', () => {
		const ds = dataset(
			[
				relationship({ id: 'ab', from_id: 'a', to_id: 'b', kind: 'message_sender' }),
				relationship({ id: 'ca', from_id: 'c', to_id: 'a', kind: 'message_reply' }),
				relationship({ id: 'bd', from_id: 'b', to_id: 'd', kind: 'dispatch_tool' })
			],
			['a', 'b', 'c', 'd']
		);
		const graph = buildCausalGraph(ds, start + 1000);
		const result = causalExchanges(ds, graph, 'a');
		expect(result.actorIds.slice().sort()).toEqual(['b', 'c']);
		expect(result.edges.map((entry) => entry.edge.relationshipId).sort()).toEqual(['ab', 'ca']);
	});
	it('bounds peer scope with maxActors and reports truncation', () => {
		const ds = dataset(
			[
				relationship({ id: 'ab', from_id: 'a', to_id: 'b' }),
				relationship({ id: 'ac', from_id: 'a', to_id: 'c' })
			],
			['a', 'b', 'c']
		);
		const graph = buildCausalGraph(ds, start + 1000);
		const result = causalExchanges(ds, graph, 'a', { maxActors: 1 });
		expect(result.actorIds).toEqual(['b']);
		expect(result.truncated).toBe(true);
	});
});
