import type { SessionFlowRelationshipKind } from '$lib/api/generated/SessionFlowRelationshipKind';
import type { SessionFlowSource } from '$lib/api/generated/SessionFlowSource';
import { compareFlowIds } from './intervalIndex';
import type { FlowDataset, FlowRelationship } from './types';

export const CAUSAL_RELATIONSHIP_KINDS: ReadonlySet<SessionFlowRelationshipKind> = new Set([
	'dispatch_tool',
	'result_carrier',
	'message_sender',
	'message_recipient',
	'message_reply',
	'request_tool',
	'activity_request'
]);

export const DISPLAY_RELATIONSHIP_KINDS: ReadonlySet<SessionFlowRelationshipKind> = new Set([
	'explicit_parent'
]);

export function isCausalRelationshipKind(kind: SessionFlowRelationshipKind): boolean {
	return CAUSAL_RELATIONSHIP_KINDS.has(kind);
}

export interface CausalEdge {
	relationshipId: string;
	kind: SessionFlowRelationshipKind;
	fromId: string | null;
	toId: string | null;
	source: SessionFlowSource;
	phase: string;
	atMs: number;
}

export type CausalDirection = 'outgoing' | 'incoming';

export interface CausalTraversalEdge {
	edge: CausalEdge;
	actorId: string | null;
	actorResolved: boolean;
	direction: CausalDirection;
	depth: number;
}

export interface CausalTraversalResult {
	actorIds: readonly string[];
	edges: readonly CausalTraversalEdge[];
	truncated: boolean;
}

export interface CausalGraph {
	atMs: number;
	edges: readonly CausalEdge[];
	displayEdges: readonly CausalEdge[];
	futureEdgeIds: readonly string[];
	unknownTimeEdgeIds: readonly string[];
}

const DEFAULT_MAX_ACTORS = 500;

function compareRelationships(a: FlowRelationship, b: FlowRelationship): number {
	return (a.atMs ?? 0) - (b.atMs ?? 0) || compareFlowIds(a.id, b.id);
}

function toCausalEdge(relationship: FlowRelationship): CausalEdge {
	return {
		relationshipId: relationship.id,
		kind: relationship.kind,
		fromId: relationship.from_id,
		toId: relationship.to_id,
		source: relationship.source,
		phase: relationship.phase,
		atMs: relationship.atMs!
	};
}

export function buildCausalGraph(dataset: FlowDataset, atMs: number): CausalGraph {
	if (!Number.isFinite(atMs)) throw new RangeError('Causal graph playhead must be finite');
	const ordered = [...dataset.relationships.values()].sort(compareRelationships);
	const edges: CausalEdge[] = [],
		displayEdges: CausalEdge[] = [],
		futureEdgeIds: string[] = [],
		unknownTimeEdgeIds: string[] = [];
	for (const relationship of ordered) {
		const causal = isCausalRelationshipKind(relationship.kind);
		if (relationship.atMs === null) {
			if (causal) unknownTimeEdgeIds.push(relationship.id);
			continue;
		}
		if (relationship.atMs > atMs) {
			if (causal) futureEdgeIds.push(relationship.id);
			continue;
		}
		(causal ? edges : displayEdges).push(toCausalEdge(relationship));
	}
	return {
		atMs,
		edges,
		displayEdges,
		futureEdgeIds: futureEdgeIds.sort(compareFlowIds),
		unknownTimeEdgeIds: unknownTimeEdgeIds.sort(compareFlowIds)
	};
}

interface Adjacency {
	outgoing: ReadonlyMap<string, readonly CausalEdge[]>;
	incoming: ReadonlyMap<string, readonly CausalEdge[]>;
}

function buildAdjacency(edges: readonly CausalEdge[]): Adjacency {
	const outgoing = new Map<string, CausalEdge[]>(),
		incoming = new Map<string, CausalEdge[]>();
	for (const edge of edges) {
		if (edge.fromId !== null) {
			const list = outgoing.get(edge.fromId);
			if (list) list.push(edge);
			else outgoing.set(edge.fromId, [edge]);
		}
		if (edge.toId !== null) {
			const list = incoming.get(edge.toId);
			if (list) list.push(edge);
			else incoming.set(edge.toId, [edge]);
		}
	}
	return { outgoing, incoming };
}

function traverseCausal(
	dataset: FlowDataset,
	adjacency: Adjacency,
	startActorId: string,
	direction: CausalDirection,
	maxActors: number
): CausalTraversalResult {
	const lookup = direction === 'outgoing' ? adjacency.outgoing : adjacency.incoming;
	const neighborOf = (edge: CausalEdge): string | null =>
		direction === 'outgoing' ? edge.toId : edge.fromId;
	const visited = new Set<string>([startActorId]);
	const actorIds: string[] = [];
	const edges: CausalTraversalEdge[] = [];
	const queue: { actorId: string; depth: number }[] = [{ actorId: startActorId, depth: 0 }];
	let truncated = false;
	while (queue.length) {
		const { actorId, depth } = queue.shift()!;
		for (const edge of lookup.get(actorId) ?? []) {
			const neighborId = neighborOf(edge);
			const resolved = neighborId !== null && dataset.actors.has(neighborId);
			edges.push({
				edge,
				actorId: neighborId,
				actorResolved: resolved,
				direction,
				depth: depth + 1
			});
			if (neighborId === null || visited.has(neighborId)) continue;
			if (!resolved) {
				visited.add(neighborId);
				continue;
			}
			if (actorIds.length >= maxActors) {
				truncated = true;
				continue;
			}
			visited.add(neighborId);
			actorIds.push(neighborId);
			queue.push({ actorId: neighborId, depth: depth + 1 });
		}
	}
	return { actorIds, edges, truncated };
}

export function causalAncestors(
	dataset: FlowDataset,
	graph: CausalGraph,
	actorId: string,
	options: { maxActors?: number } = {}
): CausalTraversalResult {
	return traverseCausal(
		dataset,
		buildAdjacency(graph.edges),
		actorId,
		'incoming',
		options.maxActors ?? DEFAULT_MAX_ACTORS
	);
}

export function causalDependents(
	dataset: FlowDataset,
	graph: CausalGraph,
	actorId: string,
	options: { maxActors?: number } = {}
): CausalTraversalResult {
	return traverseCausal(
		dataset,
		buildAdjacency(graph.edges),
		actorId,
		'outgoing',
		options.maxActors ?? DEFAULT_MAX_ACTORS
	);
}

export function causalExchanges(
	dataset: FlowDataset,
	graph: CausalGraph,
	actorId: string,
	options: { maxActors?: number } = {}
): CausalTraversalResult {
	const maxActors = options.maxActors ?? DEFAULT_MAX_ACTORS;
	const adjacency = buildAdjacency(graph.edges);
	const combined: { edge: CausalEdge; direction: CausalDirection; neighborId: string | null }[] = [
		...(adjacency.outgoing.get(actorId) ?? []).map((edge) => ({
			edge,
			direction: 'outgoing' as const,
			neighborId: edge.toId
		})),
		...(adjacency.incoming.get(actorId) ?? []).map((edge) => ({
			edge,
			direction: 'incoming' as const,
			neighborId: edge.fromId
		}))
	].sort(
		(a, b) =>
			a.edge.atMs - b.edge.atMs || compareFlowIds(a.edge.relationshipId, b.edge.relationshipId)
	);
	const visited = new Set<string>([actorId]);
	const actorIds: string[] = [];
	const edges: CausalTraversalEdge[] = [];
	let truncated = false;
	for (const { edge, direction, neighborId } of combined) {
		const resolved = neighborId !== null && dataset.actors.has(neighborId);
		edges.push({ edge, actorId: neighborId, actorResolved: resolved, direction, depth: 1 });
		if (neighborId === null || visited.has(neighborId)) continue;
		if (!resolved) {
			visited.add(neighborId);
			continue;
		}
		if (actorIds.length >= maxActors) {
			truncated = true;
			continue;
		}
		visited.add(neighborId);
		actorIds.push(neighborId);
	}
	return { actorIds, edges, truncated };
}
